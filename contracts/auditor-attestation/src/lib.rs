//! Auditor document attestation registry contract for StellarSettle.
//!
//! Anchors off-chain invoice document hashes on-chain and lets admin-approved
//! auditors publish verifiable review verdicts (`Verified`, `Inconclusive`,
//! `Rejected`). Attestations are keyed by `(invoice_id, auditor)` to prevent
//! duplicates, gated so only active auditors count toward `is_verified`, and
//! automatically invalidated whenever an invoice's stored `doc_hash` changes.

#![no_std]

mod errors;
mod events;
mod storage;
mod types;

pub use errors::Error;
pub use types::{Attestation, AuditorRecord, Verdict};

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String, Vec};

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

fn ensure_non_zero_hash(env: &Env, hash: &BytesN<32>) -> Result<(), Error> {
    if *hash == BytesN::from_array(env, &[0u8; 32]) {
        return Err(Error::InvalidHash);
    }
    Ok(())
}

fn require_admin(env: &Env, admin: &Address) -> Result<(), Error> {
    let stored_admin = storage::get_admin(env).ok_or(Error::NotInit)?;
    if *admin != stored_admin {
        return Err(Error::Unauthorized);
    }
    admin.require_auth();
    Ok(())
}

#[contract]
pub struct AuditorAttestationRegistry;

#[contractimpl]
impl AuditorAttestationRegistry {
    /// Initialize the registry with the admin allowed to approve/revoke
    /// auditors and anchor invoice document hashes.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        ensure_non_zero_address(&env, &admin)?;
        if storage::get_admin(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        admin.require_auth();
        storage::set_admin(&env, &admin);
        events::initialized(&env, &admin);
        Ok(())
    }

    /// Admin-only: register an approved auditor with their off-chain identity
    /// `name_hash`. Re-registering an active auditor is rejected; a revoked
    /// auditor can be explicitly reinstated by the admin.
    pub fn register_auditor(
        env: Env,
        admin: Address,
        auditor: Address,
        name_hash: BytesN<32>,
    ) -> Result<(), Error> {
        require_admin(&env, &admin)?;
        ensure_non_zero_address(&env, &auditor)?;
        ensure_non_zero_hash(&env, &name_hash)?;

        if let Some(existing) = storage::get_auditor(&env, &auditor) {
            if existing.active {
                return Err(Error::AuditorAlreadyRegistered);
            }
        }

        let record = AuditorRecord {
            auditor: auditor.clone(),
            name_hash: name_hash.clone(),
            active: true,
            registered_at: env.ledger().timestamp(),
            revoked_at: None,
        };
        storage::set_auditor(&env, &auditor, &record);
        events::auditor_registered(&env, &auditor, &name_hash);
        Ok(())
    }

    /// Admin-only: revoke an auditor so they can no longer attest and their
    /// prior attestations no longer count toward `is_verified`.
    pub fn revoke_auditor(env: Env, admin: Address, auditor: Address) -> Result<(), Error> {
        require_admin(&env, &admin)?;
        let mut record = storage::get_auditor(&env, &auditor).ok_or(Error::AuditorNotFound)?;
        if !record.active {
            return Err(Error::AuditorRevoked);
        }

        let now = env.ledger().timestamp();
        record.active = false;
        record.revoked_at = Some(now);
        storage::set_auditor(&env, &auditor, &record);
        events::auditor_revoked(&env, &auditor, now);
        Ok(())
    }

    /// Admin-only: anchor or update the canonical document hash for `invoice_id`.
    /// When the stored `doc_hash` changes to a different hash, all prior
    /// attestations on `invoice_id` are invalidated and cleared.
    pub fn set_invoice_doc_hash(
        env: Env,
        admin: Address,
        invoice_id: BytesN<32>,
        doc_hash: BytesN<32>,
    ) -> Result<(), Error> {
        require_admin(&env, &admin)?;
        ensure_non_zero_hash(&env, &doc_hash)?;

        let existing = storage::get_invoice_doc_hash(&env, &invoice_id);
        let mut invalidated_count: u32 = 0;

        if let Some(prev_hash) = existing {
            if prev_hash != doc_hash {
                let auditors = storage::get_invoice_auditors(&env, &invoice_id);
                invalidated_count = auditors.len();
                for addr in auditors.iter() {
                    storage::remove_attestation(&env, &invoice_id, &addr);
                }
                storage::clear_invoice_auditors(&env, &invoice_id);
            }
        }

        storage::set_invoice_doc_hash(&env, &invoice_id, &doc_hash);
        events::invoice_doc_hash_updated(&env, &invoice_id, &doc_hash, invalidated_count);
        Ok(())
    }

