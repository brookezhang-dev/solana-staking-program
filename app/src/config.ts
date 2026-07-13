// v3.x devnet mints (from scripts/setup-devnet.ts output, Config/Pool two-tier).
export const BEEF_MINT = "6wtE2ms7wbXPcpWvrauyismUZeSRWBYNWej7ABo1YwRW";
export const STAKE_MINT = "CkJryaVo6jMcsNobK98zYXCCC18Pftg3UrJGSQAPTbFX"; // Token-2022 TransferHook (transferable)
export const REWARD_MINT = "Bydv95RTYz5aVQbRjY9DksNL7JNTWTqQCUPgEmZtSnm3";

export const CLUSTER = "devnet";
export const DECIMALS = 9;
export const ACC_PRECISION = 1_000_000_000_000n; // 1e12, must match constants.rs

export const solscanTx = (sig: string) =>
  `https://solscan.io/tx/${sig}?cluster=${CLUSTER}`;
