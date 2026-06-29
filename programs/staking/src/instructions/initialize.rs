//! initialize: create Config + Vault, store mints & emission params. Design doc §6.1.

use crate::constants::*;
use crate::state::Config;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::program_option::COption;
use anchor_spl::token::{Mint, Token, TokenAccount};

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

    pub beef_mint: Account<'info, Mint>,

    // $STAKE / $MILK mint authority must already be the Config PDA so the
    // program — and only the program — can mint.
    #[account(mut, constraint = stake_mint.mint_authority == COption::Some(config.key()))]
    pub stake_mint: Account<'info, Mint>,
    #[account(mut, constraint = milk_mint.mint_authority == COption::Some(config.key()))]
    pub milk_mint: Account<'info, Mint>,

    #[account(
        init,
        payer = admin,
        seeds = [VAULT_SEED],
        bump,
        token::mint = beef_mint,
        token::authority = config
    )]
    pub vault: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handler(
    ctx: Context<Initialize>,
    initial_rate: u64,
    decay_per_sec: u64,
    min_rate: u64,
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let config = &mut ctx.accounts.config;

    config.admin = ctx.accounts.admin.key();
    config.beef_mint = ctx.accounts.beef_mint.key();
    config.stake_mint = ctx.accounts.stake_mint.key();
    config.milk_mint = ctx.accounts.milk_mint.key();
    config.vault = ctx.accounts.vault.key();

    config.total_staked = 0;
    config.acc_reward_per_share = 0;
    config.last_reward_time = now;
    config.start_time = now;

    config.initial_rate = initial_rate;
    config.decay_per_sec = decay_per_sec;
    config.min_rate = min_rate;

    config.bump = ctx.bumps.config;
    config.vault_bump = ctx.bumps.vault;

    Ok(())
}
