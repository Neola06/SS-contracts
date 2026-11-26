//! Platform upgrade registry contract for StellarSettle.
//!
//! Tracks all deployed contract versions across the platform, records a full
//! timestamped upgrade history for each contract name, and validates whether a
//! registered contract satisfies a dependent caller's minimum version
//! requirement.

#![no_std]

mod errors;
mod events;
mod storage;
mod types;

pub use errors::Error;
pub use types::{ContractRecord, UpgradeRecord};

use soroban_sdk::{contract, contractimpl, Address, Env, String, Vec};

/// Reject the zero address (all-zero 32-byte Ed25519 key).
fn ensure_non_zero_address(env: &Env, address: &Address) -> Result<(), Error> {
    let zero_str = String::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    );
    let zero = Address::from_string(&zero_str);
    if *address == zero {
        return Err(Error::InvalidAddress);
    }
    Ok(())
}

#[contract]
pub struct UpgradeRegistry;

#[contractimpl]
impl UpgradeRegistry {
    /// Initialize the upgrade registry with the platform admin address.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        ensure_non_zero_address(&env, &admin)?;
        if storage::get_admin(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        admin.require_auth();
        storage::set_admin(&env, &admin);
        events::initialized(&env, &admin);
        Ok(())
    }

    /// Admin-only: register a new contract deployment or upgrade an existing
    /// registered contract to a strictly higher version.
    ///
    /// Emits `ContractRegistered` on initial registration and `ContractUpgraded`
    /// on subsequent version upgrades.
    pub fn register_contract(
        env: Env,
        name: String,
        address: Address,
        version: u32,
    ) -> Result<(), Error> {
        let admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        admin.require_auth();

        if name.is_empty() {
            return Err(Error::InvalidName);
        }
        ensure_non_zero_address(&env, &address)?;

        let now = env.ledger().timestamp();
        let existing = storage::get_contract(&env, &name);

        if let Some(ref current) = existing {
            if version <= current.version {
                return Err(Error::VersionMustIncrease);
            }
        }

        let record = ContractRecord {
            name: name.clone(),
            address: address.clone(),
            version,
            updated_at: now,
        };
        storage::set_contract(&env, &name, &record);

        let mut history = storage::get_history(&env, &name);
        history.push_back(UpgradeRecord {
            address: address.clone(),
            version,
            timestamp: now,
        });
        storage::set_history(&env, &name, &history);

        match existing {
            None => {
                storage::add_contract_name(&env, &name);
                events::contract_registered(&env, &name, &address, version);
            }
            Some(previous) => {
                events::contract_upgraded(
                    &env,
                    &name,
                    &previous.address,
                    &address,
                    previous.version,
                    version,
                );
            }
        }

        Ok(())
    }

    /// View: return the current deployment record (address, version, updated_at)
    /// for `name`.
    pub fn get_contract(env: Env, name: String) -> Result<ContractRecord, Error> {
        storage::get_contract(&env, &name).ok_or(Error::ContractNotFound)
    }

    /// View: return the full chronological upgrade history (addresses, versions,
    /// timestamps) for `name`.
    pub fn get_upgrade_history(env: Env, name: String) -> Result<Vec<UpgradeRecord>, Error> {
        if storage::get_contract(&env, &name).is_none() {
            return Err(Error::ContractNotFound);
        }
        Ok(storage::get_history(&env, &name))
    }

    /// View: return `true` if `name` is registered and its current version is
    /// greater than or equal to `min_version`. Returns `false` if `name` is not
    /// registered or its current version is below `min_version`.
    pub fn check_compatibility(env: Env, name: String, min_version: u32) -> bool {
        match storage::get_contract(&env, &name) {
            Some(record) => record.version >= min_version,
            None => false,
        }
    }

    /// View: return all registered contract names in registration order.
    pub fn list_contracts(env: Env) -> Vec<String> {
        storage::get_contract_names(&env)
    }

    /// View: return the current admin address.
    pub fn get_admin(env: Env) -> Result<Address, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)
    }
}

#[cfg(test)]
mod test;
