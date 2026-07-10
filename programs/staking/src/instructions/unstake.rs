//! unstake (v3.x): pool-scoped, balance-authoritative. Guard = burn + explicit
//! `amount <= balance`. Refund staked token from vault; settle pending. NEVER
//! blocked by pause (users must always exit principal).

use crate::constants::*;
use crate::errors::StakingError;
use crate::events::UnstakeEvent;
use crate::instructions::reward;
use crate::state::{Config, Pool, UserInfo};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Burn, Mint, TokenAccount, TokenInterface, TransferChecked};

#[derive(Accounts)]
pub struct Unstake<'info> {
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

    #[account(mut, address = pool.staked_mint)]
    pub staked_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut, address = pool.stake_receipt_mint)]
    pub stake_receipt_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(mut, token::mint = staked_mint, token::authority = user)]
    pub user_staked_ata: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, address = pool.staked_vault)]
    pub staked_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, token::mint = stake_receipt_mint, token::authority = user)]
    pub user_stake_ata: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [USER_SEED, pool.key().as_ref(), user_stake_ata.key().as_ref()],
        bump = user_info.bump,
    )]
    pub user_info: Account<'info, UserInfo>,

    pub staked_token_program: Interface<'info, TokenInterface>,
    pub stake_token_program: Interface<'info, TokenInterface>,
}

pub fn handler(ctx: Context<Unstake>, amount: u64) -> Result<()> {
    require!(amount > 0, StakingError::AmountZero);
    let now = Clock::get()?.unix_timestamp;

    reward::update_pool(&mut ctx.accounts.pool, now)?;
    let acc = ctx.accounts.pool.acc_reward_per_share;

    let pre = ctx.accounts.user_stake_ata.amount;
    require!(amount <= pre, StakingError::InsufficientStake);
    let pending = reward::pending_reward(pre, acc, ctx.accounts.user_info.reward_debt)?;
    let post = pre - amount;

    {
        let ui = &mut ctx.accounts.user_info;
        ui.pending_unclaimed = ui.pending_unclaimed.checked_add(pending).ok_or(StakingError::MathOverflow)?;
        ui.reward_debt = reward::reward_debt_for(post, acc)?;
    }
    {
        let p = &mut ctx.accounts.pool;
        p.total_staked = p.total_staked.checked_sub(amount).ok_or(StakingError::MathOverflow)?;
    }

    // Burn $STAKE (user signs; does not trigger the hook).
    token_interface::burn(
        CpiContext::new(
            ctx.accounts.stake_token_program.to_account_info(),
            Burn {
                mint: ctx.accounts.stake_receipt_mint.to_account_info(),
                from: ctx.accounts.user_stake_ata.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        amount,
    )?;

    // Refund staked token: vault -> user (Pool PDA signs).
    let cfg = ctx.accounts.pool.config;
    let sm = ctx.accounts.pool.staked_mint;
    let rm = ctx.accounts.pool.reward_mint;
    let pb = ctx.accounts.pool.bump;
    let signer: &[&[&[u8]]] = &[&[POOL_SEED, cfg.as_ref(), sm.as_ref(), rm.as_ref(), &[pb]]];
    token_interface::transfer_checked(
        CpiContext::new_with_signer(
            ctx.accounts.staked_token_program.to_account_info(),
            TransferChecked {
                from: ctx.accounts.staked_vault.to_account_info(),
                mint: ctx.accounts.staked_mint.to_account_info(),
                to: ctx.accounts.user_staked_ata.to_account_info(),
                authority: ctx.accounts.pool.to_account_info(),
            },
            signer,
        ),
        amount,
        ctx.accounts.staked_mint.decimals,
    )?;

    emit!(UnstakeEvent {
        pool: ctx.accounts.pool.key(),
        user: ctx.accounts.user.key(),
        amount,
        remaining: post,
    });
    Ok(())
}
