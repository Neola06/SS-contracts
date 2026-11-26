#![allow(deprecated)]
//! Events emitted by the upgrade registry contract.

use soroban_sdk::{Address, Env, String, Symbol};

/// Emitted when the upgrade registry is initialized.
pub fn initialized(env: &Env, admin: &Address) {
    let topics = (Symbol::new(env, "initialized"),);
    env.events().publish(topics, admin.clone());
}

/// Emitted when a contract is registered for the first time.
/// Topics: `(ContractRegistered, name)`; Data: `(address, version)`.
pub fn contract_registered(env: &Env, name: &String, address: &Address, version: u32) {
    let topics = (Symbol::new(env, "ContractRegistered"), name.clone());
    env.events().publish(topics, (address.clone(), version));
}

/// Emitted when an existing registered contract is upgraded to a newer version.
/// Topics: `(ContractUpgraded, name)`; Data: `(old_address, new_address, old_version, new_version)`.
pub fn contract_upgraded(
    env: &Env,
    name: &String,
    old_address: &Address,
    new_address: &Address,
    old_version: u32,
    new_version: u32,
) {
    let topics = (Symbol::new(env, "ContractUpgraded"), name.clone());
    env.events().publish(
        topics,
        (
            old_address.clone(),
            new_address.clone(),
            old_version,
            new_version,
        ),
    );
}
