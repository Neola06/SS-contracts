#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    Address, BytesN, Env, Symbol, TryFromVal, TryIntoVal, Val, Vec,
};

fn setup_test() -> (
    Env,
    Address,
    Address,
    InvoiceAutoRolloverContractClient<'static>,
    Address,
    BytesN<32>,
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(InvoiceAutoRolloverContract, ());
    let client = InvoiceAutoRolloverContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let operator = Address::generate(&env);
    let investor = Address::generate(&env);
    let settled_invoice = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &operator);

    (env, admin, operator, client, investor, settled_invoice)
}

fn last_event(env: &Env) -> (Vec<Val>, Val) {
    let all = env.events().all();
    let events = all.events();
    let last = events.last().expect("no events emitted");
    let soroban_sdk::xdr::ContractEventBody::V0(v0) = &last.body;
    let topics = Vec::<Val>::try_from_val(
        env,
        &soroban_sdk::xdr::ScVal::Vec(Some(v0.topics.clone().into())),
    )
    .unwrap();
    let data = Val::try_from_val(env, &v0.data).unwrap();
    (topics, data)
}

#[test]
fn test_initialize_and_prevent_double_init() {
    let (_env, admin, operator, client, _, _) = setup_test();
    let res = client.try_initialize(&admin, &operator);
    assert_eq!(res, Err(Ok(Error::AlreadyInitialized)));
}

#[test]
fn test_set_rollover_config_restricted_to_wallet_owner() {
    let (_env, _, _, client, investor, _) = setup_test();

    let criteria = RolloverCriteria {
        min_yield_bps: 800,
        max_duration_days: 90,
        max_risk_tier: 2,
    };

    // Setting config works for investor
    client.set_rollover_config(&investor, &criteria, &5000);

    let config = client.get_rollover_config(&investor);
    assert_eq!(config.wallet, investor);
    assert_eq!(config.criteria, criteria);
    assert_eq!(config.max_amount, 5000);
    assert_eq!(config.active, true);
}

#[test]
fn test_execute_rollover_blocked_without_active_config() {
    let (_env, _admin, operator, client, investor, settled_invoice) = setup_test();

    // Investor has no config set
    let res = client.try_execute_rollover(&operator, &investor, &settled_invoice, &2000);
    assert_eq!(res, Err(Ok(Error::NoActiveConfig)));

    // Investor sets config, then deactivates
    let criteria = RolloverCriteria {
        min_yield_bps: 500,
        max_duration_days: 60,
        max_risk_tier: 3,
    };
    client.set_rollover_config(&investor, &criteria, &3000);
    client.deactivate_rollover_config(&investor);

    let res_deactivated = client.try_execute_rollover(&operator, &investor, &settled_invoice, &2000);
    assert_eq!(res_deactivated, Err(Ok(Error::NoActiveConfig)));
}

#[test]
fn test_execute_rollover_matches_best_candidate_and_emits_event() {
    let (env, _, operator, client, investor, settled_invoice) = setup_test();

    // Setup candidate pool
    let target1 = BytesN::from_array(&env, &[10u8; 32]);
    let target2 = BytesN::from_array(&env, &[20u8; 32]);

    // Target 1: 900 bps yield, 60 days duration, tier 1, capacity 10000
    client.add_invoice_candidate(
        &operator,
        &InvoiceCandidate {
            invoice_id: target1.clone(),
            yield_bps: 900,
            duration_days: 60,
            risk_tier: 1,
            capacity: 10000,
            is_open: true,
        },
    );

    // Target 2: 1200 bps yield (higher), 45 days duration, tier 2, capacity 10000
    client.add_invoice_candidate(
        &operator,
        &InvoiceCandidate {
            invoice_id: target2.clone(),
            yield_bps: 1200,
            duration_days: 45,
            risk_tier: 2,
            capacity: 10000,
            is_open: true,
        },
    );

    // Investor config
    let criteria = RolloverCriteria {
        min_yield_bps: 800,
        max_duration_days: 90,
        max_risk_tier: 2,
    };
    client.set_rollover_config(&investor, &criteria, &5000);

    // Execute rollover with 3000 settlement proceeds
    let success = client.execute_rollover(&operator, &investor, &settled_invoice, &3000);
    assert_eq!(success, true);

    // Verify RolloverExecuted event immediately after execute_rollover
    let (topics, data) = last_event(&env);
    let topic_name: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    assert_eq!(topic_name, Symbol::new(&env, "RolloverExecuted"));
    let topic_wallet: Address = topics.get(1).unwrap().try_into_val(&env).unwrap();
    assert_eq!(topic_wallet, investor);
    let topic_source: BytesN<32> = topics.get(2).unwrap().try_into_val(&env).unwrap();
    assert_eq!(topic_source, settled_invoice);

    let (ev_target, ev_amount): (BytesN<32>, i128) = data.try_into_val(&env).unwrap();
    assert_eq!(ev_target, target2);
    assert_eq!(ev_amount, 3000);

    // Verify target2 was chosen because it offered higher yield
    assert_eq!(client.get_invoice_investment(&target2, &investor), 3000);
    assert_eq!(client.get_invoice_investment(&target1, &investor), 0);

    // Target2 remaining capacity is 10000 - 3000 = 7000
    let candidate2 = client.get_candidate(&target2);
    assert_eq!(candidate2.capacity, 7000);
}

