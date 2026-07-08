//! Events for frontend / indexing / water-level monitoring (v3).

use anchor_lang::prelude::*;

#[event]
pub struct StakeEvent {
    pub user: Pubkey,
    pub credited: u64, // actual principal credited (balance-diff; fee-token safe)
    pub total: u64,    // user_info.amount after
}

#[event]
pub struct UnstakeEvent {
    pub user: Pubkey,
    pub amount: u64,
    pub remaining: u64,
}

#[event]
pub struct ClaimEvent {
    pub user: Pubkey,
    pub amount: u64, // pending_unclaimed paid out (nominal)
}

/// Emitted by fund_rewards — water-level bot tracks solvency from this + ClaimEvent.
#[event]
pub struct FundEvent {
    pub funder: Pubkey,
    pub credited: u64,            // actual amount received by reward vault
    pub reward_vault_balance: u64, // vault balance after
}

/// Emitted by set_emission_params — records the re-anchoring.
#[event]
pub struct EmissionParamsUpdatedEvent {
    pub admin: Pubkey,
    pub initial_rate: u64,
    pub decay_per_sec: u64,
    pub min_rate: u64,
    pub end_time: i64,
    pub rate_anchor_time: i64, // = now (new curve origin)
}
