#![allow(deprecated)]
//! Event helpers for the auditor document attestation registry contract.

use soroban_sdk::{Address, BytesN, Env, Symbol};

use crate::types::Verdict;

pub fn initialized(env: &Env, admin: &Address) {
    env.events()
        .publish((Symbol::new(env, "initialized"),), admin.clone());
}

pub fn auditor_registered(env: &Env, auditor: &Address, name_hash: &BytesN<32>) {
    env.events().publish(
        (Symbol::new(env, "auditor_registered"), auditor.clone()),
        name_hash.clone(),
    );
}

pub fn auditor_revoked(env: &Env, auditor: &Address, revoked_at: u64) {
    env.events().publish(
        (Symbol::new(env, "auditor_revoked"), auditor.clone()),
        revoked_at,
    );
}

pub fn attested(
    env: &Env,
    invoice_id: &BytesN<32>,
    auditor: &Address,
    doc_hash: &BytesN<32>,
    verdict: Verdict,
) {
    env.events().publish(
        (
            Symbol::new(env, "attested"),
            invoice_id.clone(),
            auditor.clone(),
        ),
        (doc_hash.clone(), verdict),
    );
}

pub fn invoice_doc_hash_updated(
    env: &Env,
    invoice_id: &BytesN<32>,
    new_doc_hash: &BytesN<32>,
    invalidated_count: u32,
) {
    env.events().publish(
        (
            Symbol::new(env, "invoice_doc_hash_updated"),
            invoice_id.clone(),
        ),
        (new_doc_hash.clone(), invalidated_count),
    );
}
