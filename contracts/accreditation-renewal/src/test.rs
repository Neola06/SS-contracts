#![cfg(test)]
extern crate std;

use super::*;
use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::{symbol_short, Symbol, TryFromVal, Val};

const NOW: u64 = 1_700_000_000;
const DAY: u64 = SECONDS_PER_DAY;

struct Setup {
    env: Env,
    client: AccreditationRenewalContractClient<'static>,
    admin: Address,
    operator: Address,
}

fn setup() -> Setup {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = NOW);
    let contract_id = env.register(AccreditationRenewalContract, ());
    let client = AccreditationRenewalContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let operator = Address::generate(&env);
    client.initialize(&admin, &operator);
    Setup {
        env,
        client,
        admin,
        operator,
    }
}

impl Setup {
    /// Register `n` new wallets, each expiring `days` from now.
    fn register(&self, n: u32, days: u64) -> std::vec::Vec<Address> {
        (0..n)
            .map(|_| {
                let wallet = Address::generate(&self.env);
                self.client
                    .set_accreditation_expiry(&self.admin, &wallet, &(NOW + days * DAY + 1));
                wallet
            })
            .collect()
    }

    /// Count events from the most recent invocation whose first topic is `name`.
    fn count_events(&self, name: Symbol) -> u32 {
        let events = self.env.events().all();
        events
            .events()
            .iter()
            .filter(|event| {
                let soroban_sdk::xdr::ContractEventBody::V0(body) = &event.body;
                body.topics.first().is_some_and(|topic| {
                    let val = Val::try_from_val(&self.env, topic).unwrap();
                    Symbol::try_from_val(&self.env, &val).is_ok_and(|s| s == name)
                })
            })
            .count() as u32
    }

    fn reminders_emitted(&self) -> u32 {
        self.count_events(symbol_short!("renew_rem"))
    }
}

#[test]
fn continuation_reaches_every_wallet_across_pages() {
    let s = setup();
    let wallets = s.register(7, 5);
    s.client.set_scan_page_limit(&s.admin, &3);

    let sweep = s.client.start_reminder_sweep();
    assert_eq!(sweep.total, 7);
    assert_eq!(sweep.next_cursor, 0);
    assert!(!sweep.completed);

    let mut cursor = 0;
    let mut pages = std::vec::Vec::new();
    loop {
        let page = s
            .client
            .check_accreditation_reminders(&sweep.sweep_id, &cursor, &3);
        assert!(page.examined <= 3);
        // 5 days out: the 60, 30 and 7 day intervals are all due.
        assert_eq!(page.emitted, page.examined * 3);
        assert_eq!(s.reminders_emitted(), page.emitted);
        pages.push((page.cursor, page.next_cursor, page.complete));
        cursor = page.next_cursor;
        if page.complete {
            break;
        }
    }

    assert_eq!(pages, [(0, 3, false), (3, 6, false), (6, 7, true)]);
    for wallet in wallets.iter() {
        assert_eq!(s.client.get_reminder_log(wallet).len(), 3);
    }
    let state = s.client.get_reminder_sweep();
    assert!(state.completed);
    assert_eq!(state.next_cursor, 7);
}

#[test]
fn page_never_exceeds_configured_limit() {
    let s = setup();
    s.register(30, 90);
    assert_eq!(s.client.get_scan_page_limit(), DEFAULT_SCAN_PAGE_LIMIT);

    let sweep = s.client.start_reminder_sweep();
    let page = s
        .client
        .check_accreditation_reminders(&sweep.sweep_id, &0, &u32::MAX);
    assert_eq!(page.examined, DEFAULT_SCAN_PAGE_LIMIT);

    s.client.set_scan_page_limit(&s.admin, &4);
    let page = s
        .client
        .check_accreditation_reminders(&sweep.sweep_id, &page.next_cursor, &50);
    assert_eq!(page.examined, 4);

    // A caller may ask for less than the configured limit.
    let page = s
        .client
        .check_accreditation_reminders(&sweep.sweep_id, &page.next_cursor, &1);
    assert_eq!(page.examined, 1);
    assert_eq!(page.next_cursor, DEFAULT_SCAN_PAGE_LIMIT + 5);
}

