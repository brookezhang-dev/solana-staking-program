//! Admin instructions (v3.x, all gated on Config.admin):
//!   set_pause        — global emergency stop (Config.paused)
//!   transfer_admin   — step 1/2: propose a new admin (does NOT take effect yet)
//!   accept_admin     — step 2/2: pending_admin signs to claim the role
//!   set_emission     — per-pool rate change (settle at OLD rate first)
//!   withdraw_surplus — per-pool: withdraw only balance above outstanding liability

use crate::constants::*;
use crate::errors::StakingError;
use crate::events::{AdminTransferProposed, AdminTransferred, EmissionSet, PauseSet, SurplusWithdrawn};
use crate::instructions::reward;
use crate::state::{Config, Pool};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

// --------------------------------------------------------------------------
// set_pause / transfer_admin — Config-level.
// --------------------------------------------------------------------------

#[derive(Accounts)]
pub struct AdminConfig<'info> {
    pub admin: Signer<'info>,
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = admin @ StakingError::Unauthorized,
    )]
    pub config: Account<'info, Config>,
}

pub fn set_pause_handler(ctx: Context<AdminConfig>, paused: bool) -> Result<()> {
    ctx.accounts.config.paused = paused;
    emit!(PauseSet { admin: ctx.accounts.admin.key(), paused });
    Ok(())
}

/// Step 1/2: only records intent. The current admin keeps full control until
/// `accept_admin` is signed by `new_admin` — protects against a mistyped pubkey
/// permanently locking the protocol out of its own admin.
pub fn transfer_admin_handler(ctx: Context<AdminConfig>, new_admin: Pubkey) -> Result<()> {
    ctx.accounts.config.pending_admin = new_admin;
    emit!(AdminTransferProposed { admin: ctx.accounts.admin.key(), pending_admin: new_admin });
    Ok(())
}

#[derive(Accounts)]
pub struct AcceptAdmin<'info> {
    pub pending_admin: Signer<'info>,
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        constraint = config.pending_admin == pending_admin.key() @ StakingError::Unauthorized,
    )]
    pub config: Account<'info, Config>,
}

/// Step 2/2: `pending_admin` claims the role. Resets `pending_admin` so it
/// cannot be replayed.
pub fn accept_admin_handler(ctx: Context<AcceptAdmin>) -> Result<()> {
    let old = ctx.accounts.config.admin;
    let new_admin = ctx.accounts.pending_admin.key();
    ctx.accounts.config.admin = new_admin;
    ctx.accounts.config.pending_admin = Pubkey::default();
    emit!(AdminTransferred { old_admin: old, new_admin });
    Ok(())
}

// --------------------------------------------------------------------------
// set_emission(pool) — settle at OLD rate, then write. Config-admin gated.
// --------------------------------------------------------------------------

#[derive(Accounts)]
pub struct SetEmission<'info> {
    pub admin: Signer<'info>,
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = admin @ StakingError::Unauthorized,
    )]
    pub config: Account<'info, Config>,
    #[account(
        mut,
        seeds = [POOL_SEED, config.key().as_ref(), pool.staked_mint.as_ref(), pool.reward_mint.as_ref()],
        bump = pool.bump,
        constraint = pool.config == config.key() @ StakingError::PoolConfigMismatch,
    )]
    pub pool: Account<'info, Pool>,
}

pub fn set_emission_handler(ctx: Context<SetEmission>, reward_per_sec: u64, end_time: i64) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    require!(
        end_time == 0 || (end_time > now && end_time > ctx.accounts.pool.start_time),
        StakingError::InvalidTimeRange
    );
    // Settle history at the OLD rate BEFORE writing the new one.
    reward::update_pool(&mut ctx.accounts.pool, now)?;
    let p = &mut ctx.accounts.pool;
    p.reward_per_sec = reward_per_sec;
    p.end_time = end_time;
    emit!(EmissionSet { pool: p.key(), reward_per_sec, end_time });
    Ok(())
}

// --------------------------------------------------------------------------
// withdraw_surplus(pool) — pay out only balance above outstanding liability.
//   amount <= reward_vault.balance - (total_emitted - total_claimed)
// --------------------------------------------------------------------------

#[derive(Accounts)]
pub struct WithdrawSurplus<'info> {
    pub admin: Signer<'info>,
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = admin @ StakingError::Unauthorized,
    )]
    pub config: Account<'info, Config>,
    #[account(
        mut,
        seeds = [POOL_SEED, config.key().as_ref(), pool.staked_mint.as_ref(), pool.reward_mint.as_ref()],
        bump = pool.bump,
        constraint = pool.config == config.key() @ StakingError::PoolConfigMismatch,
    )]
    pub pool: Account<'info, Pool>,

    #[account(mut, address = pool.reward_mint)]
    pub reward_mint: InterfaceAccount<'info, Mint>,
    #[account(mut, address = pool.reward_vault)]
    pub reward_vault: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = reward_mint)]
    pub destination: InterfaceAccount<'info, TokenAccount>,

    pub reward_token_program: Interface<'info, TokenInterface>,
}

pub fn withdraw_surplus_handler(ctx: Context<WithdrawSurplus>, amount: u64) -> Result<()> {
    require!(amount > 0, StakingError::AmountZero);
    let now = Clock::get()?.unix_timestamp;
    reward::update_pool(&mut ctx.accounts.pool, now)?;

    let outstanding = ctx
        .accounts
        .pool
        .total_emitted
        .checked_sub(ctx.accounts.pool.total_claimed as u128)
        .ok_or(StakingError::MathOverflow)?;
    let balance = ctx.accounts.reward_vault.amount as u128;
    let surplus = balance.saturating_sub(outstanding);
    require!((amount as u128) <= surplus, StakingError::NothingToWithdraw);

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
                to: ctx.accounts.destination.to_account_info(),
                authority: ctx.accounts.pool.to_account_info(),
            },
            signer,
        ),
        amount,
        ctx.accounts.reward_mint.decimals,
    )?;
    ctx.accounts.reward_vault.reload()?;

    emit!(SurplusWithdrawn {
        pool: ctx.accounts.pool.key(),
        admin: ctx.accounts.admin.key(),
        amount,
        reward_vault_balance: ctx.accounts.reward_vault.amount,
    });
    Ok(())
}
