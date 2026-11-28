//! Data types for the platform treasury management contract.

use soroban_sdk::{contracttype, Address, String, Vec};

/// Storage key enum for instance and persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Instance: contract admin.
    Admin,
    /// Instance: lifetime fees received across all sources. Never decreases.
    TotalReceived,
    /// Instance: lifetime funds allocated out of the treasury. Never decreases.
    /// Available funds are always `TotalReceived - TotalAllocated`.
    TotalAllocated,
    /// Instance: ordered, deduplicated list of every source address that
    /// has ever called `receive_fee`, in first-receipt order. Storage has no
    /// key enumeration, so the set of known sources is tracked explicitly;
    /// the order also defines the allocation attribution policy.
    KnownSources,
    /// Persistent: lifetime fees received from one source (also the value
    /// checked against the source's cap). Never decreases.
    SourceReceived(Address),
    /// Persistent: lifetime allocations attributed to one source.
    SourceAllocated(Address),
    /// Instance: number of allocation records ever written, used as
    /// both the next record's index and the paginated log's length.
    AllocationCount,
    /// Instance: minimum ledger interval between treasury allocations.
    AllocationCooldown,
    /// Instance: ledger of the last treasury allocation.
    LastAllocationLedger,
    /// Persistent: one allocation record by its index.
    Allocation(u32),
    /// Instance: whether only approved sources can call `receive_fee`.
    SourceAllowlistEnforced,
    /// Persistent: whether a source address is approved.
    ApprovedSource(Address),
    /// Persistent: optional cumulative cap for a source address.
    SourceCap(Address),
    /// Persistent: fees received from a source on a specific ledger day.
    SourceDailyTotal(Address, u64),
}

/// Portion of an allocation debited from one source's available balance.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceDebit {
    pub source: Address,
    pub amount: i128,
}

/// One historical allocation of treasury funds to a destination.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllocationRecord {
    pub destination: Address,
    pub amount: i128,
    pub purpose: String,
    pub timestamp: u64,
    /// How `amount` was attributed to sources; amounts sum to `amount`.
    pub debits: Vec<SourceDebit>,
}

/// Result of `get_treasury_balance`: funds currently available to allocate
/// and their attribution per source, as parallel vectors (an `Address`-keyed
/// `Map` is not returnable the same way across every client binding).
///
/// `available_by_source` always sums exactly to `available`. Lifetime
/// receipts are reported separately by `get_treasury_accounting`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreasuryBalance {
    pub available: i128,
    pub sources: Vec<Address>,
    pub available_by_source: Vec<i128>,
}

/// Lifetime flows and current availability for one fee source.
///
/// Invariant: `available == received - allocated`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceAccounting {
    pub source: Address,
    pub received: i128,
    pub allocated: i128,
    pub available: i128,
}

/// Reconciled treasury accounting.
///
/// Invariants: `available == total_received - total_allocated`, and each of
/// `total_received`, `total_allocated` and `available` equals the sum of the
/// matching field across `sources`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreasuryAccounting {
    pub total_received: i128,
    pub total_allocated: i128,
    pub available: i128,
    pub sources: Vec<SourceAccounting>,
}
