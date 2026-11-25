//! Invoice auto-rollover contract for reinvestment at maturity.
//!
//! Automatically reinvests an investor's settlement proceeds into the next
//! available invoice matching their criteria, eliminating idle capital between investments.
//! If no matching invoice is found, settlement proceeds are returned to the investor's wallet.

#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
pub use types::{InvoiceCandidate, RolloverConfig, RolloverCriteria, StorageKey};

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String, Vec};

fn ensure_non_zero_address(env: &Env, address: &Address) -> Result<(), Error> {
    let zero_str = String::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    );
    let zero = Address::from_string(&zero_str);
    if *address == zero {
        return Err(Error::InvalidAddress);
    }
    Ok(())
}

fn get_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&StorageKey::Admin)
}

fn get_operator(env: &Env) -> Option<Address> {
    env.storage().instance().get(&StorageKey::Operator)
}

#[contract]
pub struct InvoiceAutoRolloverContract;

#[contractimpl]
impl InvoiceAutoRolloverContract {
    /// Initializes contract with admin and operator.
    pub fn initialize(env: Env, admin: Address, operator: Address) -> Result<(), Error> {
        ensure_non_zero_address(&env, &admin)?;
        ensure_non_zero_address(&env, &operator)?;
        if get_admin(&env).is_some() {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage().instance().set(&StorageKey::Operator, &operator);
        let empty_list: Vec<BytesN<32>> = Vec::new(&env);
        env.storage()
            .persistent()
            .set(&StorageKey::CandidateList, &empty_list);
        Ok(())
    }

    /// Admin-only: sets operator.
    pub fn set_operator(env: Env, admin: Address, new_operator: Address) -> Result<(), Error> {
        let stored_admin = get_admin(&env).ok_or(Error::NotInitialized)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();
        ensure_non_zero_address(&env, &new_operator)?;
        env.storage()
            .instance()
            .set(&StorageKey::Operator, &new_operator);
        Ok(())
    }

    /// Investor-only: sets rollover configuration with criteria and max amount.
    pub fn set_rollover_config(
        env: Env,
        wallet: Address,
        criteria: RolloverCriteria,
        max_amount: i128,
    ) -> Result<(), Error> {
        wallet.require_auth();
        if max_amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let config = RolloverConfig {
            wallet: wallet.clone(),
            criteria,
            max_amount,
            active: true,
        };

        env.storage()
            .persistent()
            .set(&StorageKey::Config(wallet), &config);
        Ok(())
    }

    /// Investor-only: deactivates rollover configuration.
    pub fn deactivate_rollover_config(env: Env, wallet: Address) -> Result<(), Error> {
        wallet.require_auth();
        let mut config: RolloverConfig = env
            .storage()
            .persistent()
            .get(&StorageKey::Config(wallet.clone()))
            .ok_or(Error::NoActiveConfig)?;

        config.active = false;
        env.storage()
            .persistent()
            .set(&StorageKey::Config(wallet), &config);
        Ok(())
    }

    /// Returns rollover configuration for a wallet.
    pub fn get_rollover_config(env: Env, wallet: Address) -> Result<RolloverConfig, Error> {
        env.storage()
            .persistent()
            .get(&StorageKey::Config(wallet))
            .ok_or(Error::NoActiveConfig)
    }

    /// Operator or Admin: adds or registers an invoice candidate to the live pool.
    pub fn add_invoice_candidate(
        env: Env,
        caller: Address,
        candidate: InvoiceCandidate,
    ) -> Result<(), Error> {
        let admin = get_admin(&env).ok_or(Error::NotInitialized)?;
        let operator = get_operator(&env).ok_or(Error::NotInitialized)?;
        if caller != admin && caller != operator {
            return Err(Error::Unauthorized);
        }
        caller.require_auth();

        if candidate.capacity <= 0 {
            return Err(Error::InvalidAmount);
        }

        let key = StorageKey::Candidate(candidate.invoice_id.clone());
        if env.storage().persistent().has(&key) {
            return Err(Error::CandidateAlreadyExists);
        }

        env.storage().persistent().set(&key, &candidate);

        let mut list: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&StorageKey::CandidateList)
            .unwrap_or_else(|| Vec::new(&env));
        list.push_back(candidate.invoice_id);
        env.storage()
            .persistent()
            .set(&StorageKey::CandidateList, &list);

        Ok(())
    }

    /// Operator or Admin: updates capacity / status of an invoice candidate.
    pub fn update_candidate_status(
        env: Env,
        caller: Address,
        invoice_id: BytesN<32>,
        capacity: i128,
        is_open: bool,
    ) -> Result<(), Error> {
        let admin = get_admin(&env).ok_or(Error::NotInitialized)?;
        let operator = get_operator(&env).ok_or(Error::NotInitialized)?;
        if caller != admin && caller != operator {
            return Err(Error::Unauthorized);
        }
        caller.require_auth();

        let key = StorageKey::Candidate(invoice_id);
        let mut candidate: InvoiceCandidate = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::CandidateNotFound)?;

