//! Error codes (v3.x).

use anchor_lang::prelude::*;

#[error_code]
pub enum StakingError {
    #[msg("amount must be greater than zero")]
    AmountZero,
    #[msg("insufficient staked balance")]
    InsufficientStake,
    #[msg("nothing to claim")]
    NothingToClaim,
    #[msg("math overflow")]
    MathOverflow,
    #[msg("unauthorized")]
    Unauthorized,
    #[msg("invalid mint")]
    InvalidMint,
    #[msg("faucet amount exceeds the per-call cap")]
    FaucetTooMuch,

    // ---- reward vault / emission ----
    #[msg("reward vault balance insufficient to pay pending")]
    RewardVaultInsufficient,
    #[msg("stake mint must carry a TransferHook extension pointing at this program")]
    StakeMintHookMismatch,
    #[msg("token has an unsupported extension (transfer hook on staked/reward)")]
    UnsupportedTokenExtension,
    #[msg("invalid time range: end_time must be 0 or > now and > start_time")]
    InvalidTimeRange,

    // ---- transfer hook ----
    #[msg("hook execute must be invoked by Token-2022 during a transfer")]
    NotTransferring,
    #[msg("destination $STAKE account has no UserInfo — call register first")]
    DestinationNotRegistered,

    // ---- lifecycle / two-tier ----
    #[msg("protocol is paused")]
    Paused,
    #[msg("nothing to withdraw: amount exceeds surplus")]
    NothingToWithdraw,
    #[msg("user_info not empty: require balance == 0 && pending_unclaimed == 0")]
    AccountNotEmpty,
    #[msg("pool config mismatch")]
    PoolConfigMismatch,
}
