//! Events emitted by the invoice greylisting contract.

use soroban_sdk::{BytesN, Env, String, Symbol};

pub fn invoice_greylisted(
    env: &Env,
    invoice_id: &BytesN<32>,
    reason: &String,
    duration_seconds: u64,
) {
    let topics = (Symbol::new(env, "InvoiceGreylisted"), invoice_id.clone());
    env.events()
        .publish(topics, (reason.clone(), duration_seconds));
}

pub fn greylist_cleared(env: &Env, invoice_id: &BytesN<32>) {
    let topics = (Symbol::new(env, "GreylistCleared"), invoice_id.clone());
    env.events().publish(topics, ());
}
