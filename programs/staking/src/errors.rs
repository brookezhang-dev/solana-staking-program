//! Error codes. See design doc §10.

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
}
