//! initialize_config (v3.x): create the protocol singleton Config. Sets admin =
//! signer, paused = false, pool_count = 0. Pools are created later via create_pool.

use crate::constants::*;
use crate::state::Config;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct InitializeConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        init,
        payer = admin,
        space = 8 + Config::SPACE,
        seeds = [CONFIG_SEED],
        bump
    )]
    pub config: Account<'info, Config>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<InitializeConfig>) -> Result<()> {
    let c = &mut ctx.accounts.config;
    c.admin = ctx.accounts.admin.key();
    c.pool_count = 0;
    c.paused = false;
    c.bump = ctx.bumps.config;
    c.reserved = [0u8; 64];
    Ok(())
}
