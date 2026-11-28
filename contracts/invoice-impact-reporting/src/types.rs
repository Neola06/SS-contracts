//! Data types for the invoice impact reporting contract.

use soroban_sdk::{contracttype, Address, BytesN};

/// Storage keys for the contract.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Admin address allowed to verify impact reports.
    Admin,
    /// Invoice record storage key.
    Invoice(BytesN<32>),
    /// Impact report storage key for an invoice.
    Report(BytesN<32>),
}

/// ESG Report types.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ReportType {
    Environmental = 0,
    Social = 1,
    Governance = 2,
    Combined = 3,
}

impl ReportType {
    pub fn from_u32(val: u32) -> Option<Self> {
        match val {
            0 => Some(ReportType::Environmental),
            1 => Some(ReportType::Social),
            2 => Some(ReportType::Governance),
            3 => Some(ReportType::Combined),
            _ => None,
        }
    }
}

/// Settlement status of an invoice.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceInfo {
    pub invoice_id: BytesN<32>,
    pub issuer: Address,
    pub is_settled: bool,
}

/// On-chain impact report record.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImpactReport {
    pub invoice_id: BytesN<32>,
    pub issuer: Address,
    pub metrics_hash: BytesN<32>,
    pub report_type: ReportType,
    pub verified: bool,
}
