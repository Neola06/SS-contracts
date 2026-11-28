//! Platform treasury management contract for StellarSettle.
//!
//! Consolidates protocol fee inflows from every fee-collecting contract,
//! tracks balances per fee source, and lets the admin allocate funds to
//! platform operations, grants, and reserves with a paginated audit log.
//!
//! # Accounting model
//!
//! Lifetime inflows and spendable funds are tracked as separate, reconciled
//! quantities, both treasury-wide and per source:
//!
//! - **received**: cumulative fees accepted by `receive_fee`. Never decreases.
//! - **allocated**: cumulative funds routed out by `allocate`. Never decreases.
//! - **available**: `received - allocated`, the funds that can still be
//!   allocated.
//!
//! `get_treasury_balance` reports availability only, with a per-source
//! breakdown that sums exactly to the available total.
//! `get_treasury_accounting` and `get_source_accounting` report all three
//! figures. Receiving 100 then allocating 40 reports 100 received, 40
//! allocated and 60 available.
//!
//! # Allocation attribution policy
//!
//! Every allocation is debited from source availability in first-receipt
//! order: the source that first contributed fees is drawn down to zero before
//! the next source is touched, and so on through the known-sources list.
//! This is deterministic, needs no rounding, and keeps per-source availability
//! summing exactly to the treasury total. Each `AllocationRecord` stores the
//! resulting `debits`. A rejected allocation writes nothing.

#![no_std]
#![allow(clippy::duplicated_attributes)]
#![allow(dead_code)]
#![allow(
    deprecated,
    unused_variables,
    unused_mut,
    dead_code,
    clippy::too_many_arguments,
    clippy::needless_borrows_for_generic_args,
    clippy::len_zero,
    clippy::collapsible_if,
    clippy::unnecessary_cast,
    clippy::implicit_saturating_sub
)]

// proptest's macros expand to `format!`/`vec!`, which a `no_std` crate
// does not have in scope.
#[cfg(test)]
#[macro_use]
extern crate std;

mod errors;
mod events;
mod storage;
mod types;

pub use types::{
    AllocationRecord, SourceAccounting, SourceDebit, TreasuryAccounting, TreasuryBalance,
};

use soroban_sdk::{contract, contractimpl, Address, Env, String, Symbol, Vec};

use errors::Error;

/// Reject the zero address (all-zero 32-byte Ed25519 key).
fn ensure_non_zero_address(env: &Env, address: &Address) -> Result<(), Error> {
    let zero_str = String::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    );
    let zero = Address::from_string(&zero_str);
    if *address == zero {
        return Err(Error::InvalidAddress);
    }
    Ok(())
}

#[contract]
pub struct TreasuryManagement;

#[contractimpl]
impl TreasuryManagement {
    /// Initialize the contract with the admin allowed to call `allocate`.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        ensure_non_zero_address(&env, &admin)?;
        if storage::get_admin(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        storage::set_admin(&env, &admin);
        storage::set_total_received(&env, 0);
        storage::set_total_allocated(&env, 0);
        events::initialized(&env, &admin);
        Ok(())
    }

    /// Called by any fee-collecting contract to attribute `amount` to
    /// `source`. This contract does not itself move tokens: the caller is
    /// expected to have already transferred `amount` of the protocol's
    /// fee token to this contract's address before calling, matching how
    /// the other fee-collecting contracts in this workspace separate
    /// token movement from balance bookkeeping.
    pub fn receive_fee(env: Env, source: Address, amount: i128) -> Result<(), Error> {
        source.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        storage::get_admin(&env).ok_or(Error::NotInit)?;

        if storage::is_allowlist_enforced(&env) && !storage::is_source_approved(&env, &source) {
            events::source_rejected(&env, &source, amount, &Symbol::new(&env, "unapproved"));
            return Err(Error::SourceNotApproved);
        }

        let prev_cum = storage::get_source_received(&env, &source);
        let new_cum = prev_cum.checked_add(amount).ok_or(Error::Overflow)?;
        if let Some(cap) = storage::get_source_cap(&env, &source) {
            if new_cum > cap {
                events::source_rejected(&env, &source, amount, &Symbol::new(&env, "cap_exceeded"));
                return Err(Error::SourceCapExceeded);
            }
        }

        let ledger_day = env.ledger().timestamp() / 86_400;
        let prev_daily = storage::get_source_daily_total(&env, &source, ledger_day);
        let new_daily = prev_daily.checked_add(amount).ok_or(Error::Overflow)?;
        let total_received = storage::get_total_received(&env)
            .checked_add(amount)
            .ok_or(Error::Overflow)?;

        storage::remember_source(&env, &source);
        storage::set_source_received(&env, &source, new_cum);
        storage::set_source_daily_total(&env, &source, ledger_day, new_daily);
        storage::set_total_received(&env, total_received);

        events::source_fee_received(&env, &source, amount, new_daily, new_cum);
        events::fee_received(&env, &source, amount);
        Ok(())
    }

