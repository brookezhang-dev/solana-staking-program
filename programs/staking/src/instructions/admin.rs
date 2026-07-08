//! set_emission_params (v3): re-anchoring. ① settle at OLD params → ② write new
//! params → ③ rate_anchor_time = now. Prevents new curve re-applying to history. §1.3.

use crate::constants::*;
use crate::errors::StakingError;
use crate::events::EmissionParamsUpdatedEvent;
use crate::instructions::reward;
use crate::state::Config;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct SetEmissionParams<'info> {
    pub admin: Signer<'info>,
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = admin @ StakingError::Unauthorized,
    )]
    pub config: Account<'info, Config>,
}

pub fn handler(
    ctx: Context<SetEmissionParams>,
    initial_rate: u64,
    decay_per_sec: u64,
    min_rate: u64,
    end_time: i64,
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    require!(min_rate <= initial_rate, StakingError::InvalidEmissionParams);
    require!(
        end_time == 0 || (end_time > now && end_time > ctx.accounts.config.start_time),
        StakingError::InvalidEndTime
    );

    // ① settle history at OLD params.
    reward::update_pool(&mut ctx.accounts.config, now)?;

    // ② write new params, ③ re-anchor the decay curve to now.
    let c = &mut ctx.accounts.config;
    c.initial_rate = initial_rate;
    c.decay_per_sec = decay_per_sec;
    c.min_rate = min_rate;
    c.end_time = end_time;
    c.rate_anchor_time = now;

    emit!(EmissionParamsUpdatedEvent {
        admin: ctx.accounts.admin.key(),
        initial_rate,
        decay_per_sec,
        min_rate,
        end_time,
        rate_anchor_time: now,
    });
    Ok(())
}
