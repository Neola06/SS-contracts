#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    Address, BytesN, Env, String, Symbol, TryFromVal, TryIntoVal, Val, Vec,
};

fn setup_test() -> (
    Env,
    Address,
    InvoiceGreylistingContractClient<'static>,
    BytesN<32>,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register(InvoiceGreylistingContract, ());
    let client = InvoiceGreylistingContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin);

    (env, admin, client, invoice_id)
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
    let (env, _, client, _) = setup_test();
    let other_admin = Address::generate(&env);
    assert_eq!(client.try_initialize(&other_admin), Err(Ok(Error::AlreadyInit)));
}

#[test]
fn test_greylist_invoice_and_block_invest() {
    let (env, admin, client, invoice_id) = setup_test();

    let reason = String::from_str(&env, "Regulatory compliance hold");
    let duration = 3600u64; // 1 hour

    // Admin greylists invoice
    client.greylist_invoice(&admin, &invoice_id, &reason, &duration);

    // Verify event immediately after greylisting
    let (topics, data) = last_event(&env);
    let topic_name: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    assert_eq!(topic_name, Symbol::new(&env, "InvoiceGreylisted"));
    let topic_id: BytesN<32> = topics.get(1).unwrap().try_into_val(&env).unwrap();
    assert_eq!(topic_id, invoice_id);

    let (ev_reason, ev_duration): (String, u64) = data.try_into_val(&env).unwrap();
    assert_eq!(ev_reason, reason);
    assert_eq!(ev_duration, duration);

    assert_eq!(client.is_greylisted(&invoice_id), true);

    // New investment must be blocked with InvoiceGreylisted error
    let investor = Address::generate(&env);
    let res = client.try_invest(&investor, &invoice_id, &1000);
    assert_eq!(res, Err(Ok(Error::InvoiceGreylisted)));
}

#[test]
fn test_existing_holders_can_exit_during_greylist() {
    let (env, admin, client, invoice_id) = setup_test();

    let holder = Address::generate(&env);
    client.set_holder_balance(&admin, &invoice_id, &holder, &5000);

    // Greylist the invoice
    let reason = String::from_str(&env, "Audit in progress");
    client.greylist_invoice(&admin, &invoice_id, &reason, &3600);
    assert_eq!(client.is_greylisted(&invoice_id), true);

    // Existing holder can exit during hold
    let exit_res = client.try_exit_investment(&holder, &invoice_id, &2000);
    assert!(exit_res.is_ok());

    // New investment is still blocked
    let new_investor = Address::generate(&env);
    assert_eq!(
        client.try_invest(&new_investor, &invoice_id, &1000),
        Err(Ok(Error::InvoiceGreylisted))
    );
}

#[test]
fn test_auto_clear_after_duration_expires() {
    let (env, admin, client, invoice_id) = setup_test();

    let reason = String::from_str(&env, "Hold 100 seconds");
    let duration = 100u64;

    client.greylist_invoice(&admin, &invoice_id, &reason, &duration);
    assert_eq!(client.is_greylisted(&invoice_id), true);

    // Advance ledger timestamp beyond duration
    env.ledger().set_timestamp(1_000 + 101);

    // Now auto-clear applies, is_greylisted returns false
    assert_eq!(client.is_greylisted(&invoice_id), false);

    // Investment now succeeds
    let investor = Address::generate(&env);
    let res = client.try_invest(&investor, &invoice_id, &1000);
    assert!(res.is_ok());
}

#[test]
fn test_clear_greylist_admin_only_and_success() {
    let (env, admin, client, invoice_id) = setup_test();

    let reason = String::from_str(&env, "Temporary hold");
    client.greylist_invoice(&admin, &invoice_id, &reason, &5000);
    assert_eq!(client.is_greylisted(&invoice_id), true);

    // Non-admin cannot clear
    let non_admin = Address::generate(&env);
    assert_eq!(
        client.try_clear_greylist(&non_admin, &invoice_id),
        Err(Ok(Error::Unauthorized))
    );

    // Admin clears greylist
    client.clear_greylist(&admin, &invoice_id);

    // Verify GreylistCleared event immediately after clearing
    let (topics, _) = last_event(&env);
    let topic_name: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    assert_eq!(topic_name, Symbol::new(&env, "GreylistCleared"));
    let topic_id: BytesN<32> = topics.get(1).unwrap().try_into_val(&env).unwrap();
    assert_eq!(topic_id, invoice_id);

    assert_eq!(client.is_greylisted(&invoice_id), false);

    // Investment succeeds now
    let investor = Address::generate(&env);
    assert!(client.try_invest(&investor, &invoice_id, &1000).is_ok());
}

#[test]
fn test_greylist_non_admin_fails() {
    let (env, _, client, invoice_id) = setup_test();

    let non_admin = Address::generate(&env);
    let reason = String::from_str(&env, "Unauthorized attempt");
    let res = client.try_greylist_invoice(&non_admin, &invoice_id, &reason, &3600);
    assert_eq!(res, Err(Ok(Error::Unauthorized)));
}
