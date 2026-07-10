//! claim_rewards (v3.x): pool-scoped. Settle → pay pending_unclaimed from the pool's
//! reward vault. Balance-authoritative fresh accrual. Insufficient => whole tx fails.

use crate::constants::*;
use crate::errors::StakingError;
use crate::events::ClaimEvent;
use crate::instructions::reward;
use crate::state::{Config, Pool, UserInfo};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

#[derive(Accounts)]
pub struct ClaimRewards<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

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
    pub stake_receipt_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(token::mint = stake_receipt_mint, token::authority = user)]
    pub user_stake_ata: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [USER_SEED, pool.key().as_ref(), user_stake_ata.key().as_ref()],
        bump = user_info.bump,
    )]
    pub user_info: Account<'info, UserInfo>,

    #[account(mut, address = pool.reward_mint)]
    pub reward_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut, address = pool.reward_vault)]
    pub reward_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, token::mint = reward_mint, token::authority = user)]
    pub user_reward_ata: Box<InterfaceAccount<'info, TokenAccount>>,

    pub reward_token_program: Interface<'info, TokenInterface>,
}

pub fn handler(ctx: Context<ClaimRewards>) -> Result<()> {
    require!(!ctx.accounts.config.paused, StakingError::Paused);
    let now = Clock::get()?.unix_timestamp;
    reward::update_pool(&mut ctx.accounts.pool, now)?;
    let acc = ctx.accounts.pool.acc_reward_per_share;

    let bal = ctx.accounts.user_stake_ata.amount;
    let fresh = reward::pending_reward(bal, acc, ctx.accounts.user_info.reward_debt)?;
    let payout = ctx
        .accounts
        .user_info
        .pending_unclaimed
        .checked_add(fresh)
        .ok_or(StakingError::MathOverflow)?;
    require!(payout > 0, StakingError::NothingToClaim);
    require!(ctx.accounts.reward_vault.amount >= payout, StakingError::RewardVaultInsufficient);

    let cfg = ctx.accounts.pool.config;
    let sm = ctx.accounts.pool.staked_mint;
    let rm = ctx.accounts.pool.reward_mint;
    let pb = ctx.accounts.pool.bump;
    let signer: &[&[&[u8]]] = &[&[POOL_SEED, cfg.as_ref(), sm.as_ref(), rm.as_ref(), &[pb]]];
    token_interface::transfer_checked(
        CpiContext::new_with_signer(
            ctx.accounts.reward_token_program.to_account_info(),
            TransferChecked {
                from: ctx.accounts.reward_vault.to_account_info(),
                mint: ctx.accounts.reward_mint.to_account_info(),
                to: ctx.accounts.user_reward_ata.to_account_info(),
                authority: ctx.accounts.pool.to_account_info(),
            },
            signer,
        ),
        payout,
        ctx.accounts.reward_mint.decimals,
    )?;

    {
        let ui = &mut ctx.accounts.user_info;
        ui.pending_unclaimed = 0;
        ui.reward_debt = reward::reward_debt_for(bal, acc)?;
    }
    {
        let p = &mut ctx.accounts.pool;
        p.total_claimed = p.total_claimed.checked_add(payout).ok_or(StakingError::MathOverflow)?;
    }

    emit!(ClaimEvent {
        pool: ctx.accounts.pool.key(),
        user: ctx.accounts.user.key(),
        amount: payout,
    });
    Ok(())
}
