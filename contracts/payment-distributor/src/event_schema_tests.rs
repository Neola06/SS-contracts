//! Event Schema Snapshot Tests for Payment Distributor
//!
//! These tests capture the exact topic and data shape for each public mutation event.
//! They fail if a topic is renamed, a field is removed, or field order changes.

#![cfg(test)]
#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]

use super::*;
use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::{token, Address, BytesN, Env, Symbol, TryFromVal, Val, Vec};

use invoice_escrow::{InvoiceEscrow, InvoiceEscrowClient};
use invoice_token::{InvoiceToken, InvoiceTokenClient};

/// Parse event into structured format for comparison
fn parse_event(env: &Env, event: &soroban_sdk::xdr::ContractEvent) -> (Vec<Val>, Val) {
    let soroban_sdk::xdr::ContractEventBody::V0(v0) = &event.body;
    let topics = Vec::<Val>::try_from_val(
        env,
        &soroban_sdk::xdr::ScVal::Vec(Some(v0.topics.clone().into())),
    )
    .unwrap();
    let data = Val::try_from_val(env, &v0.data).unwrap();
    (topics, data)
}

fn assert_val_type<T>(env: &Env, val: &Val)
where
    T: TryFromVal<Env, Val>,
{
    assert!(T::try_from_val(env, val).is_ok());
}

fn test_commitment(env: &Env, data: &str) -> BytesN<32> {
    let mut array = [0u8; 32];
    let bytes = data.as_bytes();
    let len = bytes.len().min(32);
    array[..len].copy_from_slice(&bytes[..len]);
    BytesN::from_array(env, &array)
}

#[test]
fn event_schema_snapshot_payment_distributed() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let escrow = InvoiceEscrowClient::new(&env, &escrow_id);

    let distributor_id = env.register_contract(None, PaymentDistributor);
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);

    let inv_token_id = env.register_contract(None, InvoiceToken);
    let inv_token = InvoiceTokenClient::new(&env, &inv_token_id);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin);
    let payment_token = token::Client::new(&env, &token_id.address());
    let payment_asset = soroban_sdk::token::StellarAssetClient::new(&env, &token_id.address());

    let invoice_id = Symbol::new(&env, "INV_DIST_EVT");

    inv_token.initialize(
        &admin,
        &soroban_sdk::String::from_str(&env, "Invoice Dist"),
        &soroban_sdk::String::from_str(&env, "INVD"),
        &7,
        &invoice_id,
        &escrow_id,
    );

    escrow.initialize(&admin, &0);
    distributor.initialize(&admin);
    distributor.set_escrow_contract(&admin, &escrow_id);
    escrow.set_payment_distributor(&distributor_id);

    payment_asset.mint(&buyer, &1000);
    payment_asset.mint(&payer, &1000);

    escrow.create_escrow_legacy(
        &invoice_id,
        &seller,
        &payer,
        &1000,
        &1000,
        &1_000_000,
        &payment_token.address,
        &inv_token.address,
        &test_commitment(&env, "test"),
        &None,
        &None,
    );
    escrow.fund_escrow_legacy(&invoice_id, &buyer, &1000);

    let _ = env.events().all();

    escrow.record_payment(&invoice_id, &payer, &1000);

    let contract_events = env.events().all();
    let events = contract_events.events();
    let mut found = false;
    for event in events.iter() {
        let (topics, _) = parse_event(&env, &event);
        if !topics.is_empty() {
            if let Ok(sym) = Symbol::try_from_val(&env, &topics.get(0).unwrap()) {
                if sym == Symbol::new(&env, "PaymentDistributed") {
                    found = true;

                    // The contract ID is stored separately from the event topics.
                    assert!(
                        topics.len() >= 3,
                        "PaymentDistributed should have at least 3 topics"
                    );

                    // Verify data: (recipients, amounts, escrow_status, timestamp)
                    let (_, data) = parse_event(&env, &event);
                    let data_vec = Vec::<Val>::try_from_val(&env, &data).unwrap();
                    assert_eq!(
                        data_vec.len(),
                        4,
                        "PaymentDistributed data should have 4 fields"
                    );

                    // recipients: Vec<Address>
                    assert_val_type::<Vec<Address>>(&env, &data_vec.get(0).unwrap());
                    // amounts: Vec<i128>
                    assert_val_type::<Vec<i128>>(&env, &data_vec.get(1).unwrap());
                    // escrow_status: u32
                    assert_val_type::<u32>(&env, &data_vec.get(2).unwrap());
                    // timestamp: u64
                    assert_val_type::<u64>(&env, &data_vec.get(3).unwrap());

                    break;
                }
            }
        }
    }
    assert!(found, "PaymentDistributed event not found");
}

