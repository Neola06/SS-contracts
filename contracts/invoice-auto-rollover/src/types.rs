use soroban_sdk::{contracttype, Address, BytesN};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RolloverCriteria {
    pub min_yield_bps: u32,
    pub max_duration_days: u32,
    pub max_risk_tier: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RolloverConfig {
    pub wallet: Address,
    pub criteria: RolloverCriteria,
    pub max_amount: i128,
    pub active: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceCandidate {
    pub invoice_id: BytesN<32>,
    pub yield_bps: u32,
    pub duration_days: u32,
    pub risk_tier: u32,
    pub capacity: i128,
    pub is_open: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Admin,
    Operator,
    Config(Address),
    Candidate(BytesN<32>),
    CandidateList,
    SettledProcessed(BytesN<32>),
    WalletBalance(Address),
    InvoiceInvestment(BytesN<32>, Address),
}
