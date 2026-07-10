//! close_user_info (v3.x): owner reclaims rent for an empty UserInfo (pool-scoped).
//! Empty = $STAKE token-account balance == 0 AND pending_unclaimed == 0.

use crate::constants::*;
use crate::errors::StakingError;
use crate::events::UserInfoClosed;
use crate::state::{Config, Pool, UserInfo};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount};

#[derive(Accounts)]
pub struct CloseUserInfo<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Account<'info, Config>,

    #[account(
        seeds = [POOL_SEED, config.key().as_ref(), pool.staked_mint.as_ref(), pool.reward_mint.as_ref()],
        bump = pool.bump,
        constraint = pool.config == config.key() @ StakingError::PoolConfigMismatch,
    )]
    pub pool: Account<'info, Pool>,

    #[account(address = pool.stake_receipt_mint)]
    pub stake_receipt_mint: InterfaceAccount<'info, Mint>,
    #[account(token::mint = stake_receipt_mint, token::authority = owner)]
    pub stake_token_account: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        close = owner,
        seeds = [USER_SEED, pool.key().as_ref(), stake_token_account.key().as_ref()],
        bump = user_info.bump,
        constraint = user_info.token_account == stake_token_account.key() @ StakingError::Unauthorized,
    )]
    pub user_info: Account<'info, UserInfo>,
}

pub fn handler(ctx: Context<CloseUserInfo>) -> Result<()> {
    require!(
        ctx.accounts.stake_token_account.amount == 0
            && ctx.accounts.user_info.pending_unclaimed == 0,
        StakingError::AccountNotEmpty
    );
    emit!(UserInfoClosed {
        pool: ctx.accounts.pool.key(),
        user: ctx.accounts.owner.key(),
    });
    Ok(())
}
