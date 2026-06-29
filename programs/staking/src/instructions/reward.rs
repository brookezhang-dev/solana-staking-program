//! Reward math helpers: update_pool + emission_between. See design doc §7.
//! All arithmetic uses u128 + checked_* — never silently wraps.

use crate::errors::StakingError;
use crate::state::Config;
use anchor_lang::prelude::*;

/// Settle the pool up to `now`. O(1), independent of user count (design doc §7.2).
/// MUST be the first call in any instruction that mutates total_staked /
/// acc_reward_per_share.
pub fn update_pool(config: &mut Config, now: i64) -> Result<()> {
    if now <= config.last_reward_time {
        return Ok(());
    }
    if config.total_staked == 0 {
        // No stakers: accrue nothing, just advance time.
        config.last_reward_time = now;
        return Ok(());
    }

    let reward = emission_between(config, config.last_reward_time, now)?;
    let delta = (reward as u128)
        .checked_mul(crate::constants::ACC_PRECISION)
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

/// Pending reward for a given share/debt at the current acc_reward_per_share.
/// pending = amount * acc_reward_per_share / ACC_PRECISION - reward_debt
pub fn pending_reward(amount: u64, acc_reward_per_share: u128, reward_debt: u128) -> Result<u64> {
    let accrued = (amount as u128)
        .checked_mul(acc_reward_per_share)
        .ok_or(StakingError::MathOverflow)?
        .checked_div(crate::constants::ACC_PRECISION)
        .ok_or(StakingError::MathOverflow)?;
    let pending = accrued.saturating_sub(reward_debt);
    u64::try_from(pending).map_err(|_| StakingError::MathOverflow.into())
}

/// New reward_debt baseline after a share change.
pub fn reward_debt_for(amount: u64, acc_reward_per_share: u128) -> Result<u128> {
    (amount as u128)
        .checked_mul(acc_reward_per_share)
        .ok_or(StakingError::MathOverflow)?
        .checked_div(crate::constants::ACC_PRECISION)
        .ok_or(StakingError::MathOverflow.into())
}

/// Total emission over [a, b] under linear decay with a floor (design doc §7.4).
///
///   r(t) = max(initial_rate - decay_per_sec * (t - start_time), min_rate)
///
/// O(1) closed form: split [a, b] at the floor-crossing second `t_floor` into a
/// decaying trapezoid segment and a constant `min_rate` segment. All integer
/// (u128 + checked_*). Sub-second truncation around the crossing under-counts by
/// < decay_per_sec (never over-mints) — accepted MasterChef dust.
pub fn emission_between(config: &Config, a: i64, b: i64) -> Result<u64> {
    if b <= a {
        return Ok(0);
    }

    let r0 = config.initial_rate as u128;
    let dt = (b - a) as u128;

    // --- Constant-rate path (decay disabled) ---
    if config.decay_per_sec == 0 {
        return u64::try_from(r0.checked_mul(dt).ok_or(StakingError::MathOverflow)?)
            .map_err(|_| StakingError::MathOverflow.into());
    }

    // --- Linear-decay path (step 3.7) ---
    // Nothing accrues before emission start; clamp the lower bound.
    let start = config.start_time;
    let a = a.max(start);
    if b <= a {
        return Ok(0);
    }

    let k = config.decay_per_sec as u128;
    let floor = config.min_rate as u128;

    // Total drop available before hitting the floor. If 0, the rate is the floor.
    let g = r0.saturating_sub(floor);
    if g == 0 {
        let dur = (b - a) as u128;
        return u64::try_from(floor.checked_mul(dur).ok_or(StakingError::MathOverflow)?)
            .map_err(|_| StakingError::MathOverflow.into());
    }

    // Integer second (relative to start) at which the rate reaches the floor.
    let s_floor = (g / k) as i64;
    let t_floor = start
        .checked_add(s_floor)
        .ok_or(StakingError::MathOverflow)?;

    let decay_end = b.min(t_floor); // decay segment: [a, decay_end]
    let floor_start = a.max(t_floor); // floor segment: [floor_start, b]
    let mut total: u128 = 0;

    // Decaying trapezoid: integral of (r0 - k*(t-start)) over [a, decay_end]
    //   = (2*r0*(E-A) - k*(E^2 - A^2)) / 2,  with A,E measured from start.
    // The WHOLE area is floored in a single division so truncation always rounds
    // DOWN (never over-mints). The numerator is >= 0 since the integrand >= floor.
    if decay_end > a {
        let a_rel = (a - start) as u128;
        let e_rel = (decay_end - start) as u128;
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

    // Constant floor segment: min_rate over [floor_start, b].
    if b > floor_start {
        let dur = (b - floor_start) as u128;
        let area = floor.checked_mul(dur).ok_or(StakingError::MathOverflow)?;
        total = total.checked_add(area).ok_or(StakingError::MathOverflow)?;
    }

    u64::try_from(total).map_err(|_| StakingError::MathOverflow.into())
}
