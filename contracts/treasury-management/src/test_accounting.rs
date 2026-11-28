#![cfg(test)]
//! Reconciliation tests for lifetime receipts, allocations and availability.

extern crate std;

use proptest::prelude::*;
use soroban_sdk::testutils::{Address as _, EnvTestConfig, Ledger};
use soroban_sdk::{Address, Env, String};

use crate::{
    Error, SourceAccounting, TreasuryAccounting, TreasuryManagement, TreasuryManagementClient,
};

fn setup<'a>(env: &Env) -> (Address, TreasuryManagementClient<'a>) {
    env.mock_all_auths();
    let contract_id = env.register(TreasuryManagement, ());
    let client = TreasuryManagementClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (admin, client)
}

fn purpose(env: &Env) -> String {
    String::from_str(env, "operations")
}

fn source_of(accounting: &TreasuryAccounting, source: &Address) -> SourceAccounting {
    accounting
        .sources
        .iter()
        .find(|s| &s.source == source)
        .expect("source missing from accounting")
}

fn debits_of(client: &TreasuryManagementClient, index: u32) -> std::vec::Vec<(Address, i128)> {
    client
        .get_allocation_history(&index, &1)
        .get(0)
        .unwrap()
        .debits
        .iter()
        .map(|d| (d.source, d.amount))
        .collect()
}

/// Check every reconciliation invariant the contract documents.
fn assert_reconciled(client: &TreasuryManagementClient) {
    let accounting = client.get_treasury_accounting();
    let balance = client.get_treasury_balance();

    assert_eq!(
        accounting.available,
        accounting.total_received - accounting.total_allocated
    );
    assert_eq!(balance.available, accounting.available);
    assert_eq!(balance.sources.len(), accounting.sources.len());

    let (mut received, mut allocated, mut available) = (0i128, 0i128, 0i128);
    for (i, source) in accounting.sources.iter().enumerate() {
        assert_eq!(source.available, source.received - source.allocated);
        assert!(source.available >= 0);
        assert_eq!(balance.sources.get(i as u32).unwrap(), source.source);
        assert_eq!(
            balance.available_by_source.get(i as u32).unwrap(),
            source.available
        );
        assert_eq!(client.get_source_accounting(&source.source), source);
        received += source.received;
        allocated += source.allocated;
        available += source.available;
    }
    assert_eq!(received, accounting.total_received);
    assert_eq!(allocated, accounting.total_allocated);
    assert_eq!(available, accounting.available);

    // History reconciles with allocated totals, per record and per source.
    let history = client.get_allocation_history(&0, &client.get_allocation_count());
    let mut history_total = 0i128;
    let mut debited_by_source: std::vec::Vec<(Address, i128)> = std::vec::Vec::new();
    for record in history.iter() {
        let mut record_total = 0i128;
        for debit in record.debits.iter() {
            record_total += debit.amount;
            match debited_by_source
                .iter_mut()
                .find(|(s, _)| *s == debit.source)
            {
                Some(entry) => entry.1 += debit.amount,
                None => debited_by_source.push((debit.source.clone(), debit.amount)),
            }
        }
        assert_eq!(record_total, record.amount);
        history_total += record.amount;
    }
    assert_eq!(history_total, accounting.total_allocated);
    for source in accounting.sources.iter() {
        let debited = debited_by_source
            .iter()
            .find(|(s, _)| *s == source.source)
            .map_or(0, |(_, amount)| *amount);
        assert_eq!(debited, source.allocated);
    }
}

#[test]
fn receive_100_allocate_40_reports_60_available_and_100_received() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let source = Address::generate(&env);
    let destination = Address::generate(&env);

    client.receive_fee(&source, &100);
    client.allocate(&admin, &destination, &40, &purpose(&env));

    let accounting = client.get_treasury_accounting();
    assert_eq!(accounting.total_received, 100);
    assert_eq!(accounting.total_allocated, 40);
    assert_eq!(accounting.available, 60);

    let src = client.get_source_accounting(&source);
    assert_eq!((src.received, src.allocated, src.available), (100, 40, 60));

    let balance = client.get_treasury_balance();
    assert_eq!(balance.available, 60);
    assert_eq!(balance.available_by_source.get(0).unwrap(), 60);

    // Lifetime receipts used for caps are unaffected by allocation.
    assert_eq!(client.get_source_totals(&source, &0).1, 100);
    assert_reconciled(&client);
}

