//! initialize (v3): create Config + Vault + RewardVault; validate $STAKE is a
//! Config-PDA-authority NonTransferable mint; reject transfer-hook beef/reward;
//! store mints + emission params (start/end + rate anchor). See v3 执行计划 §1.3.

use crate::constants::*;
use crate::errors::StakingError;
use crate::instructions::token_ext::{require_no_transfer_hook, require_non_transferable};
use crate::state::Config;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::program_option::COption;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        init,
        payer = admin,
        space = 8 + Config::SPACE,
        seeds = [CONFIG_SEED],
        bump
    )]
    pub config: Account<'info, Config>,

    pub beef_mint: InterfaceAccount<'info, Mint>,

    // $STAKE mint authority must already be the Config PDA (program mints it).
    #[account(constraint = stake_mint.mint_authority == COption::Some(config.key()) @ StakingError::Unauthorized)]
    pub stake_mint: InterfaceAccount<'info, Mint>,

    pub reward_mint: InterfaceAccount<'info, Mint>,

    #[account(
        init, payer = admin,
        seeds = [VAULT_SEED], bump,
        token::mint = beef_mint,
        token::authority = config,
        token::token_program = beef_token_program,
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        init, payer = admin,
        seeds = [REWARD_VAULT_SEED], bump,
        token::mint = reward_mint,
        token::authority = config,
        token::token_program = reward_token_program,
    )]
    pub reward_vault: InterfaceAccount<'info, TokenAccount>,

    pub beef_token_program: Interface<'info, TokenInterface>,
    pub reward_token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handler(
    ctx: Context<Initialize>,
    initial_rate: u64,
    decay_per_sec: u64,
    min_rate: u64,
    start_time: i64,
    end_time: i64,
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;

    // --- extension checks ---
    require_non_transferable(&ctx.accounts.stake_mint.to_account_info())?;
    require_no_transfer_hook(&ctx.accounts.beef_mint.to_account_info())?;
    require_no_transfer_hook(&ctx.accounts.reward_mint.to_account_info())?;

    // --- param validation ---
    require!(min_rate <= initial_rate, StakingError::InvalidEmissionParams);
    let start = if start_time == 0 { now } else { start_time };
    require!(
        end_time == 0 || (end_time > now && end_time > start),
        StakingError::InvalidEndTime
    );

    let config = &mut ctx.accounts.config;
    config.admin = ctx.accounts.admin.key();
    config.beef_mint = ctx.accounts.beef_mint.key();
    config.stake_mint = ctx.accounts.stake_mint.key();
    config.reward_mint = ctx.accounts.reward_mint.key();
    config.vault = ctx.accounts.vault.key();
    config.reward_vault = ctx.accounts.reward_vault.key();

    config.total_staked = 0;
    config.acc_reward_per_share = 0;
    config.last_reward_time = start;
    config.start_time = start;
    config.rate_anchor_time = start;
    config.end_time = end_time;

    config.initial_rate = initial_rate;
    config.decay_per_sec = decay_per_sec;
    config.min_rate = min_rate;
    config.total_claimed = 0;

    config.bump = ctx.bumps.config;
    config.vault_bump = ctx.bumps.vault;
    config.reward_vault_bump = ctx.bumps.reward_vault;
    config.reserved = [0u8; 64];
    Ok(())
}
