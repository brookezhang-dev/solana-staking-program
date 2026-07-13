//! create_pool (v3.x, admin-gated): create a Pool for a (staked_mint, reward_mint)
//! pair + its two vaults. The externally-created $STAKE receipt mint (Token-2022 +
//! TransferHook → this program, authority already = the Pool PDA) is validated and
//! recorded. Reward vault is pre-funded afterwards via a plain transfer / fund_rewards.
//! Duplicate pools for the same pair are impossible (PDA collision on the seed).

use crate::constants::*;
use crate::errors::StakingError;
use crate::events::PoolCreated;
use crate::instructions::token_ext::{require_no_transfer_hook, require_transfer_hook_to};
use crate::state::{Config, Pool};
use anchor_lang::prelude::*;
use anchor_lang::solana_program::program_option::COption;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

#[derive(Accounts)]
pub struct CreatePool<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = admin @ StakingError::Unauthorized,
    )]
    pub config: Account<'info, Config>,

    pub staked_mint: Box<InterfaceAccount<'info, Mint>>,
    pub reward_mint: Box<InterfaceAccount<'info, Mint>>,

    // $STAKE receipt mint: mint authority must already be the Pool PDA, and decimals
    // must match staked_mint 1:1 (stake.rs mints `credited` raw units as receipt).
    #[account(
        constraint = stake_receipt_mint.mint_authority == COption::Some(pool.key()) @ StakingError::Unauthorized,
        constraint = stake_receipt_mint.decimals == staked_mint.decimals @ StakingError::InvalidMint,
    )]
    pub stake_receipt_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        init,
        payer = admin,
        space = 8 + Pool::SPACE,
        seeds = [POOL_SEED, config.key().as_ref(), staked_mint.key().as_ref(), reward_mint.key().as_ref()],
        bump
    )]
    pub pool: Account<'info, Pool>,

    #[account(
        init, payer = admin,
        seeds = [STAKED_VAULT_SEED, pool.key().as_ref()], bump,
        token::mint = staked_mint,
        token::authority = pool,
        token::token_program = staked_token_program,
    )]
    pub staked_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init, payer = admin,
        seeds = [REWARD_VAULT_SEED, pool.key().as_ref()], bump,
        token::mint = reward_mint,
        token::authority = pool,
        token::token_program = reward_token_program,
    )]
    pub reward_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    pub staked_token_program: Interface<'info, TokenInterface>,
    pub reward_token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn create_pool_handler(
    ctx: Context<CreatePool>,
    reward_per_sec: u64,
    start_time: i64,
    end_time: i64,
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;

    // Extension checks: $STAKE must route to our hook; staked/reward reject hooks.
    require_transfer_hook_to(&ctx.accounts.stake_receipt_mint.to_account_info(), &crate::ID)?;
    require_no_transfer_hook(&ctx.accounts.staked_mint.to_account_info())?;
    require_no_transfer_hook(&ctx.accounts.reward_mint.to_account_info())?;

    let start = if start_time == 0 { now } else { start_time };
    require!(
        end_time == 0 || (end_time > now && end_time > start),
        StakingError::InvalidTimeRange
    );

    let pool = &mut ctx.accounts.pool;
    pool.config = ctx.accounts.config.key();
    pool.staked_mint = ctx.accounts.staked_mint.key();
    pool.reward_mint = ctx.accounts.reward_mint.key();
    pool.stake_receipt_mint = ctx.accounts.stake_receipt_mint.key();
    pool.staked_vault = ctx.accounts.staked_vault.key();
    pool.reward_vault = ctx.accounts.reward_vault.key();
    pool.acc_reward_per_share = 0;
    pool.last_reward_time = start;
    pool.start_time = start;
    pool.end_time = end_time;
    pool.reward_per_sec = reward_per_sec;
    pool.total_emitted = 0;
    pool.total_staked = 0;
    pool.total_claimed = 0;
    pool.bump = ctx.bumps.pool;
    pool.reserved = [0u8; 64];

    let c = &mut ctx.accounts.config;
    c.pool_count = c.pool_count.checked_add(1).ok_or(StakingError::MathOverflow)?;

    emit!(PoolCreated {
        pool: pool.key(),
        staked_mint: pool.staked_mint,
        reward_mint: pool.reward_mint,
        stake_receipt_mint: pool.stake_receipt_mint,
        reward_per_sec,
        start_time: start,
        end_time,
    });
    Ok(())
}