#[test]
fn unknown_source_reports_zeros() {
    let env = Env::default();
    let (_admin, client) = setup(&env);
    let unknown = Address::generate(&env);
    let acc = client.get_source_accounting(&unknown);
    assert_eq!((acc.received, acc.allocated, acc.available), (0, 0, 0));
    assert_eq!(client.get_treasury_accounting().sources.len(), 0);
    assert_reconciled(&client);
}

#[test]
fn repeated_inflows_accumulate_receipts_and_availability() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let destination = Address::generate(&env);

    client.receive_fee(&a, &100);
    client.receive_fee(&b, &50);
    client.receive_fee(&a, &25);
    assert_reconciled(&client);

    client.allocate(&admin, &destination, &60, &purpose(&env));
    client.receive_fee(&b, &10);
    client.receive_fee(&a, &5);
    assert_reconciled(&client);

    let accounting = client.get_treasury_accounting();
    assert_eq!(accounting.total_received, 190);
    assert_eq!(accounting.total_allocated, 60);
    assert_eq!(accounting.available, 130);
    let a_acc = source_of(&accounting, &a);
    let b_acc = source_of(&accounting, &b);
    assert_eq!(
        (a_acc.received, a_acc.allocated, a_acc.available),
        (130, 60, 70)
    );
    assert_eq!(
        (b_acc.received, b_acc.allocated, b_acc.available),
        (60, 0, 60)
    );
}

#[test]
fn multi_source_allocation_debits_in_first_receipt_order() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);
    let destination = Address::generate(&env);

    client.receive_fee(&a, &100);
    client.receive_fee(&b, &50);
    client.receive_fee(&c, &30);

    // Drains `a` fully, then takes the rest from `b`; `c` is untouched.
    let index = client.allocate(&admin, &destination, &120, &purpose(&env));
    assert_eq!(
        debits_of(&client, index),
        [(a.clone(), 100), (b.clone(), 20)]
    );

    // A source that refills keeps its original position in the order.
    client.receive_fee(&a, &10);
    let index = client.allocate(&admin, &destination, &25, &purpose(&env));
    assert_eq!(
        debits_of(&client, index),
        [(a.clone(), 10), (b.clone(), 15)]
    );

    let accounting = client.get_treasury_accounting();
    assert_eq!(source_of(&accounting, &a).available, 0);
    assert_eq!(source_of(&accounting, &b).available, 15);
    assert_eq!(source_of(&accounting, &c).available, 30);
    assert_eq!(accounting.available, 45);
    assert_reconciled(&client);
}

#[test]
fn complete_allocation_empties_availability_but_keeps_receipts() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let destination = Address::generate(&env);

    client.receive_fee(&a, &70);
    client.receive_fee(&b, &30);
    let index = client.allocate(&admin, &destination, &100, &purpose(&env));
    assert_eq!(
        debits_of(&client, index),
        [(a.clone(), 70), (b.clone(), 30)]
    );

    let accounting = client.get_treasury_accounting();
    assert_eq!(accounting.available, 0);
    assert_eq!(accounting.total_received, 100);
    assert_eq!(accounting.total_allocated, 100);
    for source in accounting.sources.iter() {
        assert_eq!(source.available, 0);
        assert_eq!(source.allocated, source.received);
    }
    assert_eq!(
        client.try_allocate(&admin, &destination, &1, &purpose(&env)),
        Err(Ok(Error::InsufficientBalance))
    );
    assert_reconciled(&client);
}

