//! Reward math (v3.x): update_pool + emission_between, operating on `Pool`.
//! Constant `reward_per_sec` clamped to [start_time, end_time] (0 = uncapped).
//! O(1), u128 + checked_*. `total_emitted` accrues ONLY here (only while staked>0).

use crate::constants::ACC_PRECISION;
use crate::errors::StakingError;
use crate::state::Pool;
use anchor_lang::prelude::*;

/// Settle the pool up to `now`. First call in every share-mutating instruction.
pub fn update_pool(pool: &mut Pool, now: i64) -> Result<()> {
    if now <= pool.last_reward_time {
        return Ok(());
    }
    if pool.total_staked == 0 {
        pool.last_reward_time = now; // no stakers: accrue nothing; total_emitted unchanged
        return Ok(());
    }
    let reward = emission_between(pool, pool.last_reward_time, now)?;
    let delta = (reward as u128)
        .checked_mul(ACC_PRECISION)
        .ok_or(StakingError::MathOverflow)?
        .checked_div(pool.total_staked as u128)
        .ok_or(StakingError::MathOverflow)?;
    pool.acc_reward_per_share = pool
        .acc_reward_per_share
        .checked_add(delta)
        .ok_or(StakingError::MathOverflow)?;
    pool.total_emitted = pool
        .total_emitted
        .checked_add(reward as u128)
        .ok_or(StakingError::MathOverflow)?;
    pool.last_reward_time = now;
    Ok(())
}

/// pending = amount * acc / ACC_PRECISION - reward_debt (saturating).
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

/// Total emission over [a, b] at the pool's constant rate, clamped to [start,end].
pub fn emission_between(pool: &Pool, a: i64, b: i64) -> Result<u64> {
    let a = a.max(pool.start_time);
    let b = if pool.end_time > 0 { b.min(pool.end_time) } else { b };
    if b <= a {
        return Ok(0);
    }
    let dt = (b - a) as u128;
    let reward = (pool.reward_per_sec as u128)
        .checked_mul(dt)
        .ok_or(StakingError::MathOverflow)?;
    u64::try_from(reward).map_err(|_| StakingError::MathOverflow.into())
}
