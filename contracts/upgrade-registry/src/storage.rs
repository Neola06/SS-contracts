//! Storage helpers for the upgrade registry contract.

use soroban_sdk::{Address, Env, String, Vec};

use crate::types::{ContractRecord, StorageKey, UpgradeRecord};

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
    if env.storage().instance().has(&StorageKey::Admin) {
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
    }
    env.storage().instance().get(&StorageKey::Admin)
}

pub fn get_contract(env: &Env, name: &String) -> Option<ContractRecord> {
    let key = StorageKey::Contract(name.clone());
    let data = env.storage().persistent().get(&key);
    if data.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
    }
    data
}

pub fn set_contract(env: &Env, name: &String, record: &ContractRecord) {
    let key = StorageKey::Contract(name.clone());
    env.storage().persistent().set(&key, record);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}

pub fn get_history(env: &Env, name: &String) -> Vec<UpgradeRecord> {
    let key = StorageKey::History(name.clone());
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

pub fn set_history(env: &Env, name: &String, history: &Vec<UpgradeRecord>) {
    let key = StorageKey::History(name.clone());
    env.storage().persistent().set(&key, history);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}

pub fn get_contract_names(env: &Env) -> Vec<String> {
    if env.storage().instance().has(&StorageKey::ContractNames) {
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
    }
    env.storage()
        .instance()
        .get(&StorageKey::ContractNames)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn add_contract_name(env: &Env, name: &String) {
    let mut names = get_contract_names(env);
    names.push_back(name.clone());
    env.storage()
        .instance()
        .set(&StorageKey::ContractNames, &names);
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
}
