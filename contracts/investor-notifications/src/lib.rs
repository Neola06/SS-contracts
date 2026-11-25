#![no_std]
#![allow(deprecated)]

mod errors;
mod events;
mod storage;
mod types;

pub use errors::Error;
pub use types::{DeliveryState, NotificationRecord, NotificationStatus, RetryPolicy};

use soroban_sdk::{contract, contractimpl, symbol_short, Address, BytesN, Env, String, Symbol};

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

fn require_initialized(env: &Env) -> Result<Address, Error> {
    storage::get_admin(env).ok_or(Error::NotInitialized)
}

fn require_authorized(env: &Env, caller: &Address) -> Result<(), Error> {
    require_initialized(env)?;
    if !storage::is_authorized_caller(env, caller) {
        return Err(Error::Unauthorized);
    }
    caller.require_auth();
    Ok(())
}

fn require_delivery_provider(env: &Env, provider: &Address) -> Result<(), Error> {
    require_initialized(env)?;
    if !storage::is_delivery_provider(env, provider) {
        return Err(Error::Unauthorized);
    }
    provider.require_auth();
    Ok(())
}

fn effective_state(record: &NotificationRecord, current_ledger: u32) -> DeliveryState {
    match record.state {
        DeliveryState::Delivered => DeliveryState::Delivered,
        DeliveryState::Expired => DeliveryState::Expired,
        DeliveryState::Pending | DeliveryState::Failed => {
            if current_ledger >= record.expires_at_ledger {
                DeliveryState::Expired
            } else {
                record.state
            }
        }
    }
}

#[contract]
pub struct InvestorNotifications;

#[contractimpl]
impl InvestorNotifications {
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        ensure_non_zero_address(&env, &admin)?;
        if storage::get_admin(&env).is_some() {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        storage::set_admin(&env, &admin);
        storage::set_authorized_caller(&env, &admin, true);
        storage::set_retry_policy(
            &env,
            &RetryPolicy {
                max_retries: storage::DEFAULT_MAX_RETRIES,
                retry_delay_ledgers: storage::DEFAULT_RETRY_DELAY_LEDGERS,
                expiry_ledgers: storage::DEFAULT_EXPIRY_LEDGERS,
            },
        );
        Ok(())
    }

    pub fn set_authorized_caller(env: Env, caller: Address, authorized: bool) -> Result<(), Error> {
        let admin = require_initialized(&env)?;
        admin.require_auth();
        ensure_non_zero_address(&env, &caller)?;
        storage::set_authorized_caller(&env, &caller, authorized);
        Ok(())
    }

    /// Admin-only: authorize or revoke an off-chain delivery relayer/provider
    /// permitted to submit delivery receipts and failure reports.
    pub fn set_delivery_provider(
        env: Env,
        provider: Address,
        authorized: bool,
    ) -> Result<(), Error> {
        let admin = require_initialized(&env)?;
        admin.require_auth();
        ensure_non_zero_address(&env, &provider)?;
        storage::set_delivery_provider(&env, &provider, authorized);
        Ok(())
    }

    /// Admin-only: update bounded retry count, retry ledger delay, and expiry
    /// window applied to newly queued notifications.
    pub fn configure_retry_policy(
        env: Env,
        max_retries: u32,
        retry_delay_ledgers: u32,
        expiry_ledgers: u32,
    ) -> Result<(), Error> {
        let admin = require_initialized(&env)?;
        admin.require_auth();
        if expiry_ledgers == 0 {
            return Err(Error::InvalidConfig);
        }
        storage::set_retry_policy(
            &env,
            &RetryPolicy {
                max_retries,
                retry_delay_ledgers,
                expiry_ledgers,
            },
        );
        Ok(())
    }

