//! claim_rewards (v3): settle → pay ALL pending_unclaimed from RewardVault
//! (transfer_checked, PDA signs). Vault insufficient => whole tx fails (§9.1=a).

use crate::constants::*;
use crate::errors::StakingError;
use crate::events::ClaimEvent;
use crate::instructions::reward;
use crate::state::{Config, UserInfo};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

#[derive(Accounts)]
pub struct ClaimRewards<'info> {
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

    #[account(mut, address = config.reward_mint)]
    pub reward_mint: InterfaceAccount<'info, Mint>,
    #[account(mut, address = config.reward_vault)]
    pub reward_vault: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = reward_mint, token::authority = user)]
    pub user_reward_ata: InterfaceAccount<'info, TokenAccount>,

    pub reward_token_program: Interface<'info, TokenInterface>,
}

pub fn handler(ctx: Context<ClaimRewards>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    reward::update_pool(&mut ctx.accounts.config, now)?;
    let acc = ctx.accounts.config.acc_reward_per_share;

    // Fold freshly-accrued pending into the stored pending_unclaimed.
    let fresh = reward::pending_reward(
        ctx.accounts.user_info.amount,
        acc,
        ctx.accounts.user_info.reward_debt,
    )?;
    let payout = ctx
        .accounts
        .user_info
        .pending_unclaimed
        .checked_add(fresh)
        .ok_or(StakingError::MathOverflow)?;
    require!(payout > 0, StakingError::NothingToClaim);

    // Solvency: RewardVault must cover the payout, else whole tx fails (§9.1=a).
    require!(
        ctx.accounts.reward_vault.amount >= payout,
        StakingError::RewardVaultInsufficient
    );

    let config_bump = ctx.accounts.config.bump;
    let signer: &[&[&[u8]]] = &[&[CONFIG_SEED, &[config_bump]]];
    token_interface::transfer_checked(
        CpiContext::new_with_signer(
            ctx.accounts.reward_token_program.to_account_info(),
            TransferChecked {
                from: ctx.accounts.reward_vault.to_account_info(),
                mint: ctx.accounts.reward_mint.to_account_info(),
                to: ctx.accounts.user_reward_ata.to_account_info(),
                authority: ctx.accounts.config.to_account_info(),
            },
            signer,
        ),
        payout,
        ctx.accounts.reward_mint.decimals,
    )?;

    // Effects after successful payout.
    {
        let ui = &mut ctx.accounts.user_info;
        ui.pending_unclaimed = 0;
        ui.reward_debt = reward::reward_debt_for(ui.amount, acc)?;
    }
    {
        let c = &mut ctx.accounts.config;
        c.total_claimed = c.total_claimed.checked_add(payout).ok_or(StakingError::MathOverflow)?;
    }

    emit!(ClaimEvent {
        user: ctx.accounts.user.key(),
        amount: payout,
    });
    Ok(())
}
