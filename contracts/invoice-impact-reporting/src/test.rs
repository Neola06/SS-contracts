#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    Address, BytesN, Env, Symbol, TryFromVal, TryIntoVal, Val, Vec,
};

fn setup_test() -> (
    Env,
    Address,
    InvoiceImpactReportingContractClient<'static>,
    Address,
    BytesN<32>,
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(InvoiceImpactReportingContract, ());
    let client = InvoiceImpactReportingContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let issuer = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin);

    (env, admin, client, issuer, invoice_id)
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
    let (env, _admin, client, _, _) = setup_test();
    let other_admin = Address::generate(&env);
    let res = client.try_initialize(&other_admin);
    assert_eq!(res, Err(Ok(Error::AlreadyInit)));
}

#[test]
fn test_submit_impact_report_post_settlement_success() {
    let (env, admin, client, issuer, invoice_id) = setup_test();

    // Record invoice as settled
    client.record_settlement(&admin, &invoice_id, &issuer, &true);

    let metrics_hash = BytesN::from_array(&env, &[7u8; 32]);

    // Submit report with Environmental type (0)
    client.submit_impact_report(&issuer, &invoice_id, &metrics_hash, &0);

    let (res_hash, res_type, verified) = client.get_impact_report(&invoice_id);
    assert_eq!(res_hash, metrics_hash);
    assert_eq!(res_type, 0);
    assert_eq!(verified, false);
}

#[test]
fn test_all_four_report_types_accepted() {
    let (env, admin, client, issuer, _) = setup_test();

    let report_types = [
        (ReportType::Environmental as u32, 10u8),
        (ReportType::Social as u32, 11u8),
        (ReportType::Governance as u32, 12u8),
        (ReportType::Combined as u32, 13u8),
    ];

    for (rpt_type, byte_val) in report_types.iter() {
        let inv_id = BytesN::from_array(&env, &[*byte_val; 32]);
        client.record_settlement(&admin, &inv_id, &issuer, &true);

        let hash = BytesN::from_array(&env, &[*byte_val + 50; 32]);
        client.submit_impact_report(&issuer, &inv_id, &hash, rpt_type);

        let (returned_hash, returned_type, verified) = client.get_impact_report(&inv_id);
        assert_eq!(returned_hash, hash);
        assert_eq!(returned_type, *rpt_type);
        assert_eq!(verified, false);
    }
}

#[test]
fn test_invalid_report_type_rejected() {
    let (env, admin, client, issuer, invoice_id) = setup_test();

    client.record_settlement(&admin, &invoice_id, &issuer, &true);
    let hash = BytesN::from_array(&env, &[5u8; 32]);

    let res = client.try_submit_impact_report(&issuer, &invoice_id, &hash, &4);
    assert_eq!(res, Err(Ok(Error::InvalidReportType)));
}

#[test]
fn test_submit_fails_when_not_settled() {
    let (env, admin, client, issuer, invoice_id) = setup_test();

    // Record invoice as NOT settled
    client.record_settlement(&admin, &invoice_id, &issuer, &false);

    let hash = BytesN::from_array(&env, &[9u8; 32]);
    let res = client.try_submit_impact_report(&issuer, &invoice_id, &hash, &1);
    assert_eq!(res, Err(Ok(Error::NotSettled)));
}

#[test]
fn test_submit_fails_for_non_issuer() {
    let (env, admin, client, issuer, invoice_id) = setup_test();

    client.record_settlement(&admin, &invoice_id, &issuer, &true);

    let non_issuer = Address::generate(&env);
    let hash = BytesN::from_array(&env, &[9u8; 32]);
    let res = client.try_submit_impact_report(&non_issuer, &invoice_id, &hash, &1);
    assert_eq!(res, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_submit_fails_duplicate_report() {
    let (env, admin, client, issuer, invoice_id) = setup_test();

    client.record_settlement(&admin, &invoice_id, &issuer, &true);

    let hash = BytesN::from_array(&env, &[3u8; 32]);
    client.submit_impact_report(&issuer, &invoice_id, &hash, &2);

    let res = client.try_submit_impact_report(&issuer, &invoice_id, &hash, &2);
    assert_eq!(res, Err(Ok(Error::ReportAlreadyExists)));
}

#[test]
fn test_verify_impact_report_admin_only_and_success() {
    let (env, admin, client, issuer, invoice_id) = setup_test();

    client.record_settlement(&admin, &invoice_id, &issuer, &true);

    let hash = BytesN::from_array(&env, &[2u8; 32]);
    client.submit_impact_report(&issuer, &invoice_id, &hash, &3);

    // Non-admin verification attempt fails
    let non_admin = Address::generate(&env);
    let fail_res = client.try_verify_impact_report(&non_admin, &invoice_id);
    assert_eq!(fail_res, Err(Ok(Error::Unauthorized)));

    // Admin verifies report
    client.verify_impact_report(&admin, &invoice_id);

    let (res_hash, res_type, verified) = client.get_impact_report(&invoice_id);
    assert_eq!(res_hash, hash);
    assert_eq!(res_type, 3);
    assert_eq!(verified, true);

    // Cannot verify twice
    let double_verify = client.try_verify_impact_report(&admin, &invoice_id);
    assert_eq!(double_verify, Err(Ok(Error::AlreadyVerified)));
}

#[test]
fn test_events_emitted_correctly() {
    let (env, admin, client, issuer, invoice_id) = setup_test();

    client.record_settlement(&admin, &invoice_id, &issuer, &true);

    let hash = BytesN::from_array(&env, &[8u8; 32]);
    client.submit_impact_report(&issuer, &invoice_id, &hash, &1);

    // Verify ImpactReportSubmitted event
    let (topics, data) = last_event(&env);
    let topic_name: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    assert_eq!(topic_name, Symbol::new(&env, "ImpactReportSubmitted"));
    let topic_invoice_id: BytesN<32> = topics.get(1).unwrap().try_into_val(&env).unwrap();
    assert_eq!(topic_invoice_id, invoice_id);

    let (event_issuer, event_hash, event_type): (Address, BytesN<32>, u32) =
        data.try_into_val(&env).unwrap();
    assert_eq!(event_issuer, issuer);
    assert_eq!(event_hash, hash);
    assert_eq!(event_type, 1u32);

    client.verify_impact_report(&admin, &invoice_id);

    // Verify ImpactReportVerified event
    let (v_topics, v_data) = last_event(&env);
    let v_topic_name: Symbol = v_topics.get(0).unwrap().try_into_val(&env).unwrap();
    assert_eq!(v_topic_name, Symbol::new(&env, "ImpactReportVerified"));
    let v_topic_invoice_id: BytesN<32> = v_topics.get(1).unwrap().try_into_val(&env).unwrap();
    assert_eq!(v_topic_invoice_id, invoice_id);

    let v_event_admin: Address = v_data.try_into_val(&env).unwrap();
    assert_eq!(v_event_admin, admin);
}

#[test]
fn test_get_impact_report_not_found() {
    let (env, _, client, _, _) = setup_test();
    let unk_id = BytesN::from_array(&env, &[99u8; 32]);
    let res = client.try_get_impact_report(&unk_id);
    assert_eq!(res, Err(Ok(Error::ReportNotFound)));
}
