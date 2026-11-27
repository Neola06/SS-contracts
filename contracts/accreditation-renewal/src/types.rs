use soroban_sdk::{contracttype, Address};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReminderRecord {
    pub interval_days: u32,
    pub timestamp: u64,
}

/// Progress of the current reminder sweep.
///
/// Membership is fixed at start: the sweep covers registration indices `0..total`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SweepState {
    pub sweep_id: u32,
    /// Number of registered wallets when the sweep started (membership snapshot).
    pub total: u32,
    /// Highest cursor reached so far; every index below it has been examined.
    pub next_cursor: u32,
    pub started_at: u64,
    pub completed: bool,
}

/// Result of scanning one page of a reminder sweep.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReminderPage {
    pub sweep_id: u32,
    /// Cursor this page started from.
    pub cursor: u32,
    /// Cursor to pass for the following page.
    pub next_cursor: u32,
    pub total: u32,
    /// Wallets examined by this invocation (never above the configured page limit).
    pub examined: u32,
    /// Reminder events emitted by this invocation.
    pub emitted: u32,
    /// True once every wallet in the sweep's membership has been examined.
    pub complete: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    Operator,
    /// Number of wallets ever registered; also the next registration index.
    WalletCount,
    /// Wallet stored at a registration index (append-only).
    WalletAt(u32),
    /// Registration index of a wallet, used for O(1) duplicate detection.
    WalletIndex(Address),
    Expiry(Address),
    ReminderLog(Address),
    /// Admin-configured maximum number of wallets examined per page.
    ScanPageLimit,
    /// Current `SweepState`.
    Sweep,
}
