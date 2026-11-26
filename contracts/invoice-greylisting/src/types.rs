//! Data types for invoice greylisting.

use soroban_sdk::{contracttype, Address, BytesN, String};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Admin,
    Greylist(BytesN<32>),
    HolderBalance(BytesN<32>, Address),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GreylistRecord {
    pub invoice_id: BytesN<32>,
    pub reason: String,
    pub greylisted_at: u64,
    pub duration_seconds: u64,
    pub expires_at: u64,
    pub active: bool,
}