#[test]
fn event_schema_snapshot_refund_distributed() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let debtor = Address::generate(&env);
    let buyer = Address::generate(&env);

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let escrow = InvoiceEscrowClient::new(&env, &escrow_id);

    let distributor_id = env.register_contract(None, PaymentDistributor);
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);

    let inv_token_id = env.register_contract(None, InvoiceToken);
    let inv_token = InvoiceTokenClient::new(&env, &inv_token_id);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin);
    let payment_token = token::Client::new(&env, &token_id.address());
    let payment_asset = soroban_sdk::token::StellarAssetClient::new(&env, &token_id.address());

    let invoice_id = Symbol::new(&env, "INV_REF_EVT");

    inv_token.initialize(
        &admin,
        &soroban_sdk::String::from_str(&env, "Invoice Ref"),
        &soroban_sdk::String::from_str(&env, "INVR"),
        &7,
        &invoice_id,
        &escrow_id,
    );

    escrow.initialize(&admin, &0);
    distributor.initialize(&admin);
    distributor.set_escrow_contract(&admin, &escrow_id);
    escrow.set_payment_distributor(&distributor_id);

    payment_asset.mint(&buyer, &1000);

    escrow.create_escrow_legacy(
        &invoice_id,
        &seller,
        &debtor,
        &1000,
        &1000,
        &1_000_000,
        &payment_token.address,
        &inv_token.address,
        &test_commitment(&env, "test"),
        &None,
        &None,
    );
    escrow.fund_escrow_legacy(&invoice_id, &buyer, &1000);

    let _ = env.events().all();

    // Fast-forward time
    env.ledger().with_mut(|li| {
        li.timestamp = 2_000_000;
    });

    escrow.refund_escrow(&invoice_id);

    let contract_events = env.events().all();
    let events = contract_events.events();
    let mut found = false;
    for event in events.iter() {
        let (topics, _) = parse_event(&env, &event);
        if !topics.is_empty() {
            if let Ok(sym) = Symbol::try_from_val(&env, &topics.get(0).unwrap()) {
                if sym == Symbol::new(&env, "refund_distributed") {
                    found = true;

                    // The contract ID is stored separately from the event topics.
                    assert!(
                        topics.len() >= 3,
                        "refund_distributed should have at least 3 topics"
                    );

                    // Verify data: (recipients, amounts)
                    let (_, data) = parse_event(&env, &event);
                    let data_vec = Vec::<Val>::try_from_val(&env, &data).unwrap();
                    assert_eq!(
                        data_vec.len(),
                        2,
                        "refund_distributed data should have 2 fields"
                    );

                    assert_val_type::<Vec<Address>>(&env, &data_vec.get(0).unwrap());
                    assert_val_type::<Vec<i128>>(&env, &data_vec.get(1).unwrap());

                    break;
                }
            }
        }
    }
    assert!(found, "refund_distributed event not found");
}