#[test]
fn retrying_a_page_does_not_duplicate_events_or_writes() {
    let s = setup();
    let wallets = s.register(4, 20);
    s.client.set_scan_page_limit(&s.admin, &2);
    let sweep = s.client.start_reminder_sweep();

    let first = s
        .client
        .check_accreditation_reminders(&sweep.sweep_id, &0, &2);
    // 20 days out: 60 and 30 day intervals due.
    assert_eq!(first.emitted, 4);
    let log_before = s.client.get_reminder_log(&wallets[0]);

    s.env.ledger().with_mut(|l| l.timestamp = NOW + 60);
    let retry = s
        .client
        .check_accreditation_reminders(&sweep.sweep_id, &0, &2);
    assert_eq!(retry.examined, 2);
    assert_eq!(retry.emitted, 0);
    assert_eq!(s.reminders_emitted(), 0);
    assert_eq!(s.client.get_reminder_log(&wallets[0]), log_before);

    // Retrying does not move the frontier backwards.
    assert_eq!(s.client.get_reminder_sweep().next_cursor, 2);

    // Retrying the final page after completion is also a no-op.
    let last = s
        .client
        .check_accreditation_reminders(&sweep.sweep_id, &2, &2);
    assert!(last.complete);
    assert_eq!(last.emitted, 4);
    let last_retry = s
        .client
        .check_accreditation_reminders(&sweep.sweep_id, &2, &2);
    assert!(last_retry.complete);
    assert_eq!(last_retry.emitted, 0);
    assert_eq!(s.reminders_emitted(), 0);
    assert_eq!(s.count_events(symbol_short!("swp_done")), 0);
}

#[test]
fn end_of_list_returns_empty_page_and_completes_once() {
    let s = setup();
    s.register(2, 5);
    let sweep = s.client.start_reminder_sweep();

    let page = s
        .client
        .check_accreditation_reminders(&sweep.sweep_id, &0, &10);
    assert_eq!(page.examined, 2);
    assert!(page.complete);
    assert_eq!(s.count_events(symbol_short!("swp_done")), 1);

    let past_end = s
        .client
        .check_accreditation_reminders(&sweep.sweep_id, &2, &10);
    assert_eq!(past_end.examined, 0);
    assert_eq!(past_end.next_cursor, 2);
    assert!(past_end.complete);
    assert_eq!(s.count_events(symbol_short!("swp_done")), 0);
}

#[test]
fn empty_registry_sweep_is_complete_immediately() {
    let s = setup();
    let sweep = s.client.start_reminder_sweep();
    assert_eq!(sweep.total, 0);
    assert!(sweep.completed);
    assert_eq!(s.count_events(symbol_short!("swp_done")), 1);

    let page = s
        .client
        .check_accreditation_reminders(&sweep.sweep_id, &0, &5);
    assert_eq!(page.examined, 0);
    assert!(page.complete);
}

#[test]
fn mid_sweep_registrations_join_the_next_sweep() {
    let s = setup();
    let members = s.register(3, 5);
    s.client.set_scan_page_limit(&s.admin, &2);
    let sweep = s.client.start_reminder_sweep();

    let first = s
        .client
        .check_accreditation_reminders(&sweep.sweep_id, &0, &2);

    let late = s.register(2, 5);
    // Updating an existing member's expiry does not re-register it.
    s.client
        .set_accreditation_expiry(&s.admin, &members[2], &(NOW + 6 * DAY));
    assert_eq!(s.client.get_wallet_count(), 5);

    let second = s
        .client
        .check_accreditation_reminders(&sweep.sweep_id, &first.next_cursor, &2);
    assert_eq!(second.examined, 1);
    assert!(second.complete);
    assert_eq!(second.total, 3);
    for wallet in late.iter() {
        assert_eq!(s.client.get_reminder_log(wallet).len(), 0);
    }
    // The member updated mid-sweep is evaluated against its current expiry.
    assert_eq!(s.client.get_reminder_log(&members[2]).len(), 3);

    let next = s.client.start_reminder_sweep();
    assert_eq!(next.sweep_id, sweep.sweep_id + 1);
    assert_eq!(next.total, 5);
    let mut cursor = 0;
    let mut emitted = 0;
    loop {
        let page = s
            .client
            .check_accreditation_reminders(&next.sweep_id, &cursor, &2);
        emitted += page.emitted;
        cursor = page.next_cursor;
        if page.complete {
            break;
        }
    }
    // Only the two late wallets had pending reminders.
    assert_eq!(emitted, 6);
    for wallet in late.iter() {
        assert_eq!(s.client.get_reminder_log(wallet).len(), 3);
    }
}

#[test]
fn interval_suppression_holds_across_sweeps() {
    let s = setup();
    let wallet = s.register(1, 45)[0].clone();

    let sweep = s.client.start_reminder_sweep();
    let page = s
        .client
        .check_accreditation_reminders(&sweep.sweep_id, &0, &5);
    assert_eq!(page.emitted, 1);

    let sweep = s.client.start_reminder_sweep();
    let page = s
        .client
        .check_accreditation_reminders(&sweep.sweep_id, &0, &5);
    assert_eq!(page.emitted, 0);

    s.env.ledger().with_mut(|l| l.timestamp = NOW + 20 * DAY);
    let sweep = s.client.start_reminder_sweep();
    let page = s
        .client
        .check_accreditation_reminders(&sweep.sweep_id, &0, &5);
    assert_eq!(page.emitted, 1);

    let intervals: std::vec::Vec<u32> = s
        .client
        .get_reminder_log(&wallet)
        .iter()
        .map(|r| r.interval_days)
        .collect();
    assert_eq!(intervals, [60, 30]);
}

