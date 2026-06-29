//! Solana DeFi staking program (Anchor) with MasterChef-style rewards.
//! Architecture, account model and security: see the Technical Design Document.

use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod events;
pub mod instructions;
pub mod state;

use instructions::*;

// Replace with the program id produced by `anchor build`/`anchor keys list`.
declare_id!("6boJRbzGer4vYjprjSoAz879g68JRKXHvSsATsBRaZSq");

#[program]
pub mod staking {
    use super::*;

    /// Create Config + Vault, set $STAKE/$MILK mint authority to Config PDA,
    /// store emission params. See design doc §6.1.
    pub fn initialize(
        ctx: Context<Initialize>,
        initial_rate: u64,
        decay_per_sec: u64,
        min_rate: u64,
    ) -> Result<()> {
        instructions::initialize::handler(ctx, initial_rate, decay_per_sec, min_rate)
    }

    /// Deposit $BEEF into vault, mint equal $STAKE to user. See design doc §6.2.
    pub fn stake(ctx: Context<Stake>, amount: u64) -> Result<()> {
        instructions::stake::handler(ctx, amount)
    }

    /// Burn $STAKE, refund equal $BEEF from vault (partial allowed). See design doc §6.3.
    pub fn unstake(ctx: Context<Unstake>, amount: u64) -> Result<()> {
        instructions::unstake::handler(ctx, amount)
    }

    /// Settle and mint pending $MILK rewards (decoupled from principal). See design doc §6.4.
    pub fn claim_rewards(ctx: Context<ClaimRewards>) -> Result<()> {
        instructions::claim::handler(ctx)
    }

    /// Attach Metaplex Token Metadata (name/symbol) to $STAKE or $MILK so wallets
    /// show a readable name. Admin-only; Config PDA signs the metadata CPI.
    pub fn create_token_metadata(
        ctx: Context<CreateTokenMetadata>,
        name: String,
        symbol: String,
        uri: String,
    ) -> Result<()> {
        instructions::metadata::handler(ctx, name, symbol, uri)
    }

    /// Devnet faucet: mint capped test $BEEF to the caller so any new wallet can
    /// try staking. Requires $BEEF mint authority = Config PDA.
    pub fn faucet(ctx: Context<Faucet>, amount: u64) -> Result<()> {
        instructions::faucet::handler(ctx, amount)
    }
}
