//! Error types for the investor notifications contract.

use soroban_sdk::contracterror;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracterror]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidAddress = 4,
    InvalidConfig = 5,
    NotificationNotFound = 6,
    InvalidReceipt = 7,
    DuplicateReceipt = 8,
    AlreadyDelivered = 9,
    NotificationExpired = 10,
    RetryLimitExhausted = 11,
    RetryNotReady = 12,
    NotYetExpired = 13,
    ArithmeticOverflow = 14,
}
