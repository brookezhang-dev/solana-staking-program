//! Account state definitions. See design doc §5.2.

use anchor_lang::prelude::*;

/// Global config + reward-pool state. PDA: seeds = [CONFIG_SEED].
/// Also acts as the vault authority and the $STAKE / $MILK mint authority.
#[account]
pub struct Config {
    pub admin: Pubkey,              // 32  admin (can tune emission params)
    pub beef_mint: Pubkey,         // 32  input token mint
    pub stake_mint: Pubkey,        // 32  receipt token mint
    pub milk_mint: Pubkey,         // 32  reward token mint
    pub vault: Pubkey,             // 32  $BEEF vault token account
    pub total_staked: u64,         //  8  current total staked
    pub acc_reward_per_share: u128,// 16  accumulated reward per share (scaled by ACC_PRECISION)
    pub last_reward_time: i64,     //  8  last pool-settlement unix ts
    pub start_time: i64,           //  8  emission start (for decay calc)
    pub initial_rate: u64,         //  8  initial emission rate ($MILK base units / sec)
    pub decay_per_sec: u64,        //  8  linear decay per second (0 = constant)
    pub min_rate: u64,             //  8  emission rate floor
    pub bump: u8,                  //  1  Config PDA bump
    pub vault_bump: u8,            //  1  Vault PDA bump
}
// data = 32*5 + 8 + 16 + 8*5 + 1 + 1 = 226 bytes; account = 8 + 226 = 234
//   (160 + 8 + 16 + 40 + 1 + 1 = 226)

impl Config {
    pub const SPACE: usize = 226;
}

/// Per-user state. PDA: seeds = [USER_SEED, user_pubkey].
/// `amount` is a REWARD-SHARE MIRROR only (normally == user's $STAKE balance).
/// The authoritative principal is the user's $STAKE balance (see design doc §3.3, §18).
#[account]
pub struct UserInfo {
    pub owner: Pubkey,     // 32  owning user
    pub amount: u64,       //  8  reward-share mirror
    pub reward_debt: u128, // 16  reward debt baseline
    pub bump: u8,          //  1
}
// data = 32 + 8 + 16 + 1 = 57 bytes; account = 8 + 57 = 65

impl UserInfo {
    pub const SPACE: usize = 57;
}
