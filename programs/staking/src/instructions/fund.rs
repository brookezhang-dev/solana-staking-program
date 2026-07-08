//! fund_rewards (v3): anyone tops up the RewardVault (in-only). Balance-diff so
//! fee-token rewards credit the actual received amount. Emits FundEvent for the
//! off-chain water-level monitor. §1.3.

use crate::constants::*;
use crate::errors::StakingError;
use crate::events::FundEvent;
use crate::state::Config;
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

#[derive(Accounts)]
pub struct FundRewards<'info> {
    #[account(mut)]
    pub funder: Signer<'info>,

    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Account<'info, Config>,

    #[account(mut, address = config.reward_mint)]
    pub reward_mint: InterfaceAccount<'info, Mint>,
    #[account(mut, address = config.reward_vault)]
    pub reward_vault: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = reward_mint, token::authority = funder)]
    pub funder_reward_ata: InterfaceAccount<'info, TokenAccount>,

    pub reward_token_program: Interface<'info, TokenInterface>,
}

pub fn handler(ctx: Context<FundRewards>, amount: u64) -> Result<()> {
    require!(amount > 0, StakingError::AmountZero);

    let before = ctx.accounts.reward_vault.amount;
    token_interface::transfer_checked(
        CpiContext::new(
            ctx.accounts.reward_token_program.to_account_info(),
            TransferChecked {
                from: ctx.accounts.funder_reward_ata.to_account_info(),
                mint: ctx.accounts.reward_mint.to_account_info(),
                to: ctx.accounts.reward_vault.to_account_info(),
                authority: ctx.accounts.funder.to_account_info(),
            },
        ),
        amount,
        ctx.accounts.reward_mint.decimals,
    )?;
    ctx.accounts.reward_vault.reload()?;
    let credited = ctx
        .accounts
        .reward_vault
        .amount
        .checked_sub(before)
        .ok_or(StakingError::MathOverflow)?;

    emit!(FundEvent {
        funder: ctx.accounts.funder.key(),
        credited,
        reward_vault_balance: ctx.accounts.reward_vault.amount,
    });
    Ok(())
}
