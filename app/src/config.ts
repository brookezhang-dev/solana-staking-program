// Fill these in after creating the mints on devnet.
// `npx ts-node scripts/setup-devnet.ts` (from repo root) prints all three.
// Leave as-is and the UI will show a "configure mints" banner.
export const BEEF_MINT = "23QJ4xVcUXG4F9FGhfjmnqx8XuaFVFLCL4AujnSdEF1L";
export const STAKE_MINT = "8w9epdLaGSsd7zHZawvZHiefK41Wk7Zvb8sceWshiARG";
export const MILK_MINT = "51ZCi7fvVpNw6H9pRXRxiaZejuiNWnKromhB2L6VVjvH";

export const CLUSTER = "devnet";
export const DECIMALS = 9;
export const ACC_PRECISION = 1_000_000_000_000n; // 1e12, must match constants.rs

// Solscan link for a signature on the configured cluster.
export const solscanTx = (sig: string) =>
  `https://solscan.io/tx/${sig}?cluster=${CLUSTER}`;
