//! unstake: burn $STAKE, refund $BEEF, settle pending $MILK. Design doc §6.3.
//!
//! KEY: redeem limit is guarded by the $STAKE burn (insufficient $STAKE => burn
//! fails => whole atomic tx reverts). `user_info.amount` is a reward mirror only,
//! kept in sync via checked_sub. See design doc §6.3 + §18.
//!
//! Step 2.4 (core) + step 3.5 (reward hookup, strategy A: pending minted immediately).

use crate::constants::*;
use crate::errors::StakingError;
use crate::events::UnstakeEvent;
use crate::instructions::reward;
use crate::state::{Config, UserInfo};
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Burn, Mint, MintTo, Token, TokenAccount, Transfer};

#[derive(Accounts)]
pub struct Unstake<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(mut, seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Account<'info, Config>,

    #[account(
        mut,
        seeds = [USER_SEED, user.key().as_ref()],
        bump = user_info.bump,
        constraint = user_info.owner == user.key() @ StakingError::Unauthorized,
    )]
    pub user_info: Account<'info, UserInfo>,

    #[account(mut, constraint = user_beef_ata.mint == config.beef_mint @ StakingError::InvalidMint)]
    pub user_beef_ata: Account<'info, TokenAccount>,

    #[account(mut, address = config.vault)]
    pub vault: Account<'info, TokenAccount>,

    #[account(mut, constraint = user_stake_ata.mint == config.stake_mint @ StakingError::InvalidMint)]
    pub user_stake_ata: Account<'info, TokenAccount>,

    #[account(mut, address = config.stake_mint)]
    pub stake_mint: Account<'info, Mint>,

    // Reward (strategy A): pending $MILK is minted during unstake.
    #[account(mut, address = config.milk_mint)]
    pub milk_mint: Account<'info, Mint>,

    #[account(mut, constraint = user_milk_ata.mint == config.milk_mint @ StakingError::InvalidMint)]
    pub user_milk_ata: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn handler(ctx: Context<Unstake>, amount: u64) -> Result<()> {
    require!(amount > 0, StakingError::AmountZero);

    let now = Clock::get()?.unix_timestamp;

    // 1) Settle the pool BEFORE the share change.
    reward::update_pool(&mut ctx.accounts.config, now)?;
    let acc = ctx.accounts.config.acc_reward_per_share;
    let config_bump = ctx.accounts.config.bump;

    // 2) Capture pending against the OLD amount/debt.
    let pending = reward::pending_reward(
        ctx.accounts.user_info.amount,
        acc,
        ctx.accounts.user_info.reward_debt,
    )?;

    // 3) Effects: checked_sub mirror + total; underflow => InsufficientStake.
    {
        let user_info = &mut ctx.accounts.user_info;
        user_info.amount = user_info
            .amount
            .checked_sub(amount)
            .ok_or(StakingError::InsufficientStake)?;
        user_info.reward_debt = reward::reward_debt_for(user_info.amount, acc)?;
    }
    {
        let config = &mut ctx.accounts.config;
        config.total_staked = config
            .total_staked
            .checked_sub(amount)
            .ok_or(StakingError::MathOverflow)?;
    }

    // 4) Interactions (atomic): burn $STAKE, refund $BEEF, settle pending $MILK.
    let signer_seeds: &[&[&[u8]]] = &[&[CONFIG_SEED, &[config_bump]]];

    // burn $STAKE from user (user signs) — the authoritative principal guard
    token::burn(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Burn {
                mint: ctx.accounts.stake_mint.to_account_info(),
                from: ctx.accounts.user_stake_ata.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        amount,
    )?;

    // refund $BEEF: vault -> user (Config PDA signs)
    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.vault.to_account_info(),
                to: ctx.accounts.user_beef_ata.to_account_info(),
                authority: ctx.accounts.config.to_account_info(),
            },
            signer_seeds,
        ),
        amount,
    )?;

    // settle pending $MILK (Config PDA signs)
    if pending > 0 {
        token::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    mint: ctx.accounts.milk_mint.to_account_info(),
                    to: ctx.accounts.user_milk_ata.to_account_info(),
                    authority: ctx.accounts.config.to_account_info(),
                },
                signer_seeds,
            ),
            pending,
        )?;
    }

    emit!(UnstakeEvent {
        user: ctx.accounts.user.key(),
        amount,
        remaining: ctx.accounts.user_info.amount,
    });
    Ok(())
}
