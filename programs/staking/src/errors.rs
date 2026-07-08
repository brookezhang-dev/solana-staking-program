//! Error codes (v3).

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
    #[msg("pool not initialized")]
    NotInitialized,
    #[msg("faucet amount exceeds the per-call cap")]
    FaucetTooMuch,

    // ---- v3 additions ----
    #[msg("reward vault balance insufficient to pay pending")]
    RewardVaultInsufficient,
    #[msg("emission params invalid: require min_rate <= initial_rate")]
    InvalidEmissionParams,
    #[msg("end_time invalid: must be 0 or > now and > start_time")]
    InvalidEndTime,
    #[msg("stake mint must be a Token-2022 NonTransferable mint")]
    StakeMintNotNonTransferable,
    #[msg("token has an unsupported extension (transfer fee / transfer hook)")]
    UnsupportedTokenExtension,
    #[msg("credited amount is zero after transfer (all consumed by fee?)")]
    ZeroCredited,
}
