//! faucet (v3.x, devnet-only): mint capped test staked-token (e.g. $BEEF) to the
//! caller for a given pool. Requires the pool's staked_mint authority = the Pool PDA.
//! Compiled ONLY under `feature = "devnet-faucet"`.

use crate::constants::*;
use crate::errors::StakingError;
use crate::state::{Config, Pool};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Mint, MintTo, TokenAccount, TokenInterface};

#[derive(Accounts)]
pub struct Faucet<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Account<'info, Config>,

    #[account(
        seeds = [POOL_SEED, config.key().as_ref(), pool.staked_mint.as_ref(), pool.reward_mint.as_ref()],
        bump = pool.bump,
        constraint = pool.config == config.key() @ StakingError::PoolConfigMismatch,
    )]
    pub pool: Account<'info, Pool>,

    #[account(mut, address = pool.staked_mint)]
    pub staked_mint: InterfaceAccount<'info, Mint>,
    #[account(mut, token::mint = staked_mint, token::authority = user)]
    pub user_staked_ata: InterfaceAccount<'info, TokenAccount>,

    pub staked_token_program: Interface<'info, TokenInterface>,
}

pub fn faucet_handler(ctx: Context<Faucet>, amount: u64) -> Result<()> {
    require!(amount > 0, StakingError::AmountZero);
    require!(amount <= FAUCET_MAX, StakingError::FaucetTooMuch);

    let cfg = ctx.accounts.pool.config;
    let sm = ctx.accounts.pool.staked_mint;
    let rm = ctx.accounts.pool.reward_mint;
    let pb = ctx.accounts.pool.bump;
    let signer: &[&[&[u8]]] = &[&[POOL_SEED, cfg.as_ref(), sm.as_ref(), rm.as_ref(), &[pb]]];
    token_interface::mint_to(
        CpiContext::new_with_signer(
            ctx.accounts.staked_token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.staked_mint.to_account_info(),
                to: ctx.accounts.user_staked_ata.to_account_info(),
                authority: ctx.accounts.pool.to_account_info(),
            },
            signer,
        ),
        amount,
    )?;
    Ok(())
}
