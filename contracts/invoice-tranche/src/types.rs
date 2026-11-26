use soroban_sdk::{contracttype, Address, BytesN};

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum TrancheType {
    Senior = 1,
    Junior = 2,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrancheConfig {
    pub issuer: Address,
    pub senior_pct: u32,
    pub junior_yield_premium_bps: u32,
    pub senior_filled: i128,
    pub junior_filled: i128,
    pub is_settled: bool,
    pub senior_payout: i128,
    pub junior_payout: i128,
    pub issuer_surplus: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrancheStatus {
    pub senior_pct: u32,
    pub junior_pct: u32,
    pub senior_filled: i128,
    pub junior_filled: i128,
    pub junior_yield_premium_bps: u32,
    pub is_settled: bool,
    pub senior_payout: i128,
    pub junior_payout: i128,
    pub issuer_surplus: i128,
}

/// Complete disposition of `settlement_amount` across senior tranche, junior
/// tranche, and residual surplus retained for the invoice issuer.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettlementResult {
    pub senior_payout: i128,
    pub junior_payout: i128,
    pub issuer_surplus: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    Tranche(BytesN<32>),
    Investment(BytesN<32>, Address, u32),
}
