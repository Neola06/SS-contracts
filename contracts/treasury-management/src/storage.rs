//! Storage helpers for the platform treasury management contract.

use soroban_sdk::{Address, Env, Vec};

use crate::types::{AllocationRecord, StorageKey};

/// Ledgers below which a persistent entry's TTL is extended (~7 days at 5s/ledger).
pub const TTL_THRESHOLD: u32 = 120_960;
/// Minimum TTL extension in ledger units (~60 days at 5s/ledger).
pub const MIN_TTL_EXTEND: u32 = 1_036_800;

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&StorageKey::Admin, admin);
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
}

pub fn get_admin(env: &Env) -> Option<Address> {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
    env.storage().instance().get(&StorageKey::Admin)
}

pub fn get_total_received(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&StorageKey::TotalReceived)
        .unwrap_or(0)
}

pub fn set_total_received(env: &Env, amount: i128) {
    env.storage()
        .instance()
        .set(&StorageKey::TotalReceived, &amount);
}

pub fn get_total_allocated(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&StorageKey::TotalAllocated)
        .unwrap_or(0)
}

pub fn set_total_allocated(env: &Env, amount: i128) {
    env.storage()
        .instance()
        .set(&StorageKey::TotalAllocated, &amount);
}

pub fn get_known_sources(env: &Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&StorageKey::KnownSources)
        .unwrap_or(Vec::new(env))
}

/// Append `source` to the known-sources list if it isn't already present.
pub fn remember_source(env: &Env, source: &Address) {
    let mut sources = get_known_sources(env);
    if !sources.contains(source) {
        sources.push_back(source.clone());
        env.storage()
            .instance()
            .set(&StorageKey::KnownSources, &sources);
    }
}

fn get_persistent_i128(env: &Env, key: &StorageKey) -> i128 {
    let data = env.storage().persistent().get(key);
    if data.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(key, TTL_THRESHOLD, MIN_TTL_EXTEND);
    }
    data.unwrap_or(0)
}

fn set_persistent_i128(env: &Env, key: &StorageKey, value: i128) {
    env.storage().persistent().set(key, &value);
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}

/// Lifetime fees received from `source`.
pub fn get_source_received(env: &Env, source: &Address) -> i128 {
    get_persistent_i128(env, &StorageKey::SourceReceived(source.clone()))
}

pub fn set_source_received(env: &Env, source: &Address, total: i128) {
    set_persistent_i128(env, &StorageKey::SourceReceived(source.clone()), total);
}

/// Lifetime allocations attributed to `source`.
pub fn get_source_allocated(env: &Env, source: &Address) -> i128 {
    get_persistent_i128(env, &StorageKey::SourceAllocated(source.clone()))
}

pub fn set_source_allocated(env: &Env, source: &Address, total: i128) {
    set_persistent_i128(env, &StorageKey::SourceAllocated(source.clone()), total);
}

pub fn get_allocation_count(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&StorageKey::AllocationCount)
        .unwrap_or(0)
}

pub fn get_allocation_cooldown(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&StorageKey::AllocationCooldown)
        .unwrap_or(0)
}

pub fn set_allocation_cooldown(env: &Env, cooldown: u32) {
    env.storage()
        .instance()
        .set(&StorageKey::AllocationCooldown, &cooldown);
}

pub fn get_last_allocation_ledger(env: &Env) -> Option<u32> {
    env.storage()
        .instance()
        .get(&StorageKey::LastAllocationLedger)
}

pub fn set_last_allocation_ledger(env: &Env, ledger: u32) {
    env.storage()
        .instance()
        .set(&StorageKey::LastAllocationLedger, &ledger);
}

pub fn get_allocation(env: &Env, index: u32) -> Option<AllocationRecord> {
    let key = StorageKey::Allocation(index);
    let data = env.storage().persistent().get(&key);
    if data.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
    }
    data
}

/// Append a new allocation record, returning its assigned index.
pub fn push_allocation(env: &Env, record: &AllocationRecord) -> u32 {
    let index = get_allocation_count(env);
    let key = StorageKey::Allocation(index);
    env.storage().persistent().set(&key, record);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
    env.storage()
        .instance()
        .set(&StorageKey::AllocationCount, &(index + 1));
    index
}

pub fn is_allowlist_enforced(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&StorageKey::SourceAllowlistEnforced)
        .unwrap_or(false)
}

pub fn set_allowlist_enforced(env: &Env, enforced: bool) {
    env.storage()
        .instance()
        .set(&StorageKey::SourceAllowlistEnforced, &enforced);
}

pub fn is_source_approved(env: &Env, source: &Address) -> bool {
    env.storage()
        .persistent()
        .get(&StorageKey::ApprovedSource(source.clone()))
        .unwrap_or(false)
}

pub fn set_source_approved(env: &Env, source: &Address, approved: bool) {
    let key = StorageKey::ApprovedSource(source.clone());
    env.storage().persistent().set(&key, &approved);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}

pub fn get_source_cap(env: &Env, source: &Address) -> Option<i128> {
    env.storage()
        .persistent()
        .get(&StorageKey::SourceCap(source.clone()))
}

pub fn set_source_cap(env: &Env, source: &Address, cap: Option<i128>) {
    let key = StorageKey::SourceCap(source.clone());
    match cap {
        Some(c) => {
            env.storage().persistent().set(&key, &c);
            env.storage()
                .persistent()
                .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
        }
        None => {
            env.storage().persistent().remove(&key);
        }
    }
}

pub fn get_source_daily_total(env: &Env, source: &Address, ledger_day: u64) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::SourceDailyTotal(source.clone(), ledger_day))
        .unwrap_or(0)
}

pub fn set_source_daily_total(env: &Env, source: &Address, ledger_day: u64, total: i128) {
    let key = StorageKey::SourceDailyTotal(source.clone(), ledger_day);
    env.storage().persistent().set(&key, &total);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}
