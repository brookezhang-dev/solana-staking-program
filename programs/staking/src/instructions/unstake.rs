//! unstake (v3): checked_sub on the SOLE ledger `amount` (authority guard), burn
//! $STAKE (receipt), refund $BEEF from Vault; settle pending into pending_unclaimed.
//! §1.3.

use crate::constants::*;
use crate::errors::StakingError;
use crate::events::UnstakeEvent;
use crate::instructions::reward;
use crate::state::{Config, UserInfo};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Burn, Mint, TokenAccount, TokenInterface, TransferChecked};

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

    #[account(mut, address = config.beef_mint)]
    pub beef_mint: InterfaceAccount<'info, Mint>,
    #[account(mut, address = config.stake_mint)]
    pub stake_mint: InterfaceAccount<'info, Mint>,

    #[account(mut, token::mint = beef_mint, token::authority = user)]
    pub user_beef_ata: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, address = config.vault)]
    pub vault: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = stake_mint, token::authority = user)]
    pub user_stake_ata: InterfaceAccount<'info, TokenAccount>,

    pub beef_token_program: Interface<'info, TokenInterface>,
    pub stake_token_program: Interface<'info, TokenInterface>,
}

pub fn handler(ctx: Context<Unstake>, amount: u64) -> Result<()> {
    require!(amount > 0, StakingError::AmountZero);
    let now = Clock::get()?.unix_timestamp;

    reward::update_pool(&mut ctx.accounts.config, now)?;
    let acc = ctx.accounts.config.acc_reward_per_share;
    let config_bump = ctx.accounts.config.bump;

    let old_amount = ctx.accounts.user_info.amount;
    let pending = reward::pending_reward(old_amount, acc, ctx.accounts.user_info.reward_debt)?;

    // Effects: ledger checked_sub is the authoritative redeem guard.
    {
        let ui = &mut ctx.accounts.user_info;
        ui.amount = ui.amount.checked_sub(amount).ok_or(StakingError::InsufficientStake)?;
        ui.pending_unclaimed = ui
            .pending_unclaimed
            .checked_add(pending)
            .ok_or(StakingError::MathOverflow)?;
        ui.reward_debt = reward::reward_debt_for(ui.amount, acc)?;
    }
    {
        let c = &mut ctx.accounts.config;
        c.total_staked = c.total_staked.checked_sub(amount).ok_or(StakingError::MathOverflow)?;
    }

    // Burn $STAKE receipt (user signs). Under NonTransferable this equals the ledger sub.
    token_interface::burn(
        CpiContext::new(
            ctx.accounts.stake_token_program.to_account_info(),
            Burn {
                mint: ctx.accounts.stake_mint.to_account_info(),
                from: ctx.accounts.user_stake_ata.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        amount,
    )?;

    // Refund $BEEF: Vault -> user (Config PDA signs).
    let signer: &[&[&[u8]]] = &[&[CONFIG_SEED, &[config_bump]]];
    token_interface::transfer_checked(
        CpiContext::new_with_signer(
            ctx.accounts.beef_token_program.to_account_info(),
            TransferChecked {
                from: ctx.accounts.vault.to_account_info(),
                mint: ctx.accounts.beef_mint.to_account_info(),
                to: ctx.accounts.user_beef_ata.to_account_info(),
                authority: ctx.accounts.config.to_account_info(),
            },
            signer,
        ),
        amount,
        ctx.accounts.beef_mint.decimals,
    )?;

    emit!(UnstakeEvent {
        user: ctx.accounts.user.key(),
        amount,
        remaining: ctx.accounts.user_info.amount,
    });
    Ok(())
}