#[test]
fn event_schema_snapshot_asset_distributed() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let distributor_id = env.register_contract(None, PaymentDistributor);
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin);
    let payment_token = token::Client::new(&env, &token_id.address());
    let payment_asset = soroban_sdk::token::StellarAssetClient::new(&env, &token_id.address());

    distributor.initialize(&admin);

    // Give contract some tokens
    payment_asset.mint(&distributor_id, &1000);

    let split = crate::types::DistributionSplit {
        recipients: soroban_sdk::vec![&env, admin.clone()],
        shares_bps: soroban_sdk::vec![&env, 10000],
        referral_bps: 0,
        referral: None,
    };

    let route = crate::types::AssetRoute {
        token: payment_token.address.clone(),
        amount: 1000,
        split,
    };

    let _ = env.events().all();

    distributor.distribute_multi_asset(&admin, &soroban_sdk::vec![&env, route]);

    let contract_events = env.events().all();
    let events = contract_events.events();
    let mut found = false;
    for event in events.iter() {
        let (topics, _) = parse_event(&env, &event);
        if !topics.is_empty() {
            if let Ok(sym) = Symbol::try_from_val(&env, &topics.get(0).unwrap()) {
                if sym == Symbol::new(&env, "AssetDistributed") {
                    found = true;

                    // The contract ID is stored separately from the event topics.
                    assert!(
                        topics.len() >= 2,
                        "AssetDistributed should have at least 2 topics"
                    );

                    // Verify data: (recipients, amounts, total)
                    let (_, data) = parse_event(&env, &event);
                    let data_vec = Vec::<Val>::try_from_val(&env, &data).unwrap();
                    assert_eq!(
                        data_vec.len(),
                        3,
                        "AssetDistributed data should have 3 fields"
                    );

                    assert_val_type::<Vec<Address>>(&env, &data_vec.get(0).unwrap());
                    assert_val_type::<Vec<i128>>(&env, &data_vec.get(1).unwrap());
                    assert_val_type::<i128>(&env, &data_vec.get(2).unwrap());

                    break;
                }
            }
        }
    }
    assert!(found, "AssetDistributed event not found");
}

#[test]
fn event_schema_snapshot_emergency_withdrawal() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let to = Address::generate(&env);

    let distributor_id = env.register_contract(None, PaymentDistributor);
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin);
    let payment_token = token::Client::new(&env, &token_id.address());
    let payment_asset = soroban_sdk::token::StellarAssetClient::new(&env, &token_id.address());

    distributor.initialize(&admin);

    // Give contract some tokens
    payment_asset.mint(&distributor_id, &1000);

    let _ = env.events().all();

    distributor.emergency_withdraw(&admin, &payment_token.address, &to);

    let contract_events = env.events().all();
    let events = contract_events.events();
    let mut found = false;
    for event in events.iter() {
        let (topics, _) = parse_event(&env, &event);
        if !topics.is_empty() {
            if let Ok(sym) = Symbol::try_from_val(&env, &topics.get(0).unwrap()) {
                if sym == Symbol::new(&env, "EmergencyWithdrawal") {
                    found = true;

                    // The contract ID is stored separately from the event topics.
                    assert!(
                        topics.len() >= 2,
                        "EmergencyWithdrawal should have at least 2 topics"
                    );

                    // Verify data: (admin, to, amount)
                    let (_, data) = parse_event(&env, &event);
                    let data_vec = Vec::<Val>::try_from_val(&env, &data).unwrap();
                    assert_eq!(
                        data_vec.len(),
                        3,
                        "EmergencyWithdrawal data should have 3 fields"
                    );

                    assert_val_type::<Address>(&env, &data_vec.get(0).unwrap());
                    assert_val_type::<Address>(&env, &data_vec.get(1).unwrap());
                    assert_val_type::<i128>(&env, &data_vec.get(2).unwrap());

                    break;
                }
            }
        }
    }
    assert!(found, "EmergencyWithdrawal event not found");
}

#[test]
fn event_schema_snapshot_dust_swept() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let to = Address::generate(&env);

    let distributor_id = env.register_contract(None, PaymentDistributor);
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin);
    let payment_token = token::Client::new(&env, &token_id.address());
    let payment_asset = soroban_sdk::token::StellarAssetClient::new(&env, &token_id.address());

    distributor.initialize(&admin);

    // Give contract some tokens
    payment_asset.mint(&distributor_id, &1000);

    let _ = env.events().all();

    distributor.sweep_dust(&admin, &payment_token.address);

    let contract_events = env.events().all();
    let events = contract_events.events();
    let mut found = false;
    for event in events.iter() {
        let (topics, _) = parse_event(&env, &event);
        if !topics.is_empty() {
            if let Ok(sym) = Symbol::try_from_val(&env, &topics.get(0).unwrap()) {
                if sym == Symbol::new(&env, "DustSwept") {
                    found = true;

                    // The contract ID is stored separately from the event topics.
                    assert!(topics.len() >= 2, "DustSwept should have at least 2 topics");

                    // Verify data: (admin, to, amount)
                    let (_, data) = parse_event(&env, &event);
                    let data_vec = Vec::<Val>::try_from_val(&env, &data).unwrap();
                    assert_eq!(data_vec.len(), 3, "DustSwept data should have 3 fields");

                    assert_val_type::<Address>(&env, &data_vec.get(0).unwrap());
                    assert_val_type::<Address>(&env, &data_vec.get(1).unwrap());
                    assert_val_type::<i128>(&env, &data_vec.get(2).unwrap());

                    break;
                }
            }
        }
    }
    assert!(found, "DustSwept event not found");
}

