use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidExpiry = 4,
    WalletNotFound = 5,
    /// No reminder sweep has been started yet.
    NoActiveSweep = 6,
    /// The supplied sweep id is not the current sweep (it was superseded or never existed).
    StaleSweep = 7,
    /// The cursor points past the sweep's scanned frontier or its membership snapshot.
    InvalidCursor = 8,
    /// A page limit of zero, or a configured limit outside `1..=MAX_SCAN_PAGE_LIMIT`.
    InvalidPageLimit = 9,
}
