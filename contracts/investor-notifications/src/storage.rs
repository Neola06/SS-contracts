//! Storage helpers for the investor notifications contract.

use soroban_sdk::{Address, Env, String};

use crate::types::{DataKey, NotificationRecord, RetryPolicy};

pub const DEFAULT_MAX_RETRIES: u32 = 3;
pub const DEFAULT_RETRY_DELAY_LEDGERS: u32 = 10;
pub const DEFAULT_EXPIRY_LEDGERS: u32 = 1_000;

pub const TTL_THRESHOLD: u32 = 120_960;
pub const MIN_TTL_EXTEND: u32 = 1_036_800;

pub fn get_admin(env: &Env) -> Option<Address> {
    if env.storage().instance().has(&DataKey::Admin) {
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
    }
    env.storage().instance().get(&DataKey::Admin)
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
}

pub fn is_authorized_caller(env: &Env, caller: &Address) -> bool {
    let key = DataKey::Authorized(caller.clone());
    let authorized = env.storage().persistent().get(&key).unwrap_or(false);
    if env.storage().persistent().has(&key) {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
    }
    authorized
}

pub fn set_authorized_caller(env: &Env, caller: &Address, authorized: bool) {
    let key = DataKey::Authorized(caller.clone());
    env.storage().persistent().set(&key, &authorized);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}

pub fn is_delivery_provider(env: &Env, provider: &Address) -> bool {
    let key = DataKey::DeliveryProvider(provider.clone());
    let authorized = env.storage().persistent().get(&key).unwrap_or(false);
    if env.storage().persistent().has(&key) {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
    }
    authorized
}

pub fn set_delivery_provider(env: &Env, provider: &Address, authorized: bool) {
    let key = DataKey::DeliveryProvider(provider.clone());
    env.storage().persistent().set(&key, &authorized);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}

pub fn get_retry_policy(env: &Env) -> RetryPolicy {
    if env.storage().instance().has(&DataKey::RetryPolicy) {
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
    }
    env.storage()
        .instance()
        .get(&DataKey::RetryPolicy)
        .unwrap_or(RetryPolicy {
            max_retries: DEFAULT_MAX_RETRIES,
            retry_delay_ledgers: DEFAULT_RETRY_DELAY_LEDGERS,
            expiry_ledgers: DEFAULT_EXPIRY_LEDGERS,
        })
}

pub fn set_retry_policy(env: &Env, policy: &RetryPolicy) {
    env.storage().instance().set(&DataKey::RetryPolicy, policy);
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
}

pub fn next_notification_id(env: &Env) -> u64 {
    let current: u64 = env
        .storage()
        .instance()
        .get(&DataKey::NextNotificationId)
        .unwrap_or(1);
    env.storage()
        .instance()
        .set(&DataKey::NextNotificationId, &(current + 1));
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
    current
}

pub fn get_notification(env: &Env, notification_id: u64) -> Option<NotificationRecord> {
    let key = DataKey::Notification(notification_id);
    let data = env.storage().persistent().get(&key);
    if data.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
    }
    data
}

pub fn set_notification(env: &Env, notification_id: u64, record: &NotificationRecord) {
    let key = DataKey::Notification(notification_id);
    env.storage().persistent().set(&key, record);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}

pub fn is_receipt_used(env: &Env, external_ref: &String) -> bool {
    let key = DataKey::UsedReceipt(external_ref.clone());
    env.storage().persistent().has(&key)
}

pub fn mark_receipt_used(env: &Env, external_ref: &String, notification_id: u64) {
    let key = DataKey::UsedReceipt(external_ref.clone());
    env.storage().persistent().set(&key, &notification_id);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}
