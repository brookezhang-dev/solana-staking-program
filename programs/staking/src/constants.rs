//! Program-wide constants (v3.x — Config/Pool two-tier).

/// Accumulator precision for `acc_reward_per_share` (1e12). COMPILE-TIME CONSTANT.
pub const ACC_PRECISION: u128 = 1_000_000_000_000;

/// PDA seeds.
pub const CONFIG_SEED: &[u8] = b"config";
pub const POOL_SEED: &[u8] = b"pool";
pub const STAKED_VAULT_SEED: &[u8] = b"staked_vault";
pub const REWARD_VAULT_SEED: &[u8] = b"reward_vault";
/// UserInfo: seeds = [USER_SEED, pool, stake_token_account].
pub const USER_SEED: &[u8] = b"user_info";
/// ExtraAccountMetaList PDA seed prefix (spl-transfer-hook-interface convention).
pub const EXTRA_META_SEED: &[u8] = b"extra-account-metas";

/// Devnet faucet per-call cap: 1000 tokens at 9 decimals.
pub const FAUCET_MAX: u64 = 1_000_000_000_000;