#[test]
fn rejected_allocations_change_neither_balances_nor_history() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let destination = Address::generate(&env);

    client.receive_fee(&a, &100);
    client.receive_fee(&b, &50);
    client.allocate(&admin, &destination, &40, &purpose(&env));

    let accounting = client.get_treasury_accounting();
    let balance = client.get_treasury_balance();
    let count = client.get_allocation_count();
    let history = client.get_allocation_history(&0, &count);
    let assert_unchanged = |client: &TreasuryManagementClient| {
        assert_eq!(client.get_treasury_accounting(), accounting);
        assert_eq!(client.get_treasury_balance(), balance);
        assert_eq!(client.get_allocation_count(), count);
        assert_eq!(client.get_allocation_history(&0, &(count + 1)), history);
    };

    // Overspend by one unit.
    assert_eq!(
        client.try_allocate(&admin, &destination, &111, &purpose(&env)),
        Err(Ok(Error::InsufficientBalance))
    );
    assert_unchanged(&client);

    // Other rejection paths.
    let outsider = Address::generate(&env);
    assert_eq!(
        client.try_allocate(&outsider, &destination, &10, &purpose(&env)),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(
        client.try_allocate(&admin, &destination, &0, &purpose(&env)),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(
        client.try_allocate(&admin, &destination, &10, &String::from_str(&env, "")),
        Err(Ok(Error::InvalidPurpose))
    );
    client.set_allocation_cooldown(&admin, &100);
    assert_eq!(
        client.try_allocate(&admin, &destination, &10, &purpose(&env)),
        Err(Ok(Error::AllocationCooldownActive))
    );
    assert_unchanged(&client);

    // After the cooldown the exact remaining availability can be allocated.
    env.ledger().with_mut(|l| l.sequence_number += 100);
    client.allocate(&admin, &destination, &110, &purpose(&env));
    assert_eq!(client.get_treasury_accounting().available, 0);
    assert_reconciled(&client);
}

#[test]
fn rejected_receipts_change_neither_balances_nor_history() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let a = Address::generate(&env);
    client.set_approved_source(&admin, &a, &true, &Some(100));
    client.receive_fee(&a, &80);

    let accounting = client.get_treasury_accounting();
    assert_eq!(
        client.try_receive_fee(&a, &21),
        Err(Ok(Error::SourceCapExceeded))
    );
    assert_eq!(
        client.try_receive_fee(&a, &0),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(client.get_treasury_accounting(), accounting);
    assert_reconciled(&client);
}

#[derive(Clone, Debug)]
enum Op {
    Receive { source: usize, amount: i128 },
    Allocate { amount: i128 },
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0..3usize, 1..=1_000i128).prop_map(|(source, amount)| Op::Receive { source, amount }),
        (1..=1_500i128).prop_map(|amount| Op::Allocate { amount }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// Any mix of inflows, partial and complete allocations, and overspends
    /// keeps every figure reconciled against a simple model.
    #[test]
    fn random_flows_stay_reconciled(ops in proptest::collection::vec(op(), 1..25)) {
        // Random cases would churn snapshot files on every run.
        let env = Env::new_with_config(EnvTestConfig {
            capture_snapshot_at_drop: false,
            ..Default::default()
        });
        let (admin, client) = setup(&env);
        let sources = [
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        ];
        let destination = Address::generate(&env);
        let (mut received, mut allocated) = (0i128, 0i128);

        for op in ops {
            match op {
                Op::Receive { source, amount } => {
                    client.receive_fee(&sources[source], &amount);
                    received += amount;
                }
                Op::Allocate { amount } => {
                    let result =
                        client.try_allocate(&admin, &destination, &amount, &purpose(&env));
                    if amount <= received - allocated {
                        prop_assert!(result.is_ok());
                        allocated += amount;
                    } else {
                        prop_assert_eq!(result, Err(Ok(Error::InsufficientBalance)));
                    }
                }
            }
            let accounting = client.get_treasury_accounting();
            prop_assert_eq!(accounting.total_received, received);
            prop_assert_eq!(accounting.total_allocated, allocated);
            assert_reconciled(&client);
        }
    }
}
