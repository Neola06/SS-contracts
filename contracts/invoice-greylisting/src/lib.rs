//! Invoice greylisting contract for compliance hold periods.
//!
//! Places compliance holds on specific invoices during regulatory review,
//! allowing existing holders to exit (sell or list on secondary market)
//! while blocking new investments until cleared or expired.

#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
pub use types::{GreylistRecord, StorageKey};

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String};

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

fn get_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&StorageKey::Admin)
}

fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&StorageKey::Admin, admin);
}

#[contract]
pub struct InvoiceGreylistingContract;

#[contractimpl]
impl InvoiceGreylistingContract {
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        ensure_non_zero_address(&env, &admin)?;
        if get_admin(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        set_admin(&env, &admin);
        Ok(())
    }

    /// Admin-only: places an invoice on greylist with a reason and duration in seconds.
    pub fn greylist_invoice(
        env: Env,
        admin: Address,
        invoice_id: BytesN<32>,
        reason: String,
        duration_seconds: u64,
    ) -> Result<(), Error> {
        let stored_admin = get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();

        if duration_seconds == 0 {
            return Err(Error::InvalidDuration);
        }

        let now = env.ledger().timestamp();
        let expires_at = now.checked_add(duration_seconds).ok_or(Error::Overflow)?;

        let record = GreylistRecord {
            invoice_id: invoice_id.clone(),
            reason: reason.clone(),
            greylisted_at: now,
            duration_seconds,
            expires_at,
            active: true,
        };

        env.storage()
            .persistent()
            .set(&StorageKey::Greylist(invoice_id.clone()), &record);

        events::invoice_greylisted(&env, &invoice_id, &reason, duration_seconds);

        Ok(())
    }

    /// Admin-only: explicitly clears the greylist hold for an invoice.
    pub fn clear_greylist(
        env: Env,
        admin: Address,
        invoice_id: BytesN<32>,
    ) -> Result<(), Error> {
        let stored_admin = get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();

        let mut record: GreylistRecord = env
            .storage()
            .persistent()
            .get(&StorageKey::Greylist(invoice_id.clone()))
            .ok_or(Error::NotGreylisted)?;

        if !record.active {
            return Err(Error::NotGreylisted);
        }

        record.active = false;
        env.storage()
            .persistent()
            .set(&StorageKey::Greylist(invoice_id.clone()), &record);

        events::greylist_cleared(&env, &invoice_id);

        Ok(())
    }

    /// Checks whether an invoice is actively greylisted.
    /// Returns false if not greylisted, or if duration_seconds has expired (auto-clear).
    pub fn is_greylisted(env: Env, invoice_id: BytesN<32>) -> bool {
        let record_opt: Option<GreylistRecord> = env
            .storage()
            .persistent()
            .get(&StorageKey::Greylist(invoice_id));

        match record_opt {
            Some(record) => {
                if !record.active {
                    false
                } else {
                    let now = env.ledger().timestamp();
                    now < record.expires_at
                }
            }
            None => false,
        }
    }

    /// Returns the greylist record for an invoice if one exists.
    pub fn get_greylist_status(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<GreylistRecord, Error> {
        let mut record: GreylistRecord = env
            .storage()
            .persistent()
            .get(&StorageKey::Greylist(invoice_id))
            .ok_or(Error::NotGreylisted)?;

        let now = env.ledger().timestamp();
        if record.active && now >= record.expires_at {
            record.active = false;
        }

        Ok(record)
    }

    /// Convenience or mock helper to set holder balance for testing exit behavior.
    pub fn set_holder_balance(
        env: Env,
        caller: Address,
        invoice_id: BytesN<32>,
        holder: Address,
        amount: i128,
    ) -> Result<(), Error> {
        let stored_admin = get_admin(&env).ok_or(Error::NotInit)?;
        if caller != stored_admin {
            return Err(Error::Unauthorized);
        }
        caller.require_auth();

        env.storage().persistent().set(
            &StorageKey::HolderBalance(invoice_id, holder),
            &amount,
        );
        Ok(())
    }

    /// Mock/proxy investment entrypoint to verify investment blocking during greylist.
    pub fn invest(
        env: Env,
        investor: Address,
        invoice_id: BytesN<32>,
        amount: i128,
    ) -> Result<(), Error> {
        investor.require_auth();

        if Self::is_greylisted(env.clone(), invoice_id.clone()) {
            return Err(Error::InvoiceGreylisted);
        }

        let key = StorageKey::HolderBalance(invoice_id, investor);
        let current: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &(current + amount));
        Ok(())
    }

    /// Existing holders can sell / exit even during a compliance hold.
    pub fn exit_investment(
        env: Env,
        holder: Address,
        invoice_id: BytesN<32>,
        amount: i128,
    ) -> Result<(), Error> {
        holder.require_auth();

        let key = StorageKey::HolderBalance(invoice_id, holder);
        let balance: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        if balance < amount || amount <= 0 {
            return Err(Error::Unauthorized);
        }

        env.storage().persistent().set(&key, &(balance - amount));
        Ok(())
    }
}
