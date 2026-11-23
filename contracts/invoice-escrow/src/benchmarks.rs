//! Soroban CPU and memory resource consumption benchmarks for escrow operations (#442).
//!
//! These tests measure the resource cost of core escrow operations so we can:
//! - Track regressions when changing contract logic
//! - Ensure operations stay within Soroban's budget limits
//! - Guide batching limits and pagination thresholds
//!
//! Each benchmark runs the operation inside a measured closure and reports
//! the CPU instructions consumed and memory bytes allocated.

extern crate std;
use std::println;

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env};

use super::InvoiceCategory;
use crate::{InvoiceEscrow, InvoiceEscrowClient};

/// Build a minimal initialized escrow environment and return (env, admin, token).
fn setup_bench_env() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, InvoiceEscrow);
    InvoiceEscrowClient::new(&env, &contract_id).initialize(&admin, &300);

    (env, admin, contract_id)
}

/// Helper to create a commitment hash from a string.
fn bench_commitment(env: &Env, data: &str) -> BytesN<32> {
    let mut array = [0u8; 32];
    let bytes = data.as_bytes();
    let len = bytes.len().min(32);
    array[..len].copy_from_slice(&bytes[..len]);
    BytesN::from_array(env, &array)
}

// ── Benchmarks ────────────────────────────────────────────────────────────────

#[test]
fn bench_initialize_contract() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);

    let mut budget = env.budget();
    budget.reset_tracker();
    let contract_addr = env.register_contract(None, InvoiceEscrow);
    InvoiceEscrowClient::new(&env, &contract_addr).initialize(&admin, &300);
    let result = (budget.cpu_instruction_cost(), budget.memory_bytes_cost());

    println!(
        "[bench] initialize_contract: cpu={}, mem={}",
        result.0, result.1
    );
    // Sanity: initialization should complete without exceeding budget.
    assert!(result.0 > 0);
}

#[test]
fn bench_set_platform_fee() {
    let (env, admin, contract_id) = setup_bench_env();

    let mut budget = env.budget();
    budget.reset_tracker();
    env.as_contract(&contract_id, || {
        InvoiceEscrow::set_settlement_fee(env.clone(), admin.clone(), 500).unwrap();
    });
    let result = (budget.cpu_instruction_cost(), budget.memory_bytes_cost());

    println!(
        "[bench] set_platform_fee: cpu={}, mem={}",
        result.0, result.1
    );
    assert!(result.0 > 0);
}

#[test]
fn bench_set_grace_period() {
    let (env, admin, contract_id) = setup_bench_env();

    let mut budget = env.budget();
    budget.reset_tracker();
    env.as_contract(&contract_id, || {
        InvoiceEscrow::set_grace_period(env.clone(), admin.clone(), 86_400).unwrap();
    });
    let result = (budget.cpu_instruction_cost(), budget.memory_bytes_cost());

    println!(
        "[bench] set_grace_period: cpu={}, mem={}",
        result.0, result.1
    );
    assert!(result.0 > 0);
}

#[test]
fn bench_set_max_investors() {
    let (env, _admin, contract_id) = setup_bench_env();

    let mut budget = env.budget();
    budget.reset_tracker();
    env.as_contract(&contract_id, || {
        InvoiceEscrow::set_max_investors(env.clone(), 100).unwrap();
    });
    let result = (budget.cpu_instruction_cost(), budget.memory_bytes_cost());

    println!(
        "[bench] set_max_investors: cpu={}, mem={}",
        result.0, result.1
    );
    assert!(result.0 > 0);
}

#[test]
fn bench_propose_param_change() {
    let (env, admin, contract_id) = setup_bench_env();

    let mut budget = env.budget();
    budget.reset_tracker();
    env.as_contract(&contract_id, || {
        InvoiceEscrow::propose_param_change(
            env.clone(),
            admin.clone(),
            crate::types::ParamType::FeeBps,
            500,
            None,
        )
        .unwrap();
    });
    let result = (budget.cpu_instruction_cost(), budget.memory_bytes_cost());

    println!(
        "[bench] propose_param_change: cpu={}, mem={}",
        result.0, result.1
    );
    assert!(result.0 > 0);
}

#[test]
fn bench_get_config() {
    let (env, _admin, contract_id) = setup_bench_env();

    let mut budget = env.budget();
    budget.reset_tracker();
    env.as_contract(&contract_id, || {
        let _config = InvoiceEscrow::get_config(env.clone()).unwrap();
    });
    let result = (budget.cpu_instruction_cost(), budget.memory_bytes_cost());

    println!("[bench] get_config: cpu={}, mem={}", result.0, result.1);
    assert!(result.0 > 0);
}

#[test]
fn bench_set_category_fee() {
    let (env, admin, contract_id) = setup_bench_env();

    let mut budget = env.budget();
    budget.reset_tracker();
    env.as_contract(&contract_id, || {
        InvoiceEscrow::set_category_fee(
            env.clone(),
            admin.clone(),
            InvoiceCategory::Factoring,
            450,
        )
        .unwrap();
    });
    let result = (budget.cpu_instruction_cost(), budget.memory_bytes_cost());

    println!(
        "[bench] set_category_fee: cpu={}, mem={}",
        result.0, result.1
    );
    assert!(result.0 > 0);
}

#[test]
fn bench_toggle_whitelist() {
    let (env, admin, contract_id) = setup_bench_env();

    let mut budget = env.budget();
    budget.reset_tracker();
    env.as_contract(&contract_id, || {
        InvoiceEscrow::set_whitelist_enabled(env.clone(), admin.clone(), true).unwrap();
    });
    let result = (budget.cpu_instruction_cost(), budget.memory_bytes_cost());

    println!(
        "[bench] toggle_whitelist: cpu={}, mem={}",
        result.0, result.1
    );
    assert!(result.0 > 0);
}
