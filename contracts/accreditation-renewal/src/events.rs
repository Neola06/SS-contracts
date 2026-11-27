use soroban_sdk::{symbol_short, Address, Env};

pub fn accreditation_renewal_reminder(
    env: &Env,
    wallet: &Address,
    days_remaining: u64,
    expiry_timestamp: u64,
) {
    env.events().publish(
        (symbol_short!("renew_rem"), wallet.clone()),
        (days_remaining, expiry_timestamp),
    );
}

/// Emitted once when an operator opens a new reminder sweep.
pub fn reminder_sweep_started(env: &Env, sweep_id: u32, total: u32, started_at: u64) {
    env.events()
        .publish((symbol_short!("swp_start"), sweep_id), (total, started_at));
}

/// Emitted exactly once, by the page that first reaches the end of the sweep's membership.
pub fn reminder_sweep_completed(env: &Env, sweep_id: u32, total: u32, completed_at: u64) {
    env.events()
        .publish((symbol_short!("swp_done"), sweep_id), (total, completed_at));
}
