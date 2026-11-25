#![cfg(test)]
#![allow(deprecated)]

use soroban_sdk::testutils::{Address as _, Events, Ledger as _};
use soroban_sdk::{Address, Env, String, Symbol, TryFromVal, TryIntoVal, Val, Vec};

use crate::{ContractRecord, Error, UpgradeRecord, UpgradeRegistry, UpgradeRegistryClient};

fn zero_address(env: &Env) -> Address {
    Address::from_string(&String::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    ))
}

fn s(env: &Env, val: &str) -> String {
    String::from_str(env, val)
}

fn setup<'a>(env: &Env) -> (Address, UpgradeRegistryClient<'a>) {
    env.mock_all_auths();
    let contract_id = env.register(UpgradeRegistry, ());
    let client = UpgradeRegistryClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (admin, client)
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
    let env = Env::default();
    let (admin, client) = setup(&env);
    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.try_initialize(&admin), Err(Ok(Error::AlreadyInit)));
}

#[test]
fn test_initialize_rejects_zero_address() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(UpgradeRegistry, ());
    let client = UpgradeRegistryClient::new(&env, &contract_id);
    assert_eq!(
        client.try_initialize(&zero_address(&env)),
        Err(Ok(Error::InvalidAddress))
    );
}

#[test]
fn test_register_contract_stores_address_and_version_and_emits_event() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000);
    let (_admin, client) = setup(&env);

    let name = s(&env, "invoice-escrow");
    let addr = Address::generate(&env);
    client.register_contract(&name, &addr, &1);

    let (topics, data) = last_event(&env);
    let event_name: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    let event_contract: String = topics.get(1).unwrap().try_into_val(&env).unwrap();
    let event_data: (Address, u32) = data.try_into_val(&env).unwrap();

    assert_eq!(event_name, Symbol::new(&env, "ContractRegistered"));
    assert_eq!(event_contract, name);
    assert_eq!(event_data, (addr.clone(), 1));

    let record: ContractRecord = client.get_contract(&name);
    assert_eq!(record.name, name);
    assert_eq!(record.address, addr);
    assert_eq!(record.version, 1);
    assert_eq!(record.updated_at, 1_000);
}

#[test]
fn test_register_contract_requires_admin_auth() {
    let env = Env::default();
    let contract_id = env.register(UpgradeRegistry, ());
    let client = UpgradeRegistryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin);

    // Clear mocked auths so non-authorized call fails auth verification.
    env.set_auths(&[]);
    let name = s(&env, "invoice-escrow");
    let addr = Address::generate(&env);
    assert!(client.try_register_contract(&name, &addr, &1).is_err());
}

#[test]
fn test_register_contract_rejects_empty_name_and_zero_address() {
    let env = Env::default();
    let (_admin, client) = setup(&env);
    let addr = Address::generate(&env);

    assert_eq!(
        client.try_register_contract(&s(&env, ""), &addr, &1),
        Err(Ok(Error::InvalidName))
    );
    assert_eq!(
        client.try_register_contract(&s(&env, "valid"), &zero_address(&env), &1),
        Err(Ok(Error::InvalidAddress))
    );
}

