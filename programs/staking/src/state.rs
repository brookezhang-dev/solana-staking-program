//! Account state definitions (v3). See v3 执行计划 §1.2.

use anchor_lang::prelude::*;

/// Global config + reward-pool state. PDA: seeds = [CONFIG_SEED].
/// Also acts as authority of both vaults AND the $STAKE mint authority.
/// $MILK/reward mint authority is NOT held by the program (prefunded-vault model).
#[account]
pub struct Config {
    pub admin: Pubkey,               // 32  admin (emission params only)
    pub beef_mint: Pubkey,          // 32  input token mint (classic or 2022)
    pub stake_mint: Pubkey,         // 32  receipt mint (Token-2022 + NonTransferable)
    pub reward_mint: Pubkey,        // 32  reward mint (external; any SPL)
    pub vault: Pubkey,              // 32  $BEEF principal vault
    pub reward_vault: Pubkey,       // 32  reward vault (prefunded)
    pub total_staked: u64,          //  8
    pub acc_reward_per_share: u128, // 16  scaled by ACC_PRECISION
    pub last_reward_time: i64,      //  8  last pool-settlement ts
    pub start_time: i64,            //  8  emission start (fixed; no-accrual clamp)
    pub rate_anchor_time: i64,      //  8  decay curve anchor; reset to now on set_emission_params
    pub end_time: i64,              //  8  emission end; 0 = uncapped
    pub initial_rate: u64,          //  8  rate at anchor (base units / sec)
    pub decay_per_sec: u64,         //  8  linear decay per second (0 = constant)
    pub min_rate: u64,              //  8  rate floor
    pub total_claimed: u64,         //  8  cumulative reward paid out (water-level bot)
    pub bump: u8,                   //  1
    pub vault_bump: u8,             //  1
    pub reward_vault_bump: u8,      //  1
    pub reserved: [u8; 64],         // 64  upgrade headroom — do NOT remove
}

impl Config {
    // 32*6 + (8*9 + 16) + 3 + 64 = 192 + 88 + 3 + 64 = 347
    pub const SPACE: usize = 347;
}

/// Per-user state. PDA: seeds = [USER_SEED, user_pubkey].
/// `amount` is the SOLE principal ledger (redeem limit AND reward share both use it).
/// With NonTransferable $STAKE, `amount ≡ 用户 $STAKE 余额` holds physically.
#[account]
pub struct UserInfo {
    pub owner: Pubkey,          // 32
    pub amount: u64,            //  8  ★ sole principal ledger
    pub reward_debt: u128,      // 16
    pub pending_unclaimed: u64, //  8  settled-but-unclaimed reward (strategy B)
    pub bump: u8,               //  1
    pub reserved: [u8; 16],     // 16  upgrade headroom
}

impl UserInfo {
    // 32 + 8 + 16 + 8 + 1 + 16 = 81
    pub const SPACE: usize = 81;
}
