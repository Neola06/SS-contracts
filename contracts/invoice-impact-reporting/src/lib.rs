//! Invoice Impact Reporting Contract for ESG metric tracking.
//!
//! Allows issuers to submit ESG metrics for their invoices post-settlement,
//! providing verified on-chain impact data for socially conscious investors.

#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
pub use types::{ImpactReport, InvoiceInfo, ReportType, StorageKey};

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String};

/// Rejects the zero address.
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
pub struct InvoiceImpactReportingContract;

#[contractimpl]
impl InvoiceImpactReportingContract {
    /// Initialize the contract with an admin who has verification authority.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        ensure_non_zero_address(&env, &admin)?;
        if get_admin(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        set_admin(&env, &admin);
        Ok(())
    }

    /// Records or updates invoice settlement status for impact reporting verification.
    /// Can be called by admin or registered during settlement.
    pub fn record_settlement(
        env: Env,
        caller: Address,
        invoice_id: BytesN<32>,
        issuer: Address,
        is_settled: bool,
    ) -> Result<(), Error> {
        let admin = get_admin(&env).ok_or(Error::NotInit)?;
        if caller != admin {
            return Err(Error::Unauthorized);
        }
        caller.require_auth();
        ensure_non_zero_address(&env, &issuer)?;

        let info = InvoiceInfo {
            invoice_id: invoice_id.clone(),
            issuer,
            is_settled,
        };
        env.storage()
            .persistent()
            .set(&StorageKey::Invoice(invoice_id), &info);
        Ok(())
    }

    /// Submit ESG metrics report for an invoice post-settlement.
    /// Restricted to issuer post-settlement only.
    pub fn submit_impact_report(
        env: Env,
        issuer: Address,
        invoice_id: BytesN<32>,
        metrics_hash: BytesN<32>,
        report_type: u32,
    ) -> Result<(), Error> {
        if get_admin(&env).is_none() {
            return Err(Error::NotInit);
        }
        ensure_non_zero_address(&env, &issuer)?;
        issuer.require_auth();

        let parsed_report_type = ReportType::from_u32(report_type).ok_or(Error::InvalidReportType)?;

        let invoice_info: InvoiceInfo = env
            .storage()
            .persistent()
            .get(&StorageKey::Invoice(invoice_id.clone()))
            .ok_or(Error::InvoiceNotFound)?;

        if invoice_info.issuer != issuer {
            return Err(Error::Unauthorized);
        }

        if !invoice_info.is_settled {
            return Err(Error::NotSettled);
        }

        let report_key = StorageKey::Report(invoice_id.clone());
        if env.storage().persistent().has(&report_key) {
            return Err(Error::ReportAlreadyExists);
        }

        let report = ImpactReport {
            invoice_id: invoice_id.clone(),
            issuer: issuer.clone(),
            metrics_hash: metrics_hash.clone(),
            report_type: parsed_report_type,
            verified: false,
        };

        env.storage().persistent().set(&report_key, &report);

        events::impact_report_submitted(
            &env,
            &invoice_id,
            &issuer,
            &metrics_hash,
            parsed_report_type,
        );

        Ok(())
    }

    /// Verify an existing impact report.
    /// Restricted to admin only.
    pub fn verify_impact_report(
        env: Env,
        admin: Address,
        invoice_id: BytesN<32>,
    ) -> Result<(), Error> {
        let stored_admin = get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();

        let report_key = StorageKey::Report(invoice_id.clone());
        let mut report: ImpactReport = env
            .storage()
            .persistent()
            .get(&report_key)
            .ok_or(Error::ReportNotFound)?;

        if report.verified {
            return Err(Error::AlreadyVerified);
        }

        report.verified = true;
        env.storage().persistent().set(&report_key, &report);

        events::impact_report_verified(&env, &invoice_id, &admin);

        Ok(())
    }

    /// Retrieve impact report details: metrics_hash, report_type, and verified flag.
    pub fn get_impact_report(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<(BytesN<32>, u32, bool), Error> {
        let report_key = StorageKey::Report(invoice_id);
        let report: ImpactReport = env
            .storage()
            .persistent()
            .get(&report_key)
            .ok_or(Error::ReportNotFound)?;

        Ok((report.metrics_hash, report.report_type as u32, report.verified))
    }

    /// Retrieve full impact report record if needed.
    pub fn get_full_report(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<ImpactReport, Error> {
        let report_key = StorageKey::Report(invoice_id);
        env.storage()
            .persistent()
            .get(&report_key)
            .ok_or(Error::ReportNotFound)
    }
}
