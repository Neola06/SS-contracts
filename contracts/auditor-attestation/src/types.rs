//! Data types for the auditor document attestation registry contract.

use soroban_sdk::{contracttype, Address, BytesN};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Instance: registry admin.
    Admin,
    /// Persistent: auditor metadata keyed by auditor address.
    Auditor(Address),
    /// Persistent: anchored document hash for an invoice.
    InvoiceDocHash(BytesN<32>),
    /// Persistent: attestation keyed by `(invoice_id, auditor)`.
    Attestation(BytesN<32>, Address),
    /// Persistent: list of auditors who have attested for an invoice's current doc_hash.
    InvoiceAuditors(BytesN<32>),
}

/// Auditor review verdict on an invoice document.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Verdict {
    Verified = 0,
    Inconclusive = 1,
    Rejected = 2,
}

/// Registered auditor record.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditorRecord {
    pub auditor: Address,
    pub name_hash: BytesN<32>,
    pub active: bool,
    pub registered_at: u64,
    pub revoked_at: Option<u64>,
}

/// Single auditor attestation anchored to an invoice document hash.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attestation {
    pub invoice_id: BytesN<32>,
    pub auditor: Address,
    pub doc_hash: BytesN<32>,
    pub verdict: Verdict,
    pub attested_at: u64,
}