#[test]
fn event_schema_snapshot_excess_refunded() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let debtor = Address::generate(&env);
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let escrow = InvoiceEscrowClient::new(&env, &escrow_id);

    let distributor_id = env.register_contract(None, PaymentDistributor);
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);

    let inv_token_id = env.register_contract(None, InvoiceToken);
    let inv_token = InvoiceTokenClient::new(&env, &inv_token_id);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin);
    let payment_token = token::Client::new(&env, &token_id.address());
    let payment_asset = soroban_sdk::token::StellarAssetClient::new(&env, &token_id.address());

    let invoice_id = Symbol::new(&env, "INV_EXC_EVT");

    inv_token.initialize(
        &admin,
        &soroban_sdk::String::from_str(&env, "Invoice Exc"),
        &soroban_sdk::String::from_str(&env, "INVE"),
        &7,
        &invoice_id,
        &escrow_id,
    );

    escrow.initialize(&admin, &0);
    distributor.initialize(&admin);
    distributor.set_escrow_contract(&admin, &escrow_id);
    escrow.set_payment_distributor(&distributor_id);

    // Overfund the distributor
    payment_asset.mint(&buyer, &2000);
    payment_asset.mint(&payer, &2000);

    escrow.create_escrow_legacy(
        &invoice_id,
        &seller,
        &payer,
        &1000,
        &1000,
        &1_000_000,
        &payment_token.address,
        &inv_token.address,
        &test_commitment(&env, "test"),
        &None,
        &None,
    );
    escrow.fund_escrow_legacy(&invoice_id, &buyer, &1000);

    let _ = env.events().all();

    escrow.record_payment(&invoice_id, &payer, &1000);

    // Simulate surplus funds left by an over-deposit from the escrow.
    payment_asset.mint(&distributor_id, &100);

    // Now call refund_excess
    distributor.refund_excess(&escrow_id, &payment_token.address, &invoice_id);

    let contract_events = env.events().all();
    let events = contract_events.events();
    let mut found = false;
    for event in events.iter() {
        let (topics, _) = parse_event(&env, &event);
        if !topics.is_empty() {
            if let Ok(sym) = Symbol::try_from_val(&env, &topics.get(0).unwrap()) {
                if sym == Symbol::new(&env, "ExcessRefunded") {
                    found = true;

                    // The contract ID is stored separately from the event topics.
                    assert!(
                        topics.len() >= 3,
                        "ExcessRefunded should have at least 3 topics"
                    );

                    // Verify data: (escrow, amount)
                    let (_, data) = parse_event(&env, &event);
                    let data_vec = Vec::<Val>::try_from_val(&env, &data).unwrap();
                    assert_eq!(
                        data_vec.len(),
                        2,
                        "ExcessRefunded data should have 2 fields"
                    );

                    assert_val_type::<Address>(&env, &data_vec.get(0).unwrap());
                    assert_val_type::<i128>(&env, &data_vec.get(1).unwrap());

                    break;
                }
            }
        }
    }
    assert!(found, "ExcessRefunded event not found");
}

#[test]
fn event_schema_snapshot_initialized() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);

    let distributor_id = env.register_contract(None, PaymentDistributor);
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);

    distributor.initialize(&admin);

    let contract_events = env.events().all();
    let events = contract_events.events();
    let mut found = false;
    for event in events.iter() {
        let (topics, _) = parse_event(&env, &event);
        if !topics.is_empty() {
            if let Ok(sym) = Symbol::try_from_val(&env, &topics.get(0).unwrap()) {
                if sym == Symbol::new(&env, "initialized") {
                    found = true;

                    // The contract ID is stored separately from the event topics.
                    assert!(
                        topics.len() >= 1,
                        "initialized should have at least 1 topic"
                    );

                    // Verify data: admin
                    let (_, data) = parse_event(&env, &event);
                    assert_val_type::<Address>(&env, &data);

                    break;
                }
            }
        }
    }
    assert!(found, "initialized event not found");
}
