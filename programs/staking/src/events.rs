//! Events (v3.x — Config/Pool two-tier).

use anchor_lang::prelude::*;

#[event]
pub struct PoolCreated {
    pub pool: Pubkey,
    pub staked_mint: Pubkey,
    pub reward_mint: Pubkey,
    pub stake_receipt_mint: Pubkey,
    pub reward_per_sec: u64,
    pub start_time: i64,
    pub end_time: i64,
}

#[event]
pub struct StakeEvent {
    pub pool: Pubkey,
    pub user: Pubkey,
    pub credited: u64, // actual principal credited (balance-diff)
    pub total: u64,    // $STAKE balance after
}

#[event]
pub struct UnstakeEvent {
    pub pool: Pubkey,
    pub user: Pubkey,
    pub amount: u64,
    pub remaining: u64,
}

#[event]
pub struct ClaimEvent {
    pub pool: Pubkey,
    pub user: Pubkey,
    pub amount: u64,
}

#[event]
pub struct FundEvent {
    pub pool: Pubkey,
    pub funder: Pubkey,
    pub credited: u64,
    pub reward_vault_balance: u64,
}

#[event]
pub struct EmissionSet {
    pub pool: Pubkey,
    pub reward_per_sec: u64,
    pub end_time: i64,
}

#[event]
pub struct SurplusWithdrawn {
    pub pool: Pubkey,
    pub admin: Pubkey,
    pub amount: u64,
    pub reward_vault_balance: u64,
}

#[event]
pub struct UserInfoClosed {
    pub pool: Pubkey,
    pub user: Pubkey,
}

#[event]
pub struct PauseSet {
    pub admin: Pubkey,
    pub paused: bool,
}

#[event]
pub struct AdminTransferProposed {
    pub admin: Pubkey,
    pub pending_admin: Pubkey,
}

#[event]
pub struct AdminTransferred {
    pub old_admin: Pubkey,
    pub new_admin: Pubkey,
}
