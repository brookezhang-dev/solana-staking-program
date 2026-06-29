//! claim_rewards: settle + mint pending $MILK (decoupled from principal). Design doc §6.4.
//! Step 3.4. Constant emission for now (linear decay wired in reward.rs at step 3.7).

use crate::constants::*;
use crate::errors::StakingError;
use crate::events::ClaimEvent;
use crate::instructions::reward;
use crate::state::{Config, UserInfo};
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, MintTo, Token, TokenAccount};

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

    #[account(mut, address = config.milk_mint)]
    pub milk_mint: Account<'info, Mint>,

    #[account(mut, constraint = user_milk_ata.mint == config.milk_mint @ StakingError::InvalidMint)]
    pub user_milk_ata: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn handler(ctx: Context<ClaimRewards>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;

    // Settle the pool first (O(1)).
    reward::update_pool(&mut ctx.accounts.config, now)?;

    let acc = ctx.accounts.config.acc_reward_per_share;
    let pending = reward::pending_reward(
        ctx.accounts.user_info.amount,
        acc,
        ctx.accounts.user_info.reward_debt,
    )?;
    require!(pending > 0, StakingError::NothingToClaim);

    // Mint $MILK to user (Config PDA signs).
    let config_bump = ctx.accounts.config.bump;
    let signer_seeds: &[&[&[u8]]] = &[&[CONFIG_SEED, &[config_bump]]];
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

    // Reset reward debt to the new baseline.
    ctx.accounts.user_info.reward_debt =
        reward::reward_debt_for(ctx.accounts.user_info.amount, acc)?;

    emit!(ClaimEvent {
        user: ctx.accounts.user.key(),
        amount: pending,
    });
    Ok(())
}
