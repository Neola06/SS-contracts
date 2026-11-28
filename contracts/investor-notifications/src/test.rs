#![cfg(test)]
#![allow(deprecated)]

use soroban_sdk::testutils::{Address as _, Events, Ledger as _};
use soroban_sdk::{
    symbol_short, Address, BytesN, Env, String, Symbol, TryFromVal, TryIntoVal, Val, Vec,
};

use crate::{
    DeliveryState, Error, InvestorNotifications, InvestorNotificationsClient, RetryPolicy,
};

fn setup<'a>(env: &Env) -> (Address, Address, Address, InvestorNotificationsClient<'a>) {
    env.mock_all_auths();
    let contract_id = env.register(InvestorNotifications, ());
    let client = InvestorNotificationsClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let emitter = Address::generate(env);
    let provider = Address::generate(env);

    client.initialize(&admin);
    client.set_authorized_caller(&emitter, &true);
    client.set_delivery_provider(&provider, &true);
    (admin, emitter, provider, client)
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
fn only_authorized_callers_emit() {
    let env = Env::default();
    env.mock_all_auths();
    let contract = env.register(InvestorNotifications, ());
    let client = InvestorNotificationsClient::new(&env, &contract);
    let admin = Address::generate(&env);
    let caller = Address::generate(&env);
    let wallet = Address::generate(&env);
    client.initialize(&admin);
    assert!(client
        .try_emit_kyc_notification(&caller, &wallet, &symbol_short!("approved"))
        .is_err());
    client.set_authorized_caller(&caller, &true);
    client.emit_kyc_notification(&caller, &wallet, &symbol_short!("approved"));
}

#[test]
fn test_delivery_success_records_receipt_and_emits_event() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.sequence_number = 100);
    let (_admin, emitter, provider, client) = setup(&env);
    let wallet = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[7u8; 32]);

    let id = client.queue_notification(
        &emitter,
        &wallet,
        &Symbol::new(&env, "settlement"),
        &Some(invoice_id),
        &Some(500),
    );

    let initial_status = client.get_notification_status(&id);
    assert_eq!(initial_status.state, DeliveryState::Pending);
    assert_eq!(initial_status.retry_count, 0);
    assert_eq!(client.get_next_retry(&id), Some(100));

    env.ledger().with_mut(|li| li.sequence_number = 102);
    let ext_ref = String::from_str(&env, "provider-msg-001");
    let receipt_hash = BytesN::from_array(&env, &[9u8; 32]);
    client.confirm_delivery(&provider, &id, &ext_ref, &receipt_hash);

    let (topics, data) = last_event(&env);
    let ev_name: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    let ev_id: u64 = topics.get(1).unwrap().try_into_val(&env).unwrap();
    let ev_provider: Address = topics.get(2).unwrap().try_into_val(&env).unwrap();
    let ev_data: (String, BytesN<32>, u32) = data.try_into_val(&env).unwrap();
    assert_eq!(ev_name, Symbol::new(&env, "notification_delivered"));
    assert_eq!(ev_id, id);
    assert_eq!(ev_provider, provider);
    assert_eq!(ev_data, (ext_ref.clone(), receipt_hash.clone(), 102));

    let status = client.get_notification_status(&id);
    assert_eq!(status.state, DeliveryState::Delivered);
    assert_eq!(status.next_retry_ledger, None);
    assert_eq!(status.external_ref, Some(ext_ref));
    assert_eq!(status.receipt_hash, Some(receipt_hash));
    assert_eq!(client.get_next_retry(&id), None);
}

#[test]
fn test_delivery_receipt_cannot_be_forged() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.sequence_number = 50);
    let (_admin, emitter, provider, client) = setup(&env);
    let wallet = Address::generate(&env);
    let attacker = Address::generate(&env);

    let id1 = client.queue_notification(&emitter, &wallet, &Symbol::new(&env, "kyc"), &None, &None);
    let id2 = client.queue_notification(&emitter, &wallet, &Symbol::new(&env, "kyc"), &None, &None);

    let valid_ref = String::from_str(&env, "msg-valid-1");
    let valid_hash = BytesN::from_array(&env, &[3u8; 32]);

    // 1. Unauthorized caller and emitter-only caller cannot confirm delivery.
    assert_eq!(
        client.try_confirm_delivery(&attacker, &id1, &valid_ref, &valid_hash),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(
        client.try_confirm_delivery(&emitter, &id1, &valid_ref, &valid_hash),
        Err(Ok(Error::Unauthorized))
    );

    // 2. Empty external reference or zero receipt hash rejected.
    assert_eq!(
        client.try_confirm_delivery(&provider, &id1, &String::from_str(&env, ""), &valid_hash),
        Err(Ok(Error::InvalidReceipt))
    );
    assert_eq!(
        client.try_confirm_delivery(
            &provider,
            &id1,
            &valid_ref,
            &BytesN::from_array(&env, &[0u8; 32])
        ),
        Err(Ok(Error::InvalidReceipt))
    );

    // 3. Valid confirmation succeeds once; duplicate or replayed receipt rejected.
    client.confirm_delivery(&provider, &id1, &valid_ref, &valid_hash);
    assert_eq!(
        client.try_confirm_delivery(
            &provider,
            &id1,
            &String::from_str(&env, "msg-other"),
            &valid_hash
        ),
        Err(Ok(Error::AlreadyDelivered))
    );
    assert_eq!(
        client.try_confirm_delivery(&provider, &id2, &valid_ref, &valid_hash),
        Err(Ok(Error::DuplicateReceipt))
    );
}

