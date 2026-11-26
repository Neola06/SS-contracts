#![cfg(test)]
#![allow(deprecated)]

use soroban_sdk::testutils::{Address as _, Events, Ledger as _};
use soroban_sdk::{Address, BytesN, Env, Symbol, TryFromVal, TryIntoVal, Val, Vec};

use crate::{AuditorAttestationRegistry, AuditorAttestationRegistryClient, Error, Verdict};

fn hash(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

fn setup<'a>(env: &Env) -> (Address, AuditorAttestationRegistryClient<'a>) {
    env.mock_all_auths();
    let contract_id = env.register(AuditorAttestationRegistry, ());
    let client = AuditorAttestationRegistryClient::new(env, &contract_id);
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
fn test_only_registered_auditors_can_attest_and_emits_attested_event() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000);
    let (admin, client) = setup(&env);

    let auditor = Address::generate(&env);
    let unregistered = Address::generate(&env);
    let inv = hash(&env, 1);
    let doc = hash(&env, 11);

    assert_eq!(
        client.try_attest(&unregistered, &inv, &doc, &Verdict::Verified),
        Err(Ok(Error::AuditorNotFound))
    );

    client.register_auditor(&admin, &auditor, &hash(&env, 99));
    client.attest(&auditor, &inv, &doc, &Verdict::Verified);

    let (topics, data) = last_event(&env);
    let ev_name: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    let ev_inv: BytesN<32> = topics.get(1).unwrap().try_into_val(&env).unwrap();
    let ev_aud: Address = topics.get(2).unwrap().try_into_val(&env).unwrap();
    let ev_data: (BytesN<32>, Verdict) = data.try_into_val(&env).unwrap();
    assert_eq!(ev_name, Symbol::new(&env, "attested"));
    assert_eq!(ev_inv, inv);
    assert_eq!(ev_aud, auditor);
    assert_eq!(ev_data, (doc.clone(), Verdict::Verified));

    let list = client.attestations_of(&inv);
    assert_eq!(list.len(), 1);
    assert_eq!(list.get(0).unwrap().verdict, Verdict::Verified);
    assert!(client.is_verified(&inv, &1));
    assert!(!client.is_verified(&inv, &2));
    assert!(!client.is_verified(&inv, &0));
}

#[test]
fn test_duplicate_attestation_from_same_auditor_reverts() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let auditor = Address::generate(&env);
    let inv = hash(&env, 2);
    let doc = hash(&env, 22);

    client.register_auditor(&admin, &auditor, &hash(&env, 77));
    client.attest(&auditor, &inv, &doc, &Verdict::Inconclusive);

    assert_eq!(
        client.try_attest(&auditor, &inv, &doc, &Verdict::Verified),
        Err(Ok(Error::DuplicateAttestation))
    );
    assert_eq!(client.attestations_of(&inv).len(), 1);
    assert!(!client.is_verified(&inv, &1));
}

#[test]
fn test_is_verified_counts_only_verified_verdicts_from_active_auditors() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let a1 = Address::generate(&env);
    let a2 = Address::generate(&env);
    let a3 = Address::generate(&env);
    let a4 = Address::generate(&env);
    let inv = hash(&env, 3);
    let doc = hash(&env, 33);

    client.register_auditor(&admin, &a1, &hash(&env, 41));
    client.register_auditor(&admin, &a2, &hash(&env, 42));
    client.register_auditor(&admin, &a3, &hash(&env, 43));
    client.register_auditor(&admin, &a4, &hash(&env, 44));

    client.attest(&a1, &inv, &doc, &Verdict::Verified);
    client.attest(&a2, &inv, &doc, &Verdict::Inconclusive);
    client.attest(&a3, &inv, &doc, &Verdict::Rejected);
    client.attest(&a4, &inv, &doc, &Verdict::Verified);

    assert_eq!(client.attestations_of(&inv).len(), 4);
    assert!(client.is_verified(&inv, &2));
    assert!(!client.is_verified(&inv, &3));
}

#[test]
fn test_revoke_auditor_emits_event_blocks_attest_and_removes_from_count() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 2_000);
    let (admin, client) = setup(&env);

    let a1 = Address::generate(&env);
    let a2 = Address::generate(&env);
    let inv = hash(&env, 4);
    let doc = hash(&env, 44);

    client.register_auditor(&admin, &a1, &hash(&env, 51));
    client.register_auditor(&admin, &a2, &hash(&env, 52));
    client.attest(&a1, &inv, &doc, &Verdict::Verified);
    client.attest(&a2, &inv, &doc, &Verdict::Verified);
    assert!(client.is_verified(&inv, &2));

    env.ledger().with_mut(|li| li.timestamp = 2_500);
    client.revoke_auditor(&admin, &a1);

    let (topics, data) = last_event(&env);
    let ev_name: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    let ev_aud: Address = topics.get(1).unwrap().try_into_val(&env).unwrap();
    let ev_ts: u64 = data.try_into_val(&env).unwrap();
    assert_eq!(ev_name, Symbol::new(&env, "auditor_revoked"));
    assert_eq!(ev_aud, a1);
    assert_eq!(ev_ts, 2_500);

    // Revoking a1 drops verified count from 2 to 1.
    assert!(client.is_verified(&inv, &1));
    assert!(!client.is_verified(&inv, &2));

    // Revoked auditor cannot attest on new or existing invoices.
    let inv2 = hash(&env, 5);
    assert_eq!(
        client.try_attest(&a1, &inv2, &hash(&env, 55), &Verdict::Verified),
        Err(Ok(Error::AuditorRevoked))
    );
}

#[test]
fn test_changed_doc_hash_invalidates_prior_attestations() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    let a1 = Address::generate(&env);
    let a2 = Address::generate(&env);
    let inv = hash(&env, 6);
    let doc_v1 = hash(&env, 60);
    let doc_v2 = hash(&env, 61);

    client.register_auditor(&admin, &a1, &hash(&env, 71));
    client.register_auditor(&admin, &a2, &hash(&env, 72));

    client.set_invoice_doc_hash(&admin, &inv, &doc_v1);
    client.attest(&a1, &inv, &doc_v1, &Verdict::Verified);
    client.attest(&a2, &inv, &doc_v1, &Verdict::Verified);
    assert!(client.is_verified(&inv, &2));
    assert_eq!(client.attestations_of(&inv).len(), 2);

    // Attesting against a stale/mismatched doc hash fails.
    assert_eq!(
        client.try_attest(&a1, &inv, &doc_v2, &Verdict::Verified),
        Err(Ok(Error::DocHashMismatch))
    );

    // Admin updates stored doc_hash -> prior attestations invalidated.
    client.set_invoice_doc_hash(&admin, &inv, &doc_v2);
    assert_eq!(client.get_invoice_doc_hash(&inv), Some(doc_v2.clone()));
    assert_eq!(client.attestations_of(&inv).len(), 0);
    assert!(!client.is_verified(&inv, &1));

    // Same auditors can now attest to the updated doc_v2 hash.
    client.attest(&a1, &inv, &doc_v2, &Verdict::Verified);
    assert_eq!(client.attestations_of(&inv).len(), 1);
    assert!(client.is_verified(&inv, &1));
}
