// v3 devnet mints (from scripts/setup-devnet.ts output).
export const BEEF_MINT = "G6wz3Bm1hRFg8PGiZzrQAZgh3C7iPReS2xb11PsktYfF";
export const STAKE_MINT = "7cD7PsZJT8aoUyiKHtM9RtD7P1RtFHDwWs8ktmhkH6Gp"; // Token-2022 NonTransferable
export const REWARD_MINT = "3dK1AfnsXFnp52HpC3vmrb7bvxRH2GEimtE34cGp7tgG";

export const CLUSTER = "devnet";
export const DECIMALS = 9;
export const ACC_PRECISION = 1_000_000_000_000n; // 1e12, must match constants.rs

export const solscanTx = (sig: string) =>
  `https://solscan.io/tx/${sig}?cluster=${CLUSTER}`;