    /// Authorized caller: create a tracked notification in `Pending` state with
    /// bounded retries and an explicit expiry ledger.
    pub fn queue_notification(
        env: Env,
        caller: Address,
        wallet: Address,
        notification_type: Symbol,
        invoice_id: Option<BytesN<32>>,
        amount: Option<i128>,
    ) -> Result<u64, Error> {
        require_authorized(&env, &caller)?;
        ensure_non_zero_address(&env, &wallet)?;

        let policy = storage::get_retry_policy(&env);
        let current_ledger = env.ledger().sequence();
        let expires_at_ledger = current_ledger
            .checked_add(policy.expiry_ledgers)
            .ok_or(Error::ArithmeticOverflow)?;

        let notification_id = storage::next_notification_id(&env);
        let record = NotificationRecord {
            id: notification_id,
            wallet: wallet.clone(),
            notification_type: notification_type.clone(),
            invoice_id,
            amount,
            state: DeliveryState::Pending,
            retry_count: 0,
            max_retries: policy.max_retries,
            created_ledger: current_ledger,
            next_retry_ledger: Some(current_ledger),
            expires_at_ledger,
            external_ref: None,
            receipt_hash: None,
            delivered_by: None,
            delivered_ledger: None,
            last_error: None,
        };

        storage::set_notification(&env, notification_id, &record);
        events::notification_queued(
            &env,
            notification_id,
            &wallet,
            &notification_type,
            expires_at_ledger,
        );
        Ok(notification_id)
    }

    /// Authorized delivery provider: finalize a notification as `Delivered`
    /// after recording a non-empty, non-replayed external provider reference
    /// and non-zero receipt hash.
    pub fn confirm_delivery(
        env: Env,
        provider: Address,
        notification_id: u64,
        external_ref: String,
        receipt_hash: BytesN<32>,
    ) -> Result<(), Error> {
        require_delivery_provider(&env, &provider)?;

        if external_ref.is_empty() || receipt_hash == BytesN::from_array(&env, &[0u8; 32]) {
            return Err(Error::InvalidReceipt);
        }
        if storage::is_receipt_used(&env, &external_ref) {
            return Err(Error::DuplicateReceipt);
        }

        let mut record =
            storage::get_notification(&env, notification_id).ok_or(Error::NotificationNotFound)?;

        let current_ledger = env.ledger().sequence();
        match record.state {
            DeliveryState::Delivered => return Err(Error::AlreadyDelivered),
            DeliveryState::Expired => return Err(Error::NotificationExpired),
            DeliveryState::Pending | DeliveryState::Failed => {
                if current_ledger >= record.expires_at_ledger {
                    return Err(Error::NotificationExpired);
                }
                if record.retry_count >= record.max_retries && record.state == DeliveryState::Failed
                {
                    return Err(Error::RetryLimitExhausted);
                }
            }
        }

        record.state = DeliveryState::Delivered;
        record.next_retry_ledger = None;
        record.external_ref = Some(external_ref.clone());
        record.receipt_hash = Some(receipt_hash.clone());
        record.delivered_by = Some(provider.clone());
        record.delivered_ledger = Some(current_ledger);

        storage::mark_receipt_used(&env, &external_ref, notification_id);
        storage::set_notification(&env, notification_id, &record);

        events::notification_delivered(
            &env,
            notification_id,
            &provider,
            &external_ref,
            &receipt_hash,
            current_ledger,
        );
        Ok(())
    }

    /// Authorized delivery provider: record a failed delivery attempt.
    ///
    /// Increments `retry_count`, schedules `next_retry_ledger` when below
    /// `max_retries`, or marks retries exhausted and emits
    /// `notification_retry_exhausted` once the bound is reached.
    pub fn record_delivery_failure(
        env: Env,
        provider: Address,
        notification_id: u64,
        reason: Symbol,
    ) -> Result<DeliveryState, Error> {
        require_delivery_provider(&env, &provider)?;

        let mut record =
            storage::get_notification(&env, notification_id).ok_or(Error::NotificationNotFound)?;

        let current_ledger = env.ledger().sequence();
        match record.state {
            DeliveryState::Delivered => return Err(Error::AlreadyDelivered),
            DeliveryState::Expired => return Err(Error::NotificationExpired),
            DeliveryState::Pending | DeliveryState::Failed => {
                if current_ledger >= record.expires_at_ledger {
                    return Err(Error::NotificationExpired);
                }
                if record.retry_count >= record.max_retries {
                    return Err(Error::RetryLimitExhausted);
                }
                if let Some(next_ledger) = record.next_retry_ledger {
                    if current_ledger < next_ledger {
                        return Err(Error::RetryNotReady);
                    }
                }
            }
        }

        let new_retry_count = record
            .retry_count
            .checked_add(1)
            .ok_or(Error::ArithmeticOverflow)?;
        let policy = storage::get_retry_policy(&env);

        record.state = DeliveryState::Failed;
        record.retry_count = new_retry_count;
        record.last_error = Some(reason.clone());

        if new_retry_count >= record.max_retries {
            record.next_retry_ledger = None;
            storage::set_notification(&env, notification_id, &record);
            events::notification_retry_exhausted(&env, notification_id, new_retry_count, &reason);
        } else {
            let next_ledger = current_ledger
                .checked_add(policy.retry_delay_ledgers)
                .ok_or(Error::ArithmeticOverflow)?;
            record.next_retry_ledger = Some(next_ledger);
            storage::set_notification(&env, notification_id, &record);
            events::notification_delivery_failed(
                &env,
                notification_id,
                new_retry_count,
                next_ledger,
                &reason,
            );
        }

        Ok(record.state)
    }

