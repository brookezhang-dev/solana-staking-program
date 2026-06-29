//! Program-wide constants. See design doc §5.3.

/// Accumulator precision for `acc_reward_per_share` (1e12).
/// Big enough to keep per-share reward precise at 9-decimal tokens while
/// staying inside u128 for the `amount * acc_reward_per_share` product.
pub const ACC_PRECISION: u128 = 1_000_000_000_000;

/// PDA seeds.
pub const CONFIG_SEED: &[u8] = b"config";
pub const VAULT_SEED: &[u8] = b"vault";
pub const USER_SEED: &[u8] = b"user";

/// Account space helpers (excluding the 8-byte Anchor discriminator).
pub const CONFIG_SPACE: usize = 226; // see state::Config layout
pub const USER_INFO_SPACE: usize = 57; // see state::UserInfo layout

/// Devnet faucet per-call cap: 1000 $BEEF at 9 decimals.
pub const FAUCET_MAX: u64 = 1_000_000_000_000;
