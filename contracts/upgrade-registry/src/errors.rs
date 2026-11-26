//! Error types for the upgrade registry contract.

use soroban_sdk::contracterror;

/// Errors that can occur during upgrade registry execution.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    /// Contract has already been initialized.
    AlreadyInit = 1,
    /// Contract has not been initialized.
    NotInit = 2,
    /// Address is the zero address (all-zero 32-byte key).
    InvalidAddress = 3,
    /// Contract name is empty.
    InvalidName = 4,
    /// New version must be strictly greater than the current version on upgrade.
    VersionMustIncrease = 5,
    /// No contract is registered under the requested name.
    ContractNotFound = 6,
}
