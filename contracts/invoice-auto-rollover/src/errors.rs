use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidAmount = 4,
    InvalidAddress = 5,
    NoActiveConfig = 6,
    CandidateAlreadyExists = 7,
    CandidateNotFound = 8,
    CandidateClosed = 9,
    Overflow = 10,
}
