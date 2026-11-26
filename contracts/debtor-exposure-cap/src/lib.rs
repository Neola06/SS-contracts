//! Per-debtor exposure cap contract (issue #737).
//!
//! Too many open invoices owed by the same debtor concentrate default risk
//! across the platform. This contract caps the total outstanding funded
//! amount per debtor and is checked when an invoice is listed or funded.
//!
//! A debtor is identified by a hashed identifier supplied at invoice
//! creation. Outstanding exposure rises when an invoice is funded and falls
//! again when the invoice is settled or written off, so capacity is released
//! as risk is retired.
//!
//! Flow:
//!   1. The admin calls `initialize` with the `default_debtor_cap`.
//!   2. `list_invoice` records the debtor of an invoice and rejects a listing
//!      that would already exceed the cap.
//!   3. `fund` increases the outstanding amount of the debtor and reverts with
//!      `Error::DebtorCapExceeded` when the cap would be crossed. A funding
//!      that brings the outstanding amount exactly to the cap emits
//!      `debtor_cap_hit` (the rejected attempt itself reverts, and Soroban
//!      discards events of a reverting call).
//!   4. `settle` / `write_off` reduce the outstanding amount, freeing capacity.
//!   5. `set_debtor_cap` lets the admin override the default for one debtor.

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

mod errors;
mod events;
mod storage;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
pub use types::StorageKey;

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env};

#[contract]
pub struct DebtorExposureCap;

#[contractimpl]
impl DebtorExposureCap {
    /// Initialise the contract with the admin and the default debtor cap.
    ///
    /// The default cap applies to every debtor that has no override.
    pub fn initialize(env: Env, admin: Address, default_debtor_cap: i128) -> Result<(), Error> {
        admin.require_auth();
        if storage::get_admin(&env).is_some() {
            return Err(Error::AlreadyInitialized);
        }
        if default_debtor_cap <= 0 {
            return Err(Error::InvalidCap);
        }
        storage::set_admin(&env, &admin);
        storage::set_default_cap(&env, default_debtor_cap);
        Ok(())
    }

    /// Admin-only: override the cap of a single debtor.
    ///
    /// The override takes precedence over the default cap.
    pub fn set_debtor_cap(
        env: Env,
        admin: Address,
        debtor_id: BytesN<32>,
        cap: i128,
    ) -> Result<(), Error> {
        Self::require_admin(&env, &admin)?;
        if cap <= 0 {
            return Err(Error::InvalidCap);
        }
        storage::set_debtor_cap(&env, &debtor_id, cap);
        events::debtor_cap_set(&env, &debtor_id, cap);
        Ok(())
    }

    /// List an invoice against the debtor it is owed by.
    ///
    /// Rejects a listing whose amount would already raise the outstanding
    /// amount of the debtor above the cap.
    pub fn list_invoice(
        env: Env,
        invoice_id: BytesN<32>,
        debtor_id: BytesN<32>,
        amount: i128,
    ) -> Result<(), Error> {
        Self::require_initialized(&env)?;
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if storage::get_invoice_debtor(&env, &invoice_id).is_some() {
            return Err(Error::InvoiceAlreadyListed);
        }
        let cap = Self::cap_of(&env, &debtor_id);
        let projected = storage::get_outstanding(&env, &debtor_id)
            .checked_add(amount)
            .ok_or(Error::ArithmeticOverflow)?;
        if projected > cap {
            return Err(Error::DebtorCapExceeded);
        }
        storage::set_invoice_debtor(&env, &invoice_id, &debtor_id);
        Ok(())
    }

    /// Record funding of a listed invoice.
    ///
    /// Increases the outstanding amount of the debtor and reverts with
    /// `Error::DebtorCapExceeded` when the cap would be crossed. A funding
    /// that brings the outstanding amount exactly to the cap emits
    /// `debtor_cap_hit`.
    pub fn fund(env: Env, invoice_id: BytesN<32>, amount: i128) -> Result<(), Error> {
        Self::require_initialized(&env)?;
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let debtor_id =
            storage::get_invoice_debtor(&env, &invoice_id).ok_or(Error::InvoiceNotFound)?;
        let cap = Self::cap_of(&env, &debtor_id);
        let projected = storage::get_outstanding(&env, &debtor_id)
            .checked_add(amount)
            .ok_or(Error::ArithmeticOverflow)?;
        if projected > cap {
            return Err(Error::DebtorCapExceeded);
        }
        storage::set_outstanding(&env, &debtor_id, projected);
        if projected == cap {
            events::debtor_cap_hit(&env, &debtor_id, projected, cap);
        }
        Ok(())
    }

    /// Record settlement of a listed invoice, freeing capacity equal to the
    /// principal repaid.
    pub fn settle(env: Env, invoice_id: BytesN<32>, principal: i128) -> Result<(), Error> {
        Self::release(&env, &invoice_id, principal)
    }

    /// Write off a listed invoice, freeing capacity equal to the principal
    /// written off.
    pub fn write_off(env: Env, invoice_id: BytesN<32>, principal: i128) -> Result<(), Error> {
        Self::release(&env, &invoice_id, principal)
    }

    /// View: the outstanding funded amount of a debtor.
    pub fn outstanding(env: Env, debtor_id: BytesN<32>) -> i128 {
        storage::get_outstanding(&env, &debtor_id)
    }

    /// View: the effective cap of a debtor (override or default).
    pub fn get_debtor_cap(env: Env, debtor_id: BytesN<32>) -> i128 {
        Self::cap_of(&env, &debtor_id)
    }

    /// View: the remaining capacity of a debtor, equal to cap minus the
    /// outstanding amount.
    pub fn debtor_headroom(env: Env, debtor_id: BytesN<32>) -> i128 {
        let cap = Self::cap_of(&env, &debtor_id);
        cap.saturating_sub(storage::get_outstanding(&env, &debtor_id))
    }

    /// Reduce the outstanding amount of the debtor of a listed invoice.
    fn release(env: &Env, invoice_id: &BytesN<32>, principal: i128) -> Result<(), Error> {
        Self::require_initialized(env)?;
        if principal <= 0 {
            return Err(Error::InvalidAmount);
        }
        let debtor_id =
            storage::get_invoice_debtor(env, invoice_id).ok_or(Error::InvoiceNotFound)?;
        let next = storage::get_outstanding(env, &debtor_id).saturating_sub(principal);
        storage::set_outstanding(env, &debtor_id, next);
        Ok(())
    }

    /// The effective cap of a debtor, preferring an override over the default.
    fn cap_of(env: &Env, debtor_id: &BytesN<32>) -> i128 {
        match storage::get_debtor_cap(env, debtor_id) {
            Some(cap) => cap,
            None => storage::get_default_cap(env),
        }
    }

    /// Check that the contract has been initialized.
    fn require_initialized(env: &Env) -> Result<(), Error> {
        if storage::get_admin(env).is_none() {
            return Err(Error::NotInitialized);
        }
        Ok(())
    }

    /// Check that the supplied caller is the configured admin.
    fn require_admin(env: &Env, admin: &Address) -> Result<(), Error> {
        admin.require_auth();
        let stored = storage::get_admin(env).ok_or(Error::NotInitialized)?;
        if &stored != admin {
            return Err(Error::Unauthorized);
        }
        Ok(())
    }
}