#[test]
fn test_upgrade_updates_current_contract_and_records_full_history_and_event() {
    let env = Env::default();
    let (_admin, client) = setup(&env);
    let name = s(&env, "payment-distributor");

    let addr_v1 = Address::generate(&env);
    let addr_v2 = Address::generate(&env);
    let addr_v3 = Address::generate(&env);

    env.ledger().with_mut(|li| li.timestamp = 1_000);
    client.register_contract(&name, &addr_v1, &1);

    env.ledger().with_mut(|li| li.timestamp = 2_000);
    client.register_contract(&name, &addr_v2, &2);

    let (topics, data) = last_event(&env);
    let event_name: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    let event_contract: String = topics.get(1).unwrap().try_into_val(&env).unwrap();
    let event_data: (Address, Address, u32, u32) = data.try_into_val(&env).unwrap();
    assert_eq!(event_name, Symbol::new(&env, "ContractUpgraded"));
    assert_eq!(event_contract, name);
    assert_eq!(event_data, (addr_v1.clone(), addr_v2.clone(), 1, 2));

    env.ledger().with_mut(|li| li.timestamp = 3_500);
    client.register_contract(&name, &addr_v3, &4);

    let current = client.get_contract(&name);
    assert_eq!(current.address, addr_v3);
    assert_eq!(current.version, 4);
    assert_eq!(current.updated_at, 3_500);

    let history: Vec<UpgradeRecord> = client.get_upgrade_history(&name);
    assert_eq!(history.len(), 3);
    assert_eq!(
        history.get(0).unwrap(),
        UpgradeRecord {
            address: addr_v1,
            version: 1,
            timestamp: 1_000,
        }
    );
    assert_eq!(
        history.get(1).unwrap(),
        UpgradeRecord {
            address: addr_v2,
            version: 2,
            timestamp: 2_000,
        }
    );
    assert_eq!(
        history.get(2).unwrap(),
        UpgradeRecord {
            address: addr_v3,
            version: 4,
            timestamp: 3_500,
        }
    );
    // Name is listed once in list_contracts even after multiple upgrades.
    assert_eq!(client.list_contracts().len(), 1);
}

#[test]
fn test_upgrade_rejects_same_or_lower_version() {
    let env = Env::default();
    let (_admin, client) = setup(&env);
    let name = s(&env, "invoice-token");

    client.register_contract(&name, &Address::generate(&env), &3);

    assert_eq!(
        client.try_register_contract(&name, &Address::generate(&env), &3),
        Err(Ok(Error::VersionMustIncrease))
    );
    assert_eq!(
        client.try_register_contract(&name, &Address::generate(&env), &2),
        Err(Ok(Error::VersionMustIncrease))
    );
}

#[test]
fn test_get_contract_and_history_for_unknown_name() {
    let env = Env::default();
    let (_admin, client) = setup(&env);
    let missing = s(&env, "non-existent");

    assert_eq!(
        client.try_get_contract(&missing),
        Err(Ok(Error::ContractNotFound))
    );
    assert_eq!(
        client.try_get_upgrade_history(&missing),
        Err(Ok(Error::ContractNotFound))
    );
}

#[test]
fn test_check_compatibility_version_comparisons() {
    let env = Env::default();
    let (_admin, client) = setup(&env);

    let escrow = s(&env, "invoice-escrow");
    let missing = s(&env, "missing-contract");

    // Unregistered contract is incompatible for any version.
    assert!(!client.check_compatibility(&missing, &0));
    assert!(!client.check_compatibility(&missing, &1));

    client.register_contract(&escrow, &Address::generate(&env), &3);

    // Less than current version -> compatible
    assert!(client.check_compatibility(&escrow, &0));
    assert!(client.check_compatibility(&escrow, &1));
    assert!(client.check_compatibility(&escrow, &2));
    // Equal to current version -> compatible
    assert!(client.check_compatibility(&escrow, &3));
    // Greater than current version -> incompatible
    assert!(!client.check_compatibility(&escrow, &4));
    assert!(!client.check_compatibility(&escrow, &100));

    // After upgrade to v5, previously incompatible min_version=4 becomes compatible.
    client.register_contract(&escrow, &Address::generate(&env), &5);
    assert!(client.check_compatibility(&escrow, &4));
    assert!(client.check_compatibility(&escrow, &5));
    assert!(!client.check_compatibility(&escrow, &6));
}

#[test]
fn test_multiple_contracts_tracked_independently() {
    let env = Env::default();
    let (_admin, client) = setup(&env);

    let name_a = s(&env, "contract-a");
    let name_b = s(&env, "contract-b");
    let addr_a = Address::generate(&env);
    let addr_b = Address::generate(&env);

    client.register_contract(&name_a, &addr_a, &1);
    client.register_contract(&name_b, &addr_b, &10);

    assert_eq!(client.get_contract(&name_a).address, addr_a);
    assert_eq!(client.get_contract(&name_b).address, addr_b);
    assert!(client.check_compatibility(&name_b, &10));
    assert!(!client.check_compatibility(&name_a, &2));
    assert_eq!(client.list_contracts().len(), 2);
}