    /// Transition an undelivered notification to the terminal `Expired` state
    /// once the ledger sequence reaches `expires_at_ledger`.
    pub fn expire_notification(env: Env, notification_id: u64) -> Result<(), Error> {
        require_initialized(&env)?;
        let mut record =
            storage::get_notification(&env, notification_id).ok_or(Error::NotificationNotFound)?;

        match record.state {
            DeliveryState::Delivered => return Err(Error::AlreadyDelivered),
            DeliveryState::Expired => return Err(Error::NotificationExpired),
            DeliveryState::Pending | DeliveryState::Failed => {}
        }

        let current_ledger = env.ledger().sequence();
        if current_ledger < record.expires_at_ledger {
            return Err(Error::NotYetExpired);
        }

        record.state = DeliveryState::Expired;
        record.next_retry_ledger = None;
        storage::set_notification(&env, notification_id, &record);
        events::notification_expired(&env, notification_id, current_ledger);
        Ok(())
    }

    pub fn emit_settlement_notification(
        env: Env,
        caller: Address,
        wallet: Address,
        invoice_id: BytesN<32>,
        amount: i128,
    ) -> Result<(), Error> {
        require_authorized(&env, &caller)?;
        env.events().publish(
            (Symbol::new(&env, "settlement"), wallet, invoice_id),
            amount,
        );
        Ok(())
    }

    pub fn emit_kyc_notification(
        env: Env,
        caller: Address,
        wallet: Address,
        status: Symbol,
    ) -> Result<(), Error> {
        require_authorized(&env, &caller)?;
        env.events().publish((symbol_short!("kyc"), wallet), status);
        Ok(())
    }

    pub fn emit_auto_invest_notification(
        env: Env,
        caller: Address,
        wallet: Address,
        invoice_id: BytesN<32>,
        amount: i128,
    ) -> Result<(), Error> {
        require_authorized(&env, &caller)?;
        env.events().publish(
            (Symbol::new(&env, "auto_invest"), wallet, invoice_id),
            amount,
        );
        Ok(())
    }

    pub fn get_notification(env: Env, notification_id: u64) -> Result<NotificationRecord, Error> {
        let mut record =
            storage::get_notification(&env, notification_id).ok_or(Error::NotificationNotFound)?;
        let state = effective_state(&record, env.ledger().sequence());
        record.state = state;
        if state == DeliveryState::Expired {
            record.next_retry_ledger = None;
        }
        Ok(record)
    }

    pub fn get_notification_status(
        env: Env,
        notification_id: u64,
    ) -> Result<NotificationStatus, Error> {
        let record =
            storage::get_notification(&env, notification_id).ok_or(Error::NotificationNotFound)?;
        let state = effective_state(&record, env.ledger().sequence());
        let next_retry_ledger = if state == DeliveryState::Expired {
            None
        } else {
            record.next_retry_ledger
        };

        Ok(NotificationStatus {
            id: record.id,
            state,
            retry_count: record.retry_count,
            max_retries: record.max_retries,
            next_retry_ledger,
            expires_at_ledger: record.expires_at_ledger,
            external_ref: record.external_ref,
            receipt_hash: record.receipt_hash,
        })
    }

    pub fn get_next_retry(env: Env, notification_id: u64) -> Result<Option<u32>, Error> {
        let status = Self::get_notification_status(env, notification_id)?;
        Ok(status.next_retry_ledger)
    }

    pub fn get_retry_policy(env: Env) -> RetryPolicy {
        storage::get_retry_policy(&env)
    }

    pub fn is_authorized_caller(env: Env, caller: Address) -> bool {
        storage::is_authorized_caller(&env, &caller)
    }

    pub fn is_delivery_provider(env: Env, provider: Address) -> bool {
        storage::is_delivery_provider(&env, &provider)
    }

    pub fn get_admin(env: Env) -> Result<Address, Error> {
        require_initialized(&env)
    }
}

#[cfg(test)]
mod test;
