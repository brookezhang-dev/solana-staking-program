//! Account state (v3.x — Config/Pool two-tier). Mirrors Raydium AmmConfig→Pool /
//! Orca WhirlpoolsConfig→Whirlpool: one protocol Config, N on-demand Pools.

use anchor_lang::prelude::*;

/// Tier 1 — protocol singleton. PDA: seeds = [CONFIG_SEED].
/// Holds ONLY protocol-level state; all pool accounting lives in `Pool`.
#[account]
pub struct Config {
    pub admin: Pubkey,         // 32  protocol admin (create_pool / set_pause / set_emission)
    pub pending_admin: Pubkey, // 32  set by transfer_admin, must accept_admin to take effect;
                               //     Pubkey::default() means no transfer in progress
    pub pool_count: u16,       //  2  bookkeeping only (pool identity is the mint pair)
    pub paused: bool,          //  1  global emergency stop; every user ix checks it
    pub bump: u8,              //  1
    pub reserved: [u8; 32],    // 32
}

impl Config {
    // 32 + 32 + 2 + 1 + 1 + 32 = 100 ; account = 8 + 100 = 108
    pub const SPACE: usize = 100;
}

/// Tier 2 — one per (staked_mint, reward_mint) pair.
/// PDA: seeds = [POOL_SEED, config, staked_mint, reward_mint]. The Pool PDA is the
/// authority of its two vaults AND the $STAKE receipt mint (per-pool dual role).
#[account]
pub struct Pool {
    pub config: Pubkey,             // 32
    pub staked_mint: Pubkey,        // 32  input token (e.g. $BEEF)
    pub reward_mint: Pubkey,        // 32  reward token (e.g. $MILK; external)
    pub stake_receipt_mint: Pubkey, // 32  this pool's $STAKE (Token-2022 TransferHook)
    pub staked_vault: Pubkey,       // 32
    pub reward_vault: Pubkey,       // 32
    pub acc_reward_per_share: u128, // 16  scaled by ACC_PRECISION
    pub last_reward_time: i64,      //  8
    pub start_time: i64,            //  8
    pub end_time: i64,              //  8  0 = uncapped
    pub reward_per_sec: u64,        //  8  constant emission rate
    pub total_emitted: u128,        // 16  only ++ in update_pool (surplus liability)
    pub total_staked: u64,          //  8  == stake_receipt_mint.supply
    pub total_claimed: u64,         //  8
    pub bump: u8,                   //  1
    pub reserved: [u8; 64],         // 64
}

impl Pool {
    // 32*6 + (16+8+8+8+8+16+8+8) + 1 + 64 = 192 + 80 + 1 + 64 = 337 ; account = 8 + 337 = 345
    pub const SPACE: usize = 337;
}

/// Per-$STAKE-token-account reward state. PDA: seeds = [USER_SEED, pool, token_account].
/// The token-account BALANCE is the share authority; this struct holds only MasterChef
/// bookkeeping. Keyed by token account so the transfer hook can settle both parties.
#[account]
pub struct UserInfo {
    pub token_account: Pubkey,  // 32  the $STAKE ATA this state tracks
    pub reward_debt: u128,      // 16
    pub pending_unclaimed: u64, //  8
    pub bump: u8,               //  1
    pub reserved: [u8; 32],     // 32
}

impl UserInfo {
    // 32 + 16 + 8 + 1 + 32 = 89 ; account = 8 + 89 = 97
    pub const SPACE: usize = 89;
}
