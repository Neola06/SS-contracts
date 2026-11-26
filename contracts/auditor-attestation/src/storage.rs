//! Storage helpers for the auditor document attestation registry contract.

use soroban_sdk::{Address, BytesN, Env, Vec};

use crate::types::{Attestation, AuditorRecord, StorageKey};

pub const TTL_THRESHOLD: u32 = 120_960;
pub const MIN_TTL_EXTEND: u32 = 1_036_800;

pub fn get_admin(env: &Env) -> Option<Address> {
    if env.storage().instance().has(&StorageKey::Admin) {
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
    }
    env.storage().instance().get(&StorageKey::Admin)
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&StorageKey::Admin, admin);
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
}

pub fn get_auditor(env: &Env, auditor: &Address) -> Option<AuditorRecord> {
    let key = StorageKey::Auditor(auditor.clone());
    let data = env.storage().persistent().get(&key);
    if data.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
    }
    data
}

pub fn set_auditor(env: &Env, auditor: &Address, record: &AuditorRecord) {
    let key = StorageKey::Auditor(auditor.clone());
    env.storage().persistent().set(&key, record);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}

pub fn get_invoice_doc_hash(env: &Env, invoice_id: &BytesN<32>) -> Option<BytesN<32>> {
    let key = StorageKey::InvoiceDocHash(invoice_id.clone());
    let data = env.storage().persistent().get(&key);
    if data.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
    }
    data
}

pub fn set_invoice_doc_hash(env: &Env, invoice_id: &BytesN<32>, doc_hash: &BytesN<32>) {
    let key = StorageKey::InvoiceDocHash(invoice_id.clone());
    env.storage().persistent().set(&key, doc_hash);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}

pub fn get_attestation(
    env: &Env,
    invoice_id: &BytesN<32>,
    auditor: &Address,
) -> Option<Attestation> {
    let key = StorageKey::Attestation(invoice_id.clone(), auditor.clone());
    let data = env.storage().persistent().get(&key);
    if data.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
    }
    data
}

pub fn set_attestation(
    env: &Env,
    invoice_id: &BytesN<32>,
    auditor: &Address,
    attestation: &Attestation,
) {
    let key = StorageKey::Attestation(invoice_id.clone(), auditor.clone());
    env.storage().persistent().set(&key, attestation);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}

pub fn remove_attestation(env: &Env, invoice_id: &BytesN<32>, auditor: &Address) {
    let key = StorageKey::Attestation(invoice_id.clone(), auditor.clone());
    env.storage().persistent().remove(&key);
}

pub fn get_invoice_auditors(env: &Env, invoice_id: &BytesN<32>) -> Vec<Address> {
    let key = StorageKey::InvoiceAuditors(invoice_id.clone());
    let data = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env));
    if env.storage().persistent().has(&key) {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
    }
    data
}

pub fn set_invoice_auditors(env: &Env, invoice_id: &BytesN<32>, auditors: &Vec<Address>) {
    let key = StorageKey::InvoiceAuditors(invoice_id.clone());
    env.storage().persistent().set(&key, auditors);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}

pub fn clear_invoice_auditors(env: &Env, invoice_id: &BytesN<32>) {
    let key = StorageKey::InvoiceAuditors(invoice_id.clone());
    env.storage().persistent().remove(&key);
}
