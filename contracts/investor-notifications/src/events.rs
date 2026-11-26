#![allow(deprecated)]
//! Event helpers for the investor notifications contract.

use soroban_sdk::{Address, BytesN, Env, String, Symbol};

pub fn notification_queued(
    env: &Env,
    notification_id: u64,
    wallet: &Address,
    notification_type: &Symbol,
    expires_at_ledger: u32,
) {
    env.events().publish(
        (
            Symbol::new(env, "notification_queued"),
            notification_id,
            wallet.clone(),
        ),
        (notification_type.clone(), expires_at_ledger),
    );
}

pub fn notification_delivered(
    env: &Env,
    notification_id: u64,
    provider: &Address,
    external_ref: &String,
    receipt_hash: &BytesN<32>,
    delivered_ledger: u32,
) {
    env.events().publish(
        (
            Symbol::new(env, "notification_delivered"),
            notification_id,
            provider.clone(),
        ),
        (external_ref.clone(), receipt_hash.clone(), delivered_ledger),
    );
}

pub fn notification_delivery_failed(
    env: &Env,
    notification_id: u64,
    retry_count: u32,
    next_retry_ledger: u32,
    reason: &Symbol,
) {
    env.events().publish(
        (
            Symbol::new(env, "notification_delivery_failed"),
            notification_id,
        ),
        (retry_count, next_retry_ledger, reason.clone()),
    );
}

pub fn notification_retry_exhausted(
    env: &Env,
    notification_id: u64,
    retry_count: u32,
    reason: &Symbol,
) {
    env.events().publish(
        (
            Symbol::new(env, "notification_retry_exhausted"),
            notification_id,
        ),
        (retry_count, reason.clone()),
    );
}

pub fn notification_expired(env: &Env, notification_id: u64, expired_at_ledger: u32) {
    env.events().publish(
        (Symbol::new(env, "notification_expired"), notification_id),
        expired_at_ledger,
    );
}
