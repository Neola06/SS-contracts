//! Data types for the investor notifications contract.

use soroban_sdk::{contracttype, Address, BytesN, String, Symbol};

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    Authorized(Address),
    DeliveryProvider(Address),
    RetryPolicy,
    NextNotificationId,
    Notification(u64),
    UsedReceipt(String),
}

/// Lifecycle state of an investor notification delivery.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum DeliveryState {
    Pending = 0,
    Delivered = 1,
    Failed = 2,
    Expired = 3,
}

/// Global bounded retry and expiry policy configured by the contract admin.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetryPolicy {
    /// Maximum number of failed delivery attempts before exhaustion.
    pub max_retries: u32,
    /// Minimum number of ledgers between consecutive retry attempts.
    pub retry_delay_ledgers: u32,
    /// Ledger lifetime after creation before a notification becomes expired.
    pub expiry_ledgers: u32,
}

/// Full persisted record for a tracked investor notification.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotificationRecord {
    pub id: u64,
    pub wallet: Address,
    pub notification_type: Symbol,
    pub invoice_id: Option<BytesN<32>>,
    pub amount: Option<i128>,
    pub state: DeliveryState,
    pub retry_count: u32,
    pub max_retries: u32,
    pub created_ledger: u32,
    pub next_retry_ledger: Option<u32>,
    pub expires_at_ledger: u32,
    pub external_ref: Option<String>,
    pub receipt_hash: Option<BytesN<32>>,
    pub delivered_by: Option<Address>,
    pub delivered_ledger: Option<u32>,
    pub last_error: Option<Symbol>,
}

/// Public status summary exposing delivery state and next retry schedule.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotificationStatus {
    pub id: u64,
    pub state: DeliveryState,
    pub retry_count: u32,
    pub max_retries: u32,
    pub next_retry_ledger: Option<u32>,
    pub expires_at_ledger: u32,
    pub external_ref: Option<String>,
    pub receipt_hash: Option<BytesN<32>>,
}
