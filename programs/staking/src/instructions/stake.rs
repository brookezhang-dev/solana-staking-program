//! stake (v3): transfer $BEEF in (balance-diff, fee-token safe), mint equal $STAKE,
//! settle pending into pending_unclaimed (strategy B, no reward transfer). §1.3.

use crate::constants::*;
use crate::errors::StakingError;
use crate::events::StakeEvent;
use crate::instructions::reward;
use crate::state::{Config, UserInfo};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    self, Mint, MintTo, TokenAccount, TokenInterface, TransferChecked,
};

#[derive(Accounts)]
pub struct Stake<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(mut, seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Account<'info, Config>,

    #[account(
        init_if_needed,
        payer = user,
        space = 8 + UserInfo::SPACE,
        seeds = [USER_SEED, user.key().as_ref()],
        bump
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
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<Stake>, amount: u64) -> Result<()> {
    require!(amount > 0, StakingError::AmountZero);
    let now = Clock::get()?.unix_timestamp;

    reward::update_pool(&mut ctx.accounts.config, now)?;
    let acc = ctx.accounts.config.acc_reward_per_share;
    let config_bump = ctx.accounts.config.bump;

    // Strategy B: settle pending against OLD amount → accumulate into pending_unclaimed.
    let old_amount = ctx.accounts.user_info.amount;
    let pending = reward::pending_reward(old_amount, acc, ctx.accounts.user_info.reward_debt)?;

    // Transfer $BEEF in; credit the ACTUAL received amount (balance-diff, fee-token safe).
    let before = ctx.accounts.vault.amount;
    token_interface::transfer_checked(
        CpiContext::new(
            ctx.accounts.beef_token_program.to_account_info(),
            TransferChecked {
                from: ctx.accounts.user_beef_ata.to_account_info(),
                mint: ctx.accounts.beef_mint.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        amount,
        ctx.accounts.beef_mint.decimals,
    )?;
    ctx.accounts.vault.reload()?;
    let credited = ctx
        .accounts
        .vault
        .amount
        .checked_sub(before)
        .ok_or(StakingError::MathOverflow)?;
    require!(credited > 0, StakingError::ZeroCredited);

    // Effects.
    {
        let ui = &mut ctx.accounts.user_info;
        ui.owner = ctx.accounts.user.key();
        ui.bump = ctx.bumps.user_info;
        ui.pending_unclaimed = ui
            .pending_unclaimed
            .checked_add(pending)
            .ok_or(StakingError::MathOverflow)?;
        ui.amount = ui.amount.checked_add(credited).ok_or(StakingError::MathOverflow)?;
        ui.reward_debt = reward::reward_debt_for(ui.amount, acc)?;
    }
    {
        let c = &mut ctx.accounts.config;
        c.total_staked = c.total_staked.checked_add(credited).ok_or(StakingError::MathOverflow)?;
    }

    // Mint receipt $STAKE = credited (1:1 with actual principal). Config PDA signs.
    let signer: &[&[&[u8]]] = &[&[CONFIG_SEED, &[config_bump]]];
    token_interface::mint_to(
        CpiContext::new_with_signer(
            ctx.accounts.stake_token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.stake_mint.to_account_info(),
                to: ctx.accounts.user_stake_ata.to_account_info(),
                authority: ctx.accounts.config.to_account_info(),
            },
            signer,
        ),
        credited,
    )?;

    emit!(StakeEvent {
        user: ctx.accounts.user.key(),
        credited,
        total: ctx.accounts.user_info.amount,
    });
    Ok(())
}
