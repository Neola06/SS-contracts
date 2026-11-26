//! Error types for the auditor document attestation registry contract.

use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInit = 1,
    NotInit = 2,
    Unauthorized = 3,
    InvalidAddress = 4,
    InvalidHash = 5,
    AuditorAlreadyRegistered = 6,
    AuditorNotFound = 7,
    AuditorRevoked = 8,
    DuplicateAttestation = 9,
    DocHashMismatch = 10,
}
