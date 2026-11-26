//! Invoice tranche contract for StellarSettle.
//!
//! Splits invoice funding into senior and junior investor classes and settles
//! proceeds through a strict priority waterfall:
//! 1. Senior principal is repaid first, up to `senior_filled`.
//! 2. Remaining proceeds repay junior principal up to `junior_filled`.
//! 3. Any surplus above senior + junior principal is split between the junior
//!    tranche (`junior_yield_premium_bps` / `10_000` of surplus, truncated down
//!    to whole units) and the invoice issuer (`issuer_surplus`), which receives
//!    all unallocated residual surplus including integer division dust.
//!    When `junior_filled == 0`, all proceeds above `senior_filled` belong to
//!    `issuer_surplus`.
//!
//! Conservation invariant: `senior_payout + junior_payout + issuer_surplus == settlement_amount`.

#![no_std]

mod errors;
mod events;
mod types;

pub use errors::Error;
pub use types::{DataKey, SettlementResult, TrancheConfig, TrancheStatus, TrancheType};

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env};

const BPS_DENOMINATOR: i128 = 10_000;

#[contract]
pub struct InvoiceTrancheContract;

#[contractimpl]
impl InvoiceTrancheContract {
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        Ok(())
    }

    pub fn create_tranches(
        env: Env,
        issuer: Address,
        invoice_id: BytesN<32>,
        senior_pct: u32,
        junior_yield_premium_bps: u32,
    ) -> Result<(), Error> {
        issuer.require_auth();

        if senior_pct == 0 || senior_pct >= 100 {
            return Err(Error::InvalidTrancheConfig);
        }
        if (junior_yield_premium_bps as i128) > BPS_DENOMINATOR {
            return Err(Error::InvalidTrancheConfig);
        }

        let tranche_key = DataKey::Tranche(invoice_id.clone());
        if env.storage().persistent().has(&tranche_key) {
            return Err(Error::TrancheAlreadyExists);
        }

        let config = TrancheConfig {
            issuer,
            senior_pct,
            junior_yield_premium_bps,
            senior_filled: 0,
            junior_filled: 0,
            is_settled: false,
            senior_payout: 0,
            junior_payout: 0,
            issuer_surplus: 0,
        };

        env.storage().persistent().set(&tranche_key, &config);
        Ok(())
    }

    pub fn invest_tranche(
        env: Env,
        investor: Address,
        invoice_id: BytesN<32>,
        tranche_type: u32,
        amount: i128,
    ) -> Result<(), Error> {
        investor.require_auth();

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        if tranche_type != TrancheType::Senior as u32 && tranche_type != TrancheType::Junior as u32
        {
            return Err(Error::InvalidTrancheType);
        }

        let tranche_key = DataKey::Tranche(invoice_id.clone());
        let mut config: TrancheConfig = env
            .storage()
            .persistent()
            .get(&tranche_key)
            .ok_or(Error::TrancheNotFound)?;

        if config.is_settled {
            return Err(Error::AlreadySettled);
        }

        if tranche_type == TrancheType::Senior as u32 {
            config.senior_filled = config
                .senior_filled
                .checked_add(amount)
                .ok_or(Error::ArithmeticOverflow)?;
        } else {
            config.junior_filled = config
                .junior_filled
                .checked_add(amount)
                .ok_or(Error::ArithmeticOverflow)?;
        }

        let inv_key = DataKey::Investment(invoice_id.clone(), investor.clone(), tranche_type);
        let previous: i128 = env.storage().persistent().get(&inv_key).unwrap_or(0);
        let updated_inv = previous
            .checked_add(amount)
            .ok_or(Error::ArithmeticOverflow)?;

        env.storage().persistent().set(&tranche_key, &config);
        env.storage().persistent().set(&inv_key, &updated_inv);

        events::tranche_investment_recorded(&env, &invoice_id, &investor, tranche_type, amount);

        Ok(())
    }

    /// Settle tranche payouts and explicitly record any residual surplus owned
    /// by the issuer so that `senior_payout + junior_payout + issuer_surplus == settlement_amount`.
    pub fn settle_tranches(
        env: Env,
        invoice_id: BytesN<32>,
        settlement_amount: i128,
    ) -> Result<SettlementResult, Error> {
        let tranche_key = DataKey::Tranche(invoice_id.clone());
        let mut config: TrancheConfig = env
            .storage()
            .persistent()
            .get(&tranche_key)
            .ok_or(Error::TrancheNotFound)?;

        config.issuer.require_auth();

        if config.is_settled {
            return Err(Error::AlreadySettled);
        }

        if settlement_amount < 0 {
            return Err(Error::InvalidAmount);
        }

        let premium_bps = config.junior_yield_premium_bps as i128;
        if premium_bps > BPS_DENOMINATOR {
            return Err(Error::InvalidTrancheConfig);
        }

        // Senior tranche repaid first up to filled principal.
        let senior_payout = settlement_amount.min(config.senior_filled);
        let remainder = settlement_amount
            .checked_sub(senior_payout)
            .ok_or(Error::ArithmeticOverflow)?;

        // Junior tranche receives principal plus configured share of remaining surplus;
        // any unallocated surplus (including integer-division dust) belongs to the issuer.
        let (junior_payout, issuer_surplus) = if config.junior_filled == 0 {
            (0, remainder)
        } else if remainder > config.junior_filled {
            let surplus = remainder
                .checked_sub(config.junior_filled)
                .ok_or(Error::ArithmeticOverflow)?;
            let bonus = surplus
                .checked_mul(premium_bps)
                .ok_or(Error::ArithmeticOverflow)?
                .checked_div(BPS_DENOMINATOR)
                .ok_or(Error::ArithmeticOverflow)?;
            let junior_total = config
                .junior_filled
                .checked_add(bonus)
                .ok_or(Error::ArithmeticOverflow)?;
            let residual = surplus
                .checked_sub(bonus)
                .ok_or(Error::ArithmeticOverflow)?;
            (junior_total, residual)
        } else {
            (remainder, 0)
        };

        let total_allocated = senior_payout
            .checked_add(junior_payout)
            .and_then(|sum| sum.checked_add(issuer_surplus))
            .ok_or(Error::ArithmeticOverflow)?;
        if total_allocated != settlement_amount {
            return Err(Error::ArithmeticOverflow);
        }

        config.senior_payout = senior_payout;
        config.junior_payout = junior_payout;
        config.issuer_surplus = issuer_surplus;
        config.is_settled = true;

        env.storage().persistent().set(&tranche_key, &config);

        events::tranche_settled(
            &env,
            &invoice_id,
            senior_payout,
            junior_payout,
            issuer_surplus,
        );

        Ok(SettlementResult {
            senior_payout,
            junior_payout,
            issuer_surplus,
        })
    }

    pub fn get_tranche_status(env: Env, invoice_id: BytesN<32>) -> Result<TrancheStatus, Error> {
        let tranche_key = DataKey::Tranche(invoice_id);
        let config: TrancheConfig = env
            .storage()
            .persistent()
            .get(&tranche_key)
            .ok_or(Error::TrancheNotFound)?;

        let junior_pct = 100 - config.senior_pct;

        Ok(TrancheStatus {
            senior_pct: config.senior_pct,
            junior_pct,
            senior_filled: config.senior_filled,
            junior_filled: config.junior_filled,
            junior_yield_premium_bps: config.junior_yield_premium_bps,
            is_settled: config.is_settled,
            senior_payout: config.senior_payout,
            junior_payout: config.junior_payout,
            issuer_surplus: config.issuer_surplus,
        })
    }

    pub fn get_investor_tranche_amount(
        env: Env,
        invoice_id: BytesN<32>,
        investor: Address,
        tranche_type: u32,
    ) -> i128 {
        let inv_key = DataKey::Investment(invoice_id, investor, tranche_type);
        env.storage().persistent().get(&inv_key).unwrap_or(0)
    }
}

#[cfg(test)]
mod test;
