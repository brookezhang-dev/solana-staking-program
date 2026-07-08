//! Reward math (v3): update_pool + emission_between. See v3 执行计划 §1.4.
//! MasterChef O(1). All arithmetic u128 + checked_*; area floored (never over-accrues).
//! Emission is measured against `rate_anchor_time` (re-anchoring) and capped at
//! `end_time` (0 = uncapped).

use crate::constants::ACC_PRECISION;
use crate::errors::StakingError;
use crate::state::Config;
use anchor_lang::prelude::*;

/// Settle the pool up to `now`. First call in every share-mutating instruction.
pub fn update_pool(config: &mut Config, now: i64) -> Result<()> {
    if now <= config.last_reward_time {
        return Ok(());
    }
    if config.total_staked == 0 {
        config.last_reward_time = now; // no stakers: accrue nothing, advance time
        return Ok(());
    }
    let reward = emission_between(config, config.last_reward_time, now)?;
    let delta = (reward as u128)
        .checked_mul(ACC_PRECISION)
        .ok_or(StakingError::MathOverflow)?
        .checked_div(config.total_staked as u128)
        .ok_or(StakingError::MathOverflow)?;
    config.acc_reward_per_share = config
        .acc_reward_per_share
        .checked_add(delta)
        .ok_or(StakingError::MathOverflow)?;
    config.last_reward_time = now;
    Ok(())
}

/// pending = amount * acc_reward_per_share / ACC_PRECISION - reward_debt (saturating).
pub fn pending_reward(amount: u64, acc_reward_per_share: u128, reward_debt: u128) -> Result<u64> {
    let accrued = (amount as u128)
        .checked_mul(acc_reward_per_share)
        .ok_or(StakingError::MathOverflow)?
        .checked_div(ACC_PRECISION)
        .ok_or(StakingError::MathOverflow)?;
    u64::try_from(accrued.saturating_sub(reward_debt)).map_err(|_| StakingError::MathOverflow.into())
}

/// New reward_debt baseline after a share change.
pub fn reward_debt_for(amount: u64, acc_reward_per_share: u128) -> Result<u128> {
    (amount as u128)
        .checked_mul(acc_reward_per_share)
        .ok_or(StakingError::MathOverflow)?
        .checked_div(ACC_PRECISION)
        .ok_or(StakingError::MathOverflow.into())
}

/// Total emission over [a, b].
///   r(t) = max(initial_rate − decay_per_sec·(t − rate_anchor_time), min_rate)
/// Clamped to [start_time, end_time]; end_time == 0 means uncapped.
pub fn emission_between(config: &Config, a: i64, b: i64) -> Result<u64> {
    // Clamp window: nothing before start_time, nothing after end_time.
    let mut a = a.max(config.start_time);
    let mut b = b;
    if config.end_time > 0 {
        b = b.min(config.end_time);
    }
    // Decay curve is anchored at rate_anchor_time; by construction a >= anchor.
    a = a.max(config.rate_anchor_time);
    if b <= a {
        return Ok(0);
    }

    let r0 = config.initial_rate as u128;

    // Constant-rate path.
    if config.decay_per_sec == 0 {
        let dt = (b - a) as u128;
        return u64::try_from(r0.checked_mul(dt).ok_or(StakingError::MathOverflow)?)
            .map_err(|_| StakingError::MathOverflow.into());
    }

    let k = config.decay_per_sec as u128;
    let floor = config.min_rate as u128;
    let g = r0.saturating_sub(floor);
    if g == 0 {
        let dt = (b - a) as u128;
        return u64::try_from(floor.checked_mul(dt).ok_or(StakingError::MathOverflow)?)
            .map_err(|_| StakingError::MathOverflow.into());
    }

    // Floor-crossing second, relative to the anchor.
    let s_floor = (g / k) as i64;
    let t_floor = config
        .rate_anchor_time
        .checked_add(s_floor)
        .ok_or(StakingError::MathOverflow)?;

    let decay_end = b.min(t_floor);
    let floor_start = a.max(t_floor);
    let mut total: u128 = 0;

    // Decaying trapezoid over [a, decay_end], relative to rate_anchor_time.
    // area = (2*r0*span - k*(E^2 - A^2)) / 2, floored (never over-accrues).
    if decay_end > a {
        let a_rel = (a - config.rate_anchor_time) as u128;
        let e_rel = (decay_end - config.rate_anchor_time) as u128;
        let span = e_rel - a_rel;
        let two_rect = r0
            .checked_mul(span)
            .ok_or(StakingError::MathOverflow)?
            .checked_mul(2)
            .ok_or(StakingError::MathOverflow)?;
        let sq = e_rel
            .checked_mul(e_rel)
            .ok_or(StakingError::MathOverflow)?
            .checked_sub(a_rel.checked_mul(a_rel).ok_or(StakingError::MathOverflow)?)
            .ok_or(StakingError::MathOverflow)?;
        let decline = k.checked_mul(sq).ok_or(StakingError::MathOverflow)?;
        let area = two_rect
            .checked_sub(decline)
            .ok_or(StakingError::MathOverflow)?
            / 2;
        total = total.checked_add(area).ok_or(StakingError::MathOverflow)?;
    }

    // Constant floor segment over [floor_start, b].
    if b > floor_start {
        let dur = (b - floor_start) as u128;
        let area = floor.checked_mul(dur).ok_or(StakingError::MathOverflow)?;
        total = total.checked_add(area).ok_or(StakingError::MathOverflow)?;
    }

    u64::try_from(total).map_err(|_| StakingError::MathOverflow.into())
}
