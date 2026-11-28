#![no_std]
#![allow(clippy::duplicated_attributes)]
#![allow(dead_code)]
#![allow(deprecated)]

//! Accreditation renewal reminders.
//!
//! # Bounded reminder sweeps
//!
//! Reminder scans run as *sweeps* that operators drive over several calls, so
//! no single invocation has to touch every registered wallet:
//!
//! 1. `start_reminder_sweep` opens a sweep and snapshots the current
//!    registration count as `total`.
//! 2. `check_accreditation_reminders(sweep_id, cursor, limit)` examines at most
//!    `min(limit, scan_page_limit)` wallets from registration index `cursor`
//!    and returns the `next_cursor` to continue from. Repeat until
//!    `complete` is true.
//!
//! **Membership.** Wallets are stored append-only by registration index. A
//! sweep covers exactly the indices `0..total` captured at start. Wallets
//! registered while a sweep is running get an index `>= total`, are excluded
//! from that sweep, and are covered by the next one. A member whose expiry is
//! updated mid-sweep is evaluated against the expiry stored when its page is
//! scanned.
//!
//! **Retries.** Any cursor up to the sweep's scanned frontier (`next_cursor`)
//! may be replayed. Each wallet's reminder log suppresses intervals that were
//! already sent, so a retried page emits no duplicate reminder events and
//! writes nothing new. Cursors beyond the frontier would skip wallets and are
//! rejected with `InvalidCursor`. Starting a new sweep supersedes the previous
//! one; pages for an older sweep id fail with `StaleSweep`.

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
pub use types::{DataKey, ReminderPage, ReminderRecord, SweepState};

use soroban_sdk::{contract, contractimpl, Address, Env, Vec};

const SECONDS_PER_DAY: u64 = 86400;
const REMINDER_INTERVALS: [u32; 3] = [60, 30, 7];

/// Page limit used until the admin configures one.
pub const DEFAULT_SCAN_PAGE_LIMIT: u32 = 25;
/// Hard ceiling on the configurable page limit.
pub const MAX_SCAN_PAGE_LIMIT: u32 = 100;

#[contract]
pub struct AccreditationRenewalContract;