    /// Approved auditor-only: record an attestation verdict on `invoice_id`
    /// anchored to `doc_hash`. Rejects duplicate attestations from the same
    /// auditor for the active document hash.
    pub fn attest(
        env: Env,
        auditor: Address,
        invoice_id: BytesN<32>,
        doc_hash: BytesN<32>,
        verdict: Verdict,
    ) -> Result<(), Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)?;
        let auditor_record = storage::get_auditor(&env, &auditor).ok_or(Error::AuditorNotFound)?;
        if !auditor_record.active {
            return Err(Error::AuditorRevoked);
        }
        auditor.require_auth();
        ensure_non_zero_hash(&env, &doc_hash)?;

        match storage::get_invoice_doc_hash(&env, &invoice_id) {
            Some(stored_hash) => {
                if stored_hash != doc_hash {
                    return Err(Error::DocHashMismatch);
                }
            }
            None => {
                storage::set_invoice_doc_hash(&env, &invoice_id, &doc_hash);
            }
        }

        if storage::get_attestation(&env, &invoice_id, &auditor).is_some() {
            return Err(Error::DuplicateAttestation);
        }

        let attestation = Attestation {
            invoice_id: invoice_id.clone(),
            auditor: auditor.clone(),
            doc_hash: doc_hash.clone(),
            verdict,
            attested_at: env.ledger().timestamp(),
        };
        storage::set_attestation(&env, &invoice_id, &auditor, &attestation);

        let mut auditors = storage::get_invoice_auditors(&env, &invoice_id);
        auditors.push_back(auditor.clone());
        storage::set_invoice_auditors(&env, &invoice_id, &auditors);

        events::attested(&env, &invoice_id, &auditor, &doc_hash, verdict);
        Ok(())
    }

    /// View: return all valid attestations currently recorded for `invoice_id`'s
    /// active `doc_hash`.
    pub fn attestations_of(env: Env, invoice_id: BytesN<32>) -> Vec<Attestation> {
        let mut out = Vec::new(&env);
        let Some(current_hash) = storage::get_invoice_doc_hash(&env, &invoice_id) else {
            return out;
        };
        let auditors = storage::get_invoice_auditors(&env, &invoice_id);
        for addr in auditors.iter() {
            if let Some(att) = storage::get_attestation(&env, &invoice_id, &addr) {
                if att.doc_hash == current_hash {
                    out.push_back(att);
                }
            }
        }
        out
    }

    /// View: gating check returning `true` iff at least `min_attestations`
    /// (`min_attestations > 0`) active registered auditors have attested
    /// `Verdict::Verified` for `invoice_id`'s current `doc_hash`.
    pub fn is_verified(env: Env, invoice_id: BytesN<32>, min_attestations: u32) -> bool {
        if min_attestations == 0 {
            return false;
        }
        let Some(current_hash) = storage::get_invoice_doc_hash(&env, &invoice_id) else {
            return false;
        };
        let auditors = storage::get_invoice_auditors(&env, &invoice_id);
        let mut verified_count: u32 = 0;

        for addr in auditors.iter() {
            let is_active = storage::get_auditor(&env, &addr)
                .map(|r| r.active)
                .unwrap_or(false);
            if !is_active {
                continue;
            }
            if let Some(att) = storage::get_attestation(&env, &invoice_id, &addr) {
                if att.doc_hash == current_hash && att.verdict == Verdict::Verified {
                    verified_count += 1;
                    if verified_count >= min_attestations {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// View: return an auditor's registration record.
    pub fn get_auditor(env: Env, auditor: Address) -> Result<AuditorRecord, Error> {
        storage::get_auditor(&env, &auditor).ok_or(Error::AuditorNotFound)
    }

    /// View: return the currently anchored document hash for `invoice_id`, if set.
    pub fn get_invoice_doc_hash(env: Env, invoice_id: BytesN<32>) -> Option<BytesN<32>> {
        storage::get_invoice_doc_hash(&env, &invoice_id)
    }

    /// View: return the current admin address.
    pub fn get_admin(env: Env) -> Result<Address, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)
    }
}

#[cfg(test)]
mod test;