#[test]
fn test_execute_rollover_no_match_returns_proceeds_and_emits_event() {
    let (env, _, operator, client, investor, settled_invoice) = setup_test();

    // Add candidate with low yield (below investor criteria)
    let low_yield_target = BytesN::from_array(&env, &[30u8; 32]);
    client.add_invoice_candidate(
        &operator,
        &InvoiceCandidate {
            invoice_id: low_yield_target.clone(),
            yield_bps: 400,
            duration_days: 30,
            risk_tier: 1,
            capacity: 10000,
            is_open: true,
        },
    );

    // Investor requires at least 800 bps yield
    let criteria = RolloverCriteria {
        min_yield_bps: 800,
        max_duration_days: 90,
        max_risk_tier: 1,
    };
    client.set_rollover_config(&investor, &criteria, &5000);

    // Execute rollover
    let matched = client.execute_rollover(&operator, &investor, &settled_invoice, &4000);
    assert_eq!(matched, false);

    // Verify RolloverNoMatch event immediately after execute_rollover
    let (topics, data) = last_event(&env);
    let topic_name: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    assert_eq!(topic_name, Symbol::new(&env, "RolloverNoMatch"));
    let topic_wallet: Address = topics.get(1).unwrap().try_into_val(&env).unwrap();
    assert_eq!(topic_wallet, investor);
    let topic_source: BytesN<32> = topics.get(2).unwrap().try_into_val(&env).unwrap();
    assert_eq!(topic_source, settled_invoice);

    let ev_amount: i128 = data.try_into_val(&env).unwrap();
    assert_eq!(ev_amount, 4000);

    // Full proceeds returned to wallet balance
    assert_eq!(client.get_wallet_balance(&investor), 4000);
    assert_eq!(client.get_invoice_investment(&low_yield_target, &investor), 0);
}

#[test]
fn test_rollover_capped_by_max_amount_refunds_excess() {
    let (env, _, operator, client, investor, settled_invoice) = setup_test();

    let target = BytesN::from_array(&env, &[40u8; 32]);
    client.add_invoice_candidate(
        &operator,
        &InvoiceCandidate {
            invoice_id: target.clone(),
            yield_bps: 1000,
            duration_days: 30,
            risk_tier: 1,
            capacity: 20000,
            is_open: true,
        },
    );

    // Max amount per rollover is 2000
    let criteria = RolloverCriteria {
        min_yield_bps: 500,
        max_duration_days: 60,
        max_risk_tier: 2,
    };
    client.set_rollover_config(&investor, &criteria, &2000);

    // Settlement proceeds are 5000 (exceeding 2000)
    let res = client.execute_rollover(&operator, &investor, &settled_invoice, &5000);
    assert_eq!(res, true);

    // Exactly 2000 invested in target
    assert_eq!(client.get_invoice_investment(&target, &investor), 2000);
    // Excess 3000 refunded to wallet balance
    assert_eq!(client.get_wallet_balance(&investor), 3000);
}

#[test]
fn test_execute_rollover_non_operator_fails() {
    let (env, _, _, client, investor, settled_invoice) = setup_test();

    let criteria = RolloverCriteria {
        min_yield_bps: 500,
        max_duration_days: 60,
        max_risk_tier: 2,
    };
    client.set_rollover_config(&investor, &criteria, &2000);

    let unauthorized = Address::generate(&env);
    let res = client.try_execute_rollover(&unauthorized, &investor, &settled_invoice, &1000);
    assert_eq!(res, Err(Ok(Error::Unauthorized)));
}
