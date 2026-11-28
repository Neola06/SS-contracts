//! Error types for the invoice greylisting contract.

use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInit = 1,
    NotInit = 2,
    Unauthorized = 3,
    InvalidAddress = 4,
    InvalidDuration = 5,
    InvoiceGreylisted = 6,
    NotGreylisted = 7,
    InvoiceNotFound = 8,
    Overflow = 9,
}