#[test]
fn malformed_cursors_and_sweeps_fail_explicitly() {
    let s = setup();
    s.register(5, 5);

    assert_eq!(
        s.client.try_check_accreditation_reminders(&1, &0, &2),
        Err(Ok(Error::NoActiveSweep))
    );
    assert_eq!(
        s.client.try_get_reminder_sweep(),
        Err(Ok(Error::NoActiveSweep))
    );

    let sweep = s.client.start_reminder_sweep();
    // Skipping ahead of the scanned frontier would miss wallets.
    assert_eq!(
        s.client
            .try_check_accreditation_reminders(&sweep.sweep_id, &1, &2),
        Err(Ok(Error::InvalidCursor))
    );
    assert_eq!(
        s.client
            .try_check_accreditation_reminders(&sweep.sweep_id, &u32::MAX, &2),
        Err(Ok(Error::InvalidCursor))
    );
    assert_eq!(
        s.client
            .try_check_accreditation_reminders(&sweep.sweep_id, &0, &0),
        Err(Ok(Error::InvalidPageLimit))
    );
    assert_eq!(
        s.client
            .try_check_accreditation_reminders(&(sweep.sweep_id + 1), &0, &2),
        Err(Ok(Error::StaleSweep))
    );

    // A superseded sweep can no longer be paged.
    s.client.start_reminder_sweep();
    assert_eq!(
        s.client
            .try_check_accreditation_reminders(&sweep.sweep_id, &0, &2),
        Err(Ok(Error::StaleSweep))
    );
}

#[test]
fn page_limit_configuration_is_admin_only_and_bounded() {
    let s = setup();
    assert_eq!(
        s.client.try_set_scan_page_limit(&s.admin, &0),
        Err(Ok(Error::InvalidPageLimit))
    );
    assert_eq!(
        s.client
            .try_set_scan_page_limit(&s.admin, &(MAX_SCAN_PAGE_LIMIT + 1)),
        Err(Ok(Error::InvalidPageLimit))
    );
    assert_eq!(
        s.client.try_set_scan_page_limit(&s.operator, &10),
        Err(Ok(Error::Unauthorized))
    );
    s.client.set_scan_page_limit(&s.admin, &MAX_SCAN_PAGE_LIMIT);
    assert_eq!(s.client.get_scan_page_limit(), MAX_SCAN_PAGE_LIMIT);
}

#[test]
fn sweep_calls_require_operator_auth() {
    let s = setup();
    s.register(2, 5);

    let sweep = s.client.start_reminder_sweep();
    let auths = s.env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, s.operator);

    s.client
        .check_accreditation_reminders(&sweep.sweep_id, &0, &2);
    let auths = s.env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, s.operator);

    // Without the operator's signature both entrypoints are rejected.
    s.env.set_auths(&[]);
    assert!(s.client.try_start_reminder_sweep().is_err());
    assert!(s
        .client
        .try_check_accreditation_reminders(&sweep.sweep_id, &0, &2)
        .is_err());
}

/// Ledger footprint (entries read, entries written, bytes read) of scanning one
/// 5-wallet page with `population` wallets registered.
fn page_footprint(population: u32) -> (u32, u32, u32) {
    let s = setup();
    s.register(population, 5);
    s.client.set_scan_page_limit(&s.admin, &5);
    let sweep = s.client.start_reminder_sweep();

    let page = s
        .client
        .check_accreditation_reminders(&sweep.sweep_id, &0, &5);
    assert_eq!(page.examined, 5);
    let res = s.env.cost_estimate().resources();
    (
        res.disk_read_entries + res.memory_read_entries,
        res.write_entries,
        res.disk_read_bytes,
    )
}

#[test]
fn page_footprint_is_independent_of_population() {
    let small = page_footprint(5);
    let large = page_footprint(200);
    std::println!(
        "[bench] 5-wallet reminder page (reads, writes, read bytes): \
         population 5 {small:?}, population 200 {large:?}"
    );
    // Entries touched depend only on the page size, never on how many
    // wallets are registered.
    assert_eq!(small, large);
    // Per wallet: WalletAt + Expiry + ReminderLog reads and one log write,
    // plus fixed overhead (contract instance, code, operator auth nonce).
    assert!(large.0 <= 5 * 3 + 3, "reads={}", large.0);
    assert!(large.1 <= 5 + 2, "writes={}", large.1);
}
