#![cfg(test)]
#![allow(deprecated)]

use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{symbol_short, Address, BytesN, Env, Symbol, TryFromVal, TryIntoVal, Val, Vec};

use crate::{
    Error, InvoiceTrancheContract, InvoiceTrancheContractClient, SettlementResult, TrancheType,
};

fn invoice_id(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

fn setup_tranche<'a>(
    env: &Env,
    seed: u8,
    senior_filled: i128,
    junior_filled: i128,
    junior_yield_premium_bps: u32,
) -> (Address, BytesN<32>, InvoiceTrancheContractClient<'a>) {
    env.mock_all_auths();
    let contract_id = env.register(InvoiceTrancheContract, ());
    let client = InvoiceTrancheContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let issuer = Address::generate(env);
    let senior_investor = Address::generate(env);
    let junior_investor = Address::generate(env);
    let id = invoice_id(env, seed);

    client.initialize(&admin);
    client.create_tranches(&issuer, &id, &60, &junior_yield_premium_bps);

    if senior_filled > 0 {
        client.invest_tranche(
            &senior_investor,
            &id,
            &(TrancheType::Senior as u32),
            &senior_filled,
        );
    }
    if junior_filled > 0 {
        client.invest_tranche(
            &junior_investor,
            &id,
            &(TrancheType::Junior as u32),
            &junior_filled,
        );
    }

    (issuer, id, client)
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
fn test_issue_example_60_40_settlement_110_assigns_every_unit() {
    let env = Env::default();
    let (_issuer, id, client) = setup_tranche(&env, 1, 60, 40, 1_000);

    let result = client.settle_tranches(&id, &110);
    assert_eq!(
        result,
        SettlementResult {
            senior_payout: 60,
            junior_payout: 41,
            issuer_surplus: 9,
        }
    );
    assert_eq!(
        result.senior_payout + result.junior_payout + result.issuer_surplus,
        110
    );

    let (topics, data) = last_event(&env);
    let event_sym: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    let event_id: BytesN<32> = topics.get(1).unwrap().try_into_val(&env).unwrap();
    let event_payouts: (i128, i128, i128) = data.try_into_val(&env).unwrap();
    assert_eq!(event_sym, symbol_short!("tr_sett"));
    assert_eq!(event_id, id);
    assert_eq!(event_payouts, (60, 41, 9));

    let status = client.get_tranche_status(&id);
    assert!(status.is_settled);
    assert_eq!(status.senior_payout, 60);
    assert_eq!(status.junior_payout, 41);
    assert_eq!(status.issuer_surplus, 9);
}

#[test]
fn test_total_loss_and_partial_recovery_prioritizes_senior() {
    let env = Env::default();

    // Total loss (settlement = 0)
    let (_issuer, id0, client0) = setup_tranche(&env, 2, 60, 40, 1_000);
    let r0 = client0.settle_tranches(&id0, &0);
    assert_eq!(
        r0,
        SettlementResult {
            senior_payout: 0,
            junior_payout: 0,
            issuer_surplus: 0,
        }
    );

    // Partial senior recovery (settlement = 35 < 60)
    let (_issuer, id1, client1) = setup_tranche(&env, 3, 60, 40, 1_000);
    let r1 = client1.settle_tranches(&id1, &35);
    assert_eq!(
        r1,
        SettlementResult {
            senior_payout: 35,
            junior_payout: 0,
            issuer_surplus: 0,
        }
    );

    // Exact senior recovery (settlement = 60)
    let (_issuer, id2, client2) = setup_tranche(&env, 4, 60, 40, 1_000);
    let r2 = client2.settle_tranches(&id2, &60);
    assert_eq!(
        r2,
        SettlementResult {
            senior_payout: 60,
            junior_payout: 0,
            issuer_surplus: 0,
        }
    );

    // Partial junior recovery (settlement = 85 -> senior 60, junior 25, issuer 0)
    let (_issuer, id3, client3) = setup_tranche(&env, 5, 60, 40, 1_000);
    let r3 = client3.settle_tranches(&id3, &85);
    assert_eq!(
        r3,
        SettlementResult {
            senior_payout: 60,
            junior_payout: 25,
            issuer_surplus: 0,
        }
    );

    // Exact principal recovery (settlement = 100 -> senior 60, junior 40, issuer 0)
    let (_issuer, id4, client4) = setup_tranche(&env, 6, 60, 40, 1_000);
    let r4 = client4.settle_tranches(&id4, &100);
    assert_eq!(
        r4,
        SettlementResult {
            senior_payout: 60,
            junior_payout: 40,
            issuer_surplus: 0,
        }
    );
}

#[test]
fn test_dust_goes_explicitly_to_issuer_surplus() {
    let env = Env::default();
    // Surplus = 15, bps = 1_000 (10%) -> bonus = floor(1.5) = 1, dust + remainder = 14
    let (_issuer, id, client) = setup_tranche(&env, 7, 60, 40, 1_000);
    let res = client.settle_tranches(&id, &115);
    assert_eq!(
        res,
        SettlementResult {
            senior_payout: 60,
            junior_payout: 41,
            issuer_surplus: 14,
        }
    );
    assert_eq!(
        res.senior_payout + res.junior_payout + res.issuer_surplus,
        115
    );

    // Tiny surplus = 3, bps = 2_500 (25%) -> bonus = floor(0.75) = 0, all 3 dust goes to issuer
    let (_issuer, id_tiny, client_tiny) = setup_tranche(&env, 8, 60, 40, 2_500);
    let res_tiny = client_tiny.settle_tranches(&id_tiny, &103);
    assert_eq!(
        res_tiny,
        SettlementResult {
            senior_payout: 60,
            junior_payout: 40,
            issuer_surplus: 3,
        }
    );
    assert_eq!(
        res_tiny.senior_payout + res_tiny.junior_payout + res_tiny.issuer_surplus,
        103
    );
}

#[test]
fn test_zero_and_maximum_premiums_follow_surplus_ownership() {
    let env = Env::default();

    // Zero premium (0 bps): 100% of surplus belongs to issuer.
    let (_issuer, id_zero, client_zero) = setup_tranche(&env, 9, 60, 40, 0);
    let res_zero = client_zero.settle_tranches(&id_zero, &150);
    assert_eq!(
        res_zero,
        SettlementResult {
            senior_payout: 60,
            junior_payout: 40,
            issuer_surplus: 50,
        }
    );

    // Maximum premium (10_000 bps = 100%): 100% of surplus goes to junior, 0 to issuer.
    let (_issuer, id_max, client_max) = setup_tranche(&env, 10, 60, 40, 10_000);
    let res_max = client_max.settle_tranches(&id_max, &150);
    assert_eq!(
        res_max,
        SettlementResult {
            senior_payout: 60,
            junior_payout: 90,
            issuer_surplus: 0,
        }
    );

    // Over 10_000 bps rejected at creation.
    let contract_id = env.register(InvoiceTrancheContract, ());
    let client_bad = InvoiceTrancheContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let issuer = Address::generate(&env);
    client_bad.initialize(&admin);
    assert_eq!(
        client_bad.try_create_tranches(&issuer, &invoice_id(&env, 11), &60, &10_001),
        Err(Ok(Error::InvalidTrancheConfig))
    );
}

#[test]
fn test_no_junior_investments_allocates_surplus_to_issuer() {
    let env = Env::default();
    let (_issuer, id, client) = setup_tranche(&env, 12, 60, 0, 5_000);

    let res = client.settle_tranches(&id, &100);
    assert_eq!(
        res,
        SettlementResult {
            senior_payout: 60,
            junior_payout: 0,
            issuer_surplus: 40,
        }
    );
}

#[test]
fn test_invalid_arithmetic_leaves_settlement_flags_and_payouts_unchanged() {
    let env = Env::default();
    let (_issuer, id, client) = setup_tranche(&env, 13, 60, 40, 1_000);

    // Negative settlement amount rejected cleanly.
    assert_eq!(
        client.try_settle_tranches(&id, &-1),
        Err(Ok(Error::InvalidAmount))
    );
    let status_after_neg = client.get_tranche_status(&id);
    assert!(!status_after_neg.is_settled);
    assert_eq!(status_after_neg.senior_payout, 0);
    assert_eq!(status_after_neg.junior_payout, 0);
    assert_eq!(status_after_neg.issuer_surplus, 0);

    // Overflowing bonus multiplication (`i128::MAX` surplus * 1_000 bps) fails atomically.
    assert_eq!(
        client.try_settle_tranches(&id, &i128::MAX),
        Err(Ok(Error::ArithmeticOverflow))
    );
    let status_after_overflow = client.get_tranche_status(&id);
    assert!(!status_after_overflow.is_settled);
    assert_eq!(status_after_overflow.senior_payout, 0);
    assert_eq!(status_after_overflow.junior_payout, 0);
    assert_eq!(status_after_overflow.issuer_surplus, 0);

    // Valid settlement still succeeds afterwards, and double settlement is rejected.
    let ok_res = client.settle_tranches(&id, &110);
    assert_eq!(ok_res.issuer_surplus, 9);
    assert_eq!(
        client.try_settle_tranches(&id, &110),
        Err(Ok(Error::AlreadySettled))
    );
}

#[test]
fn test_independent_conservation_across_waterfall_matrix() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(InvoiceTrancheContract, ());
    let client = InvoiceTrancheContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let issuer = Address::generate(&env);
    let senior_investor = Address::generate(&env);
    let junior_investor = Address::generate(&env);
    client.initialize(&admin);

    let senior_cases = [0_i128, 1, 60, 1_000];
    let junior_cases = [0_i128, 1, 40, 500];
    let bps_cases = [0_u32, 1, 1_000, 3_333, 10_000];
    let settlement_cases = [0_i128, 1, 30, 60, 80, 100, 101, 117, 2_000];

    let mut counter: u32 = 0;
    for &senior_filled in &senior_cases {
        for &junior_filled in &junior_cases {
            for &bps in &bps_cases {
                for &settlement in &settlement_cases {
                    counter += 1;
                    let mut raw = [0_u8; 32];
                    raw[0..4].copy_from_slice(&counter.to_be_bytes());
                    let id = BytesN::from_array(&env, &raw);

                    client.create_tranches(&issuer, &id, &60, &bps);
                    if senior_filled > 0 {
                        client.invest_tranche(
                            &senior_investor,
                            &id,
                            &(TrancheType::Senior as u32),
                            &senior_filled,
                        );
                    }
                    if junior_filled > 0 {
                        client.invest_tranche(
                            &junior_investor,
                            &id,
                            &(TrancheType::Junior as u32),
                            &junior_filled,
                        );
                    }

                    let res = client.settle_tranches(&id, &settlement);

                    // Conservation: every unit of settlement_amount is accounted for.
                    assert_eq!(
                        res.senior_payout + res.junior_payout + res.issuer_surplus,
                        settlement
                    );
                    assert!(res.senior_payout >= 0);
                    assert!(res.junior_payout >= 0);
                    assert!(res.issuer_surplus >= 0);

                    // Senior priority invariant.
                    assert!(res.senior_payout <= senior_filled);
                    if res.junior_payout > 0 || res.issuer_surplus > 0 {
                        assert_eq!(res.senior_payout, senior_filled);
                    }
                    if res.issuer_surplus > 0 && junior_filled > 0 {
                        assert!(res.junior_payout >= junior_filled);
                    }

                    // Persisted status matches returned settlement result.
                    let status = client.get_tranche_status(&id);
                    assert!(status.is_settled);
                    assert_eq!(status.senior_payout, res.senior_payout);
                    assert_eq!(status.junior_payout, res.junior_payout);
                    assert_eq!(status.issuer_surplus, res.issuer_surplus);
                }
            }
        }
    }
}