    /// Admin-only: toggle whether `receive_fee` enforces the approved source allowlist.
    pub fn set_source_allowlist_enforced(
        env: Env,
        admin: Address,
        enforced: bool,
    ) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();
        storage::set_allowlist_enforced(&env, enforced);
        Ok(())
    }

    /// Admin-only: approve or revoke a fee source and optionally set its aggregate cap.
    pub fn set_approved_source(
        env: Env,
        admin: Address,
        source: Address,
        approved: bool,
        cap: Option<i128>,
    ) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();
        ensure_non_zero_address(&env, &source)?;
        if let Some(c) = cap {
            if c <= 0 {
                return Err(Error::InvalidCap);
            }
        }
        storage::set_source_approved(&env, &source, approved);
        storage::set_source_cap(&env, &source, cap);
        Ok(())
    }

    /// View: whether `source` is approved.
    pub fn is_source_approved(env: Env, source: Address) -> bool {
        storage::is_source_approved(&env, &source)
    }

    /// View: optional aggregate cap for `source`.
    pub fn get_source_cap(env: Env, source: Address) -> Option<i128> {
        storage::get_source_cap(&env, &source)
    }

    /// View: `(daily_total, cumulative_total)` for `source` on `ledger_day`.
    pub fn get_source_totals(env: Env, source: Address, ledger_day: u64) -> (i128, i128) {
        (
            storage::get_source_daily_total(&env, &source, ledger_day),
            storage::get_source_received(&env, &source),
        )
    }

    /// Admin-only: route `amount` of treasury funds to `destination` for
    /// `purpose` (e.g. "operations", "grant:Q3-community-fund",
    /// "reserve"). Records the allocation in the paginated history log.
    ///
    /// Like `receive_fee`, this contract tracks the allocation as
    /// bookkeeping; the admin (or a caller the admin authorizes) is
    /// responsible for the actual token transfer to `destination`.
    pub fn allocate(
        env: Env,
        admin: Address,
        destination: Address,
        amount: i128,
        purpose: String,
    ) -> Result<u32, Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();
        ensure_non_zero_address(&env, &destination)?;
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if purpose.is_empty() {
            return Err(Error::InvalidPurpose);
        }

        let total_received = storage::get_total_received(&env);
        let total_allocated = storage::get_total_allocated(&env);
        if amount > total_received - total_allocated {
            return Err(Error::InsufficientBalance);
        }

        let current_ledger = env.ledger().sequence();
        let cooldown = storage::get_allocation_cooldown(&env);
        if cooldown > 0 {
            if let Some(last_ledger) = storage::get_last_allocation_ledger(&env) {
                if current_ledger < last_ledger.saturating_add(cooldown) {
                    return Err(Error::AllocationCooldownActive);
                }
            }
        }

        // Every check is done before the first write, so a rejected
        // allocation leaves balances and history untouched.
        let debits = Self::attribute_allocation(&env, amount)?;
        for debit in debits.iter() {
            let allocated = storage::get_source_allocated(&env, &debit.source);
            storage::set_source_allocated(&env, &debit.source, allocated + debit.amount);
        }
        storage::set_total_allocated(&env, total_allocated + amount);
        storage::set_last_allocation_ledger(&env, current_ledger);

        let record = AllocationRecord {
            destination: destination.clone(),
            amount,
            purpose: purpose.clone(),
            timestamp: env.ledger().timestamp(),
            debits,
        };
        let index = storage::push_allocation(&env, &record);

        if cooldown > 0 {
            events::allocation_cooldown_started(
                &env,
                current_ledger,
                cooldown,
                current_ledger.saturating_add(cooldown),
            );
        }
        events::funds_allocated(&env, &destination, amount, &purpose);
        Ok(index)
    }

    pub fn set_allocation_cooldown(
        env: Env,
        admin: Address,
        cooldown_ledgers: u32,
    ) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();
        storage::set_allocation_cooldown(&env, cooldown_ledgers);
        Ok(())
    }

    pub fn get_allocation_cooldown(env: Env) -> u32 {
        storage::get_allocation_cooldown(&env)
    }

    pub fn get_last_allocation_ledger(env: Env) -> Option<u32> {
        storage::get_last_allocation_ledger(&env)
    }

    pub fn get_remaining_cooldown(env: Env) -> u32 {
        let cooldown = storage::get_allocation_cooldown(&env);
        if cooldown == 0 {
            return 0;
        }
        match storage::get_last_allocation_ledger(&env) {
            Some(last) => last
                .saturating_add(cooldown)
                .saturating_sub(env.ledger().sequence()),
            None => 0,
        }
    }

    /// View: funds currently available to allocate, attributed per source.
    ///
    /// `available_by_source` sums exactly to `available`. This is not a
    /// history of inflows; use `get_treasury_accounting` for lifetime
    /// received and allocated amounts.
    pub fn get_treasury_balance(env: Env) -> TreasuryBalance {
        let mut sources = Vec::new(&env);
        let mut available_by_source = Vec::new(&env);
        for source in storage::get_known_sources(&env).iter() {
            available_by_source.push_back(Self::source_accounting(&env, &source).available);
            sources.push_back(source);
        }
        TreasuryBalance {
            available: storage::get_total_received(&env) - storage::get_total_allocated(&env),
            sources,
            available_by_source,
        }
    }

    /// View: reconciled lifetime receipts, lifetime allocations and current
    /// availability, treasury-wide and for every known source.
    pub fn get_treasury_accounting(env: Env) -> TreasuryAccounting {
        let total_received = storage::get_total_received(&env);
        let total_allocated = storage::get_total_allocated(&env);
        let mut sources = Vec::new(&env);
        for source in storage::get_known_sources(&env).iter() {
            sources.push_back(Self::source_accounting(&env, &source));
        }
        TreasuryAccounting {
            total_received,
            total_allocated,
            available: total_received - total_allocated,
            sources,
        }
    }

    /// View: lifetime receipts, allocations and availability for one source.
    /// Unknown sources report zeros.
    pub fn get_source_accounting(env: Env, source: Address) -> SourceAccounting {
        Self::source_accounting(&env, &source)
    }

    /// View: paginated allocation log, oldest-first within the page.
    /// `offset` is the index of the first record to return; `limit` caps
    /// how many are returned. Returns fewer than `limit` records once the
    /// end of the log is reached, and an empty Vec once `offset` is past
    /// the end.
    pub fn get_allocation_history(env: Env, offset: u32, limit: u32) -> Vec<AllocationRecord> {
        let count = storage::get_allocation_count(&env);
        let mut result = Vec::new(&env);
        if offset >= count {
            return result;
        }
        let end = core::cmp::min(offset.saturating_add(limit), count);
        for index in offset..end {
            if let Some(record) = storage::get_allocation(&env, index) {
                result.push_back(record);
            }
        }
        result
    }

    /// View: total number of allocation records ever written.
    pub fn get_allocation_count(env: Env) -> u32 {
        storage::get_allocation_count(&env)
    }

    /// View: return the current admin.
    pub fn get_admin(env: Env) -> Result<Address, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)
    }
}

impl TreasuryManagement {
    fn source_accounting(env: &Env, source: &Address) -> SourceAccounting {
        let received = storage::get_source_received(env, source);
        let allocated = storage::get_source_allocated(env, source);
        SourceAccounting {
            source: source.clone(),
            received,
            allocated,
            available: received - allocated,
        }
    }

    /// Split `amount` across source availability in first-receipt order
    /// (see the module docs). Read-only; the caller applies the debits.
    fn attribute_allocation(env: &Env, amount: i128) -> Result<Vec<SourceDebit>, Error> {
        let mut debits = Vec::new(env);
        let mut remaining = amount;
        for source in storage::get_known_sources(env).iter() {
            if remaining == 0 {
                break;
            }
            let available = Self::source_accounting(env, &source).available;
            if available <= 0 {
                continue;
            }
            let take = remaining.min(available);
            debits.push_back(SourceDebit {
                source,
                amount: take,
            });
            remaining -= take;
        }
        // Per-source availability always sums to the treasury total, which
        // the caller has already checked covers `amount`.
        if remaining != 0 {
            return Err(Error::InsufficientBalance);
        }
        Ok(debits)
    }
}

#[cfg(test)]
mod test;
#[cfg(test)]
mod test_accounting;
