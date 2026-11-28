#![allow(deprecated)]

use soroban_sdk::{Address, BytesN, Env, Symbol};

/// Emitted when rollover investment succeeds.
/// Topics: (RolloverExecuted, wallet, source_invoice_id)
/// Data: (target_invoice_id, amount)
pub fn rollover_executed(
    env: &Env,
    wallet: &Address,
    source_invoice_id: &BytesN<32>,
    target_invoice_id: &BytesN<32>,
    amount: i128,
) {
    let topics = (
        Symbol::new(env, "RolloverExecuted"),
        wallet.clone(),
        source_invoice_id.clone(),
    );
    env.events()
        .publish(topics, (target_invoice_id.clone(), amount));
}

/// Emitted when no matching invoice is found in pool; proceeds returned to wallet.
/// Topics: (RolloverNoMatch, wallet, source_invoice_id)
/// Data: amount
pub fn rollover_no_match(
    env: &Env,
    wallet: &Address,
    source_invoice_id: &BytesN<32>,
    amount: i128,
) {
    let topics = (
        Symbol::new(env, "RolloverNoMatch"),
        wallet.clone(),
        source_invoice_id.clone(),
    );
    env.events().publish(topics, amount);
}
