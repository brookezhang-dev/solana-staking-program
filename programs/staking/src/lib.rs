//! Solana DeFi staking program (Anchor, v3).
//! $BEEF → 1:1 NonTransferable $STAKE; MasterChef O(1) rewards paid from a
//! prefunded RewardVault; configurable linear-decay emission with re-anchoring
//! and end_time. See v3 执行计划 / 技术设计文档 v3.

use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod events;
pub mod instructions;
pub mod state;

use instructions::*;

declare_id!("54HWhVGu8HoK46PUj3ijauVjrgNScGHyzpnvHsvZGpcv");

#[program]
pub mod staking {
    use super::*;

    /// Create Config + Vault + RewardVault; validate $STAKE (NonTransferable,
    /// authority = Config PDA) and reject transfer-hook beef/reward; store params.
    pub fn initialize(
        ctx: Context<Initialize>,
        initial_rate: u64,
        decay_per_sec: u64,
        min_rate: u64,
        start_time: i64,
        end_time: i64,
    ) -> Result<()> {
        instructions::initialize::handler(ctx, initial_rate, decay_per_sec, min_rate, start_time, end_time)
    }

    /// Deposit $BEEF (balance-diff), mint equal $STAKE, settle pending (strategy B).
    pub fn stake(ctx: Context<Stake>, amount: u64) -> Result<()> {
        instructions::stake::handler(ctx, amount)
    }

    /// Burn $STAKE, refund $BEEF from Vault, settle pending (strategy B).
    pub fn unstake(ctx: Context<Unstake>, amount: u64) -> Result<()> {
        instructions::unstake::handler(ctx, amount)
    }

    /// Pay all pending_unclaimed from RewardVault (insufficient => fails).
    pub fn claim_rewards(ctx: Context<ClaimRewards>) -> Result<()> {
        instructions::claim::handler(ctx)
    }

    /// Top up the RewardVault (anyone, in-only).
    pub fn fund_rewards(ctx: Context<FundRewards>, amount: u64) -> Result<()> {
        instructions::fund::handler(ctx, amount)
    }

    /// Admin: update emission params with re-anchoring (settle → write → re-anchor).
    pub fn set_emission_params(
        ctx: Context<SetEmissionParams>,
        initial_rate: u64,
        decay_per_sec: u64,
        min_rate: u64,
        end_time: i64,
    ) -> Result<()> {
        instructions::admin::handler(ctx, initial_rate, decay_per_sec, min_rate, end_time)
    }

    /// Devnet-only faucet (compiled under `devnet-faucet` feature).
    #[cfg(feature = "devnet-faucet")]
    pub fn faucet(ctx: Context<Faucet>, amount: u64) -> Result<()> {
        instructions::faucet::handler(ctx, amount)
    }
}
