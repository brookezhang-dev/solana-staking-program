//! register (v3.x): create the UserInfo PDA for a $STAKE token account in a given
//! pool. REQUIRED before that account can RECEIVE a transfer. stake auto-registers.

use crate::constants::*;
use crate::errors::StakingError;
use crate::instructions::reward;
use crate::state::{Config, Pool, UserInfo};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount};

#[derive(Accounts)]
pub struct Register<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Account<'info, Config>,

    #[account(
        mut,
        seeds = [POOL_SEED, config.key().as_ref(), pool.staked_mint.as_ref(), pool.reward_mint.as_ref()],
        bump = pool.bump,
        constraint = pool.config == config.key() @ StakingError::PoolConfigMismatch,
    )]
    pub pool: Account<'info, Pool>,

    #[account(address = pool.stake_receipt_mint)]
    pub stake_receipt_mint: InterfaceAccount<'info, Mint>,
    #[account(token::mint = stake_receipt_mint)]
    pub stake_token_account: InterfaceAccount<'info, TokenAccount>,

    #[account(
        init,
        payer = payer,
        space = 8 + UserInfo::SPACE,
        seeds = [USER_SEED, pool.key().as_ref(), stake_token_account.key().as_ref()],
        bump
    )]
    pub user_info: Account<'info, UserInfo>,

    pub system_program: Program<'info, System>,
}

pub fn register_handler(ctx: Context<Register>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    reward::update_pool(&mut ctx.accounts.pool, now)?;
    let acc = ctx.accounts.pool.acc_reward_per_share;
    let bal = ctx.accounts.stake_token_account.amount;

    let ui = &mut ctx.accounts.user_info;
    ui.token_account = ctx.accounts.stake_token_account.key();
    ui.reward_debt = reward::reward_debt_for(bal, acc)?;
    ui.pending_unclaimed = 0;
    ui.bump = ctx.bumps.user_info;
    ui.reserved = [0u8; 32];
    Ok(())
}
