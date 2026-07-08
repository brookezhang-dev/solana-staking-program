//! Program-wide constants (v3). See v3 执行计划 §1.

/// Accumulator precision for `acc_reward_per_share` (1e12).
/// COMPILE-TIME CONSTANT — never runtime-configurable: changing it would corrupt
/// the meaning of existing acc_reward_per_share / reward_debt (§9.5).
pub const ACC_PRECISION: u128 = 1_000_000_000_000;

/// PDA seeds.
pub const CONFIG_SEED: &[u8] = b"config";
pub const VAULT_SEED: &[u8] = b"vault";
pub const REWARD_VAULT_SEED: &[u8] = b"reward_vault";
pub const USER_SEED: &[u8] = b"user";

/// Devnet faucet per-call cap: 1000 $BEEF at 9 decimals.
/// Only compiled in under the `devnet-faucet` feature (main-net build excludes it).
pub const FAUCET_MAX: u64 = 1_000_000_000_000;