#[contractimpl]
impl AccreditationRenewalContract {
    pub fn initialize(env: Env, admin: Address, operator: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Operator, &operator);
        Ok(())
    }

    pub fn set_accreditation_expiry(
        env: Env,
        caller: Address,
        wallet: Address,
        expiry_timestamp: u64,
    ) -> Result<(), Error> {
        let admin_addr = Self::admin(&env)?;
        let operator_addr = Self::operator(&env)?;
        caller.require_auth();

        if caller != admin_addr && caller != operator_addr {
            return Err(Error::Unauthorized);
        }

        if expiry_timestamp <= env.ledger().timestamp() {
            return Err(Error::InvalidExpiry);
        }

        let key = DataKey::Expiry(wallet.clone());
        env.storage().persistent().set(&key, &expiry_timestamp);

        let index_key = DataKey::WalletIndex(wallet.clone());
        if !env.storage().persistent().has(&index_key) {
            let index = Self::wallet_count(&env);
            env.storage()
                .persistent()
                .set(&DataKey::WalletAt(index), &wallet);
            env.storage().persistent().set(&index_key, &index);
            env.storage()
                .instance()
                .set(&DataKey::WalletCount, &(index + 1));
        }
        Ok(())
    }

    /// Admin-only: set the maximum number of wallets a single page may examine.
    pub fn set_scan_page_limit(env: Env, caller: Address, limit: u32) -> Result<(), Error> {
        let admin_addr = Self::admin(&env)?;
        caller.require_auth();
        if caller != admin_addr {
            return Err(Error::Unauthorized);
        }
        if limit == 0 || limit > MAX_SCAN_PAGE_LIMIT {
            return Err(Error::InvalidPageLimit);
        }
        env.storage()
            .instance()
            .set(&DataKey::ScanPageLimit, &limit);
        Ok(())
    }

    pub fn get_scan_page_limit(env: Env) -> u32 {
        Self::scan_page_limit(&env)
    }

    pub fn get_wallet_count(env: Env) -> u32 {
        Self::wallet_count(&env)
    }

    /// Operator-only: open a new sweep over every wallet registered so far.
    ///
    /// Supersedes any sweep in progress. Reminder suppression is per wallet,
    /// so abandoning a partial sweep never causes duplicate reminders.
    pub fn start_reminder_sweep(env: Env) -> Result<SweepState, Error> {
        let operator = Self::operator(&env)?;
        operator.require_auth();

        let sweep_id = match Self::sweep(&env) {
            Some(previous) => previous.sweep_id + 1,
            None => 1,
        };
        let total = Self::wallet_count(&env);
        let now = env.ledger().timestamp();
        let state = SweepState {
            sweep_id,
            total,
            next_cursor: 0,
            started_at: now,
            completed: total == 0,
        };
        env.storage().instance().set(&DataKey::Sweep, &state);

        events::reminder_sweep_started(&env, sweep_id, total, now);
        if state.completed {
            events::reminder_sweep_completed(&env, sweep_id, total, now);
        }
        Ok(state)
    }

    pub fn get_reminder_sweep(env: Env) -> Result<SweepState, Error> {
        Self::sweep(&env).ok_or(Error::NoActiveSweep)
    }

    /// Operator-only: examine one bounded page of the current sweep.
    ///
    /// Examines at most `min(limit, scan_page_limit)` wallets starting at
    /// registration index `cursor`. `cursor` may be any value up to the
    /// sweep's `next_cursor`, so failed or repeated pages can be retried.
    pub fn check_accreditation_reminders(
        env: Env,
        sweep_id: u32,
        cursor: u32,
        limit: u32,
    ) -> Result<ReminderPage, Error> {
        let operator = Self::operator(&env)?;
        operator.require_auth();

        let mut state = Self::sweep(&env).ok_or(Error::NoActiveSweep)?;
        if sweep_id != state.sweep_id {
            return Err(Error::StaleSweep);
        }
        if limit == 0 {
            return Err(Error::InvalidPageLimit);
        }
        if cursor > state.next_cursor {
            return Err(Error::InvalidCursor);
        }

        let page_limit = limit.min(Self::scan_page_limit(&env));
        let end = cursor.saturating_add(page_limit).min(state.total);
        let now = env.ledger().timestamp();

        let mut emitted = 0u32;
        for index in cursor..end {
            if let Some(wallet) = env
                .storage()
                .persistent()
                .get::<DataKey, Address>(&DataKey::WalletAt(index))
            {
                emitted += Self::remind_wallet(&env, &wallet, now);
            }
        }

        if end > state.next_cursor {
            state.next_cursor = end;
        }
        let just_completed = !state.completed && state.next_cursor == state.total;
        if just_completed {
            state.completed = true;
        }
        if end > cursor || just_completed {
            env.storage().instance().set(&DataKey::Sweep, &state);
        }
        if just_completed {
            events::reminder_sweep_completed(&env, state.sweep_id, state.total, now);
        }

        Ok(ReminderPage {
            sweep_id: state.sweep_id,
            cursor,
            next_cursor: end,
            total: state.total,
            examined: end - cursor,
            emitted,
            complete: end == state.total,
        })
    }

    pub fn get_reminder_log(env: Env, wallet: Address) -> Vec<ReminderRecord> {
        let log_key = DataKey::ReminderLog(wallet);
        env.storage()
            .persistent()
            .get(&log_key)
            .unwrap_or(Vec::new(&env))
    }

    pub fn get_accreditation_expiry(env: Env, wallet: Address) -> Result<u64, Error> {
        let key = DataKey::Expiry(wallet);
        env.storage()
            .persistent()
            .get(&key)
            .ok_or(Error::WalletNotFound)
    }
}

impl AccreditationRenewalContract {
    fn admin(env: &Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)
    }

    fn operator(env: &Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Operator)
            .ok_or(Error::NotInitialized)
    }

    fn wallet_count(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::WalletCount)
            .unwrap_or(0)
    }

    fn scan_page_limit(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::ScanPageLimit)
            .unwrap_or(DEFAULT_SCAN_PAGE_LIMIT)
    }

    fn sweep(env: &Env) -> Option<SweepState> {
        env.storage().instance().get(&DataKey::Sweep)
    }

    /// Emit any due, not-yet-sent interval reminders for `wallet`.
    ///
    /// Writes the reminder log only when a new interval is recorded, so
    /// re-examining a wallet is a no-op for both storage and events.
    fn remind_wallet(env: &Env, wallet: &Address, now: u64) -> u32 {
        let expiry_timestamp = match env
            .storage()
            .persistent()
            .get::<DataKey, u64>(&DataKey::Expiry(wallet.clone()))
        {
            Some(expiry) if expiry > now => expiry,
            _ => return 0,
        };
        let days_remaining = (expiry_timestamp - now) / SECONDS_PER_DAY;

        let log_key = DataKey::ReminderLog(wallet.clone());
        let mut log: Vec<ReminderRecord> = env
            .storage()
            .persistent()
            .get(&log_key)
            .unwrap_or(Vec::new(env));

        let mut emitted = 0u32;
        for &interval in REMINDER_INTERVALS.iter() {
            if days_remaining > interval as u64 {
                continue;
            }
            if log.iter().any(|record| record.interval_days == interval) {
                continue;
            }
            events::accreditation_renewal_reminder(env, wallet, days_remaining, expiry_timestamp);
            log.push_back(ReminderRecord {
                interval_days: interval,
                timestamp: now,
            });
            emitted += 1;
        }

        if emitted > 0 {
            env.storage().persistent().set(&log_key, &log);
        }
        emitted
    }
}
