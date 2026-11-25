//! Data types for the upgrade registry contract.

use soroban_sdk::{contracttype, Address, String};

/// Storage key enum for instance and persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Instance: registry admin.
    Admin,
    /// Persistent: current deployed contract info keyed by contract name.
    Contract(String),
    /// Persistent: full version history log keyed by contract name.
    History(String),
    /// Instance: list of all registered contract names.
    ContractNames,
}

/// Current deployment metadata for a registered platform contract.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractRecord {
    /// Logical contract name (e.g. `"invoice-escrow"`).
    pub name: String,
    /// Current deployed contract address.
    pub address: Address,
    /// Current contract version number.
    pub version: u32,
    /// Ledger timestamp when this version was recorded.
    pub updated_at: u64,
}

/// Single historical deployment entry for a contract.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpgradeRecord {
    /// Deployed contract address for this version.
    pub address: Address,
    /// Contract version number.
    pub version: u32,
    /// Ledger timestamp when this deployment was recorded.
    pub timestamp: u64,
}