#[test]
fn test_failure_retry_backoff_and_eventual_success() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.sequence_number = 100);
    let (_admin, emitter, provider, client) = setup(&env);
    client.configure_retry_policy(&3, &10, &500);

    let wallet = Address::generate(&env);
    let id = client.queue_notification(
        &emitter,
        &wallet,
        &Symbol::new(&env, "auto_invest"),
        &None,
        &Some(1_000),
    );

    // First failure at ledger 100 schedules next retry at 110.
    let state = client.record_delivery_failure(&provider, &id, &symbol_short!("timeout"));
    assert_eq!(state, DeliveryState::Failed);
    assert_eq!(client.get_next_retry(&id), Some(110));

    // Retrying before ledger 110 is rejected.
    env.ledger().with_mut(|li| li.sequence_number = 109);
    assert_eq!(
        client.try_record_delivery_failure(&provider, &id, &symbol_short!("timeout")),
        Err(Ok(Error::RetryNotReady))
    );

    // At ledger 110, delivery succeeds on retry.
    env.ledger().with_mut(|li| li.sequence_number = 110);
    client.confirm_delivery(
        &provider,
        &id,
        &String::from_str(&env, "retry-ok-1"),
        &BytesN::from_array(&env, &[5u8; 32]),
    );
    assert_eq!(
        client.get_notification_status(&id).state,
        DeliveryState::Delivered
    );
}

#[test]
fn test_bounded_retry_exhaustion_blocks_further_retries_and_emits_event() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.sequence_number = 10);
    let (_admin, emitter, provider, client) = setup(&env);
    client.configure_retry_policy(&2, &5, &200);
    assert_eq!(
        client.get_retry_policy(),
        RetryPolicy {
            max_retries: 2,
            retry_delay_ledgers: 5,
            expiry_ledgers: 200,
        }
    );

    let wallet = Address::generate(&env);
    let id = client.queue_notification(
        &emitter,
        &wallet,
        &Symbol::new(&env, "settlement"),
        &None,
        &Some(250),
    );

    // Attempt 1 fails -> retry_count = 1, next_retry = 15.
    client.record_delivery_failure(&provider, &id, &symbol_short!("http_503"));
    assert_eq!(client.get_notification_status(&id).retry_count, 1);
    assert_eq!(client.get_next_retry(&id), Some(15));

    // Attempt 2 fails -> retry_count = 2 == max_retries -> exhausted!
    env.ledger().with_mut(|li| li.sequence_number = 15);
    client.record_delivery_failure(&provider, &id, &symbol_short!("http_500"));

    let (topics, data) = last_event(&env);
    let ev_name: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    let ev_id: u64 = topics.get(1).unwrap().try_into_val(&env).unwrap();
    let ev_data: (u32, Symbol) = data.try_into_val(&env).unwrap();
    assert_eq!(ev_name, Symbol::new(&env, "notification_retry_exhausted"));
    assert_eq!(ev_id, id);
    assert_eq!(ev_data, (2, symbol_short!("http_500")));

    let status = client.get_notification_status(&id);
    assert_eq!(status.state, DeliveryState::Failed);
    assert_eq!(status.retry_count, 2);
    assert_eq!(status.next_retry_ledger, None);

    // No retries or delivery confirmation permitted past the configured bound.
    env.ledger().with_mut(|li| li.sequence_number = 25);
    assert_eq!(
        client.try_record_delivery_failure(&provider, &id, &symbol_short!("again")),
        Err(Ok(Error::RetryLimitExhausted))
    );
    assert_eq!(
        client.try_confirm_delivery(
            &provider,
            &id,
            &String::from_str(&env, "late-receipt"),
            &BytesN::from_array(&env, &[8u8; 32])
        ),
        Err(Ok(Error::RetryLimitExhausted))
    );
}

#[test]
fn test_expired_state_is_terminal() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.sequence_number = 1_000);
    let (_admin, emitter, provider, client) = setup(&env);
    client.configure_retry_policy(&3, &10, &50);

    let wallet = Address::generate(&env);
    let id = client.queue_notification(&emitter, &wallet, &Symbol::new(&env, "kyc"), &None, &None);

    // Cannot expire before expires_at_ledger (1_050).
    env.ledger().with_mut(|li| li.sequence_number = 1_049);
    assert_eq!(
        client.try_expire_notification(&id),
        Err(Ok(Error::NotYetExpired))
    );

    // At ledger 1_050, expire_notification transitions to terminal Expired state.
    env.ledger().with_mut(|li| li.sequence_number = 1_050);
    client.expire_notification(&id);

    let status = client.get_notification_status(&id);
    assert_eq!(status.state, DeliveryState::Expired);
    assert_eq!(status.next_retry_ledger, None);

    // Expired state is terminal: confirm, failure, and re-expire all reject.
    assert_eq!(
        client.try_confirm_delivery(
            &provider,
            &id,
            &String::from_str(&env, "too-late"),
            &BytesN::from_array(&env, &[4u8; 32])
        ),
        Err(Ok(Error::NotificationExpired))
    );
    assert_eq!(
        client.try_record_delivery_failure(&provider, &id, &symbol_short!("timeout")),
        Err(Ok(Error::NotificationExpired))
    );
    assert_eq!(
        client.try_expire_notification(&id),
        Err(Ok(Error::NotificationExpired))
    );
}