        candidate.capacity = capacity;
        candidate.is_open = is_open;
        env.storage().persistent().set(&key, &candidate);
        Ok(())
    }

    /// Operator-callable at settlement: executes rollover of settlement proceeds.
    /// Finds best matching invoice from live pool and invests settlement proceeds.
    /// If no matching invoice is found, returns proceeds to wallet and emits RolloverNoMatch.
    pub fn execute_rollover(
        env: Env,
        operator: Address,
        wallet: Address,
        settled_invoice_id: BytesN<32>,
        settlement_proceeds: i128,
    ) -> Result<bool, Error> {
        let stored_operator = get_operator(&env).ok_or(Error::NotInitialized)?;
        let stored_admin = get_admin(&env).ok_or(Error::NotInitialized)?;
        if operator != stored_operator && operator != stored_admin {
            return Err(Error::Unauthorized);
        }
        operator.require_auth();

        if settlement_proceeds <= 0 {
            return Err(Error::InvalidAmount);
        }

        let config: RolloverConfig = env
            .storage()
            .persistent()
            .get(&StorageKey::Config(wallet.clone()))
            .ok_or(Error::NoActiveConfig)?;

        if !config.active {
            return Err(Error::NoActiveConfig);
        }

        // Amount to rollover is capped by investor's max_amount
        let invest_amount = if settlement_proceeds < config.max_amount {
            settlement_proceeds
        } else {
            config.max_amount
        };

        // Candidate search: find open candidate meeting criteria with sufficient capacity
        let candidates_list: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&StorageKey::CandidateList)
            .unwrap_or_else(|| Vec::new(&env));

        let mut matched_target: Option<BytesN<32>> = None;
        let mut best_yield: u32 = 0;

        for id in candidates_list.iter() {
            let key = StorageKey::Candidate(id.clone());
            if let Some(candidate) = env.storage().persistent().get::<_, InvoiceCandidate>(&key) {
                if candidate.is_open
                    && candidate.capacity >= invest_amount
                    && candidate.yield_bps >= config.criteria.min_yield_bps
                    && candidate.duration_days <= config.criteria.max_duration_days
                    && candidate.risk_tier <= config.criteria.max_risk_tier
                {
                    // Select candidate with highest yield (or first if equal)
                    if matched_target.is_none() || candidate.yield_bps > best_yield {
                        matched_target = Some(id.clone());
                        best_yield = candidate.yield_bps;
                    }
                }
            }
        }

        match matched_target {
            Some(target_id) => {
                // Deduct capacity from target candidate
                let key = StorageKey::Candidate(target_id.clone());
                let mut candidate: InvoiceCandidate =
                    env.storage().persistent().get(&key).unwrap();
                candidate.capacity = candidate
                    .capacity
                    .checked_sub(invest_amount)
                    .ok_or(Error::Overflow)?;
                if candidate.capacity == 0 {
                    candidate.is_open = false;
                }
                env.storage().persistent().set(&key, &candidate);

                // Record investment for wallet in target invoice
                let inv_key = StorageKey::InvoiceInvestment(target_id.clone(), wallet.clone());
                let cur_inv: i128 = env.storage().persistent().get(&inv_key).unwrap_or(0);
                env.storage()
                    .persistent()
                    .set(&inv_key, &(cur_inv + invest_amount));

                // If settlement_proceeds exceeded max_amount, leftover is refunded to wallet
                let excess = settlement_proceeds - invest_amount;
                if excess > 0 {
                    let bal_key = StorageKey::WalletBalance(wallet.clone());
                    let cur_bal: i128 = env.storage().persistent().get(&bal_key).unwrap_or(0);
                    env.storage()
                        .persistent()
                        .set(&bal_key, &(cur_bal + excess));
                }

                events::rollover_executed(
                    &env,
                    &wallet,
                    &settled_invoice_id,
                    &target_id,
                    invest_amount,
                );
                Ok(true)
            }
            None => {
                // No match: return full proceeds to wallet
                let bal_key = StorageKey::WalletBalance(wallet.clone());
                let cur_bal: i128 = env.storage().persistent().get(&bal_key).unwrap_or(0);
                env.storage()
                    .persistent()
                    .set(&bal_key, &(cur_bal + settlement_proceeds));

                events::rollover_no_match(
                    &env,
                    &wallet,
                    &settled_invoice_id,
                    settlement_proceeds,
                );
                Ok(false)
            }
        }
    }

    /// Returns recorded wallet balance (e.g. from returned proceeds or excess).
    pub fn get_wallet_balance(env: Env, wallet: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&StorageKey::WalletBalance(wallet))
            .unwrap_or(0)
    }

    /// Returns recorded investment amount of a wallet into a target invoice.
    pub fn get_invoice_investment(
        env: Env,
        invoice_id: BytesN<32>,
        wallet: Address,
    ) -> i128 {
        env.storage()
            .persistent()
            .get(&StorageKey::InvoiceInvestment(invoice_id, wallet))
            .unwrap_or(0)
    }

    /// Returns invoice candidate details.
    pub fn get_candidate(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<InvoiceCandidate, Error> {
        env.storage()
            .persistent()
            .get(&StorageKey::Candidate(invoice_id))
            .ok_or(Error::CandidateNotFound)
    }
}
