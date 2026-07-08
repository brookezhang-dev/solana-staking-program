import {
  AnchorProvider, BN, Program, type Idl, type Wallet,
} from "@coral-xyz/anchor";
import { Connection, PublicKey, SystemProgram } from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID,
  getAssociatedTokenAddressSync, createAssociatedTokenAccountInstruction, getAccount,
} from "@solana/spl-token";
import { ACC_PRECISION, BEEF_MINT, STAKE_MINT, REWARD_MINT, DECIMALS } from "./config";

// Token programs per mint: $STAKE is Token-2022 (NonTransferable); $BEEF / reward classic.
export const STAKE_PROGRAM = TOKEN_2022_PROGRAM_ID;
export const CLASSIC_PROGRAM = TOKEN_PROGRAM_ID;

// ---- IDL loaded at runtime from /staking.json (copied by `npm run copy-idl`) ----
let idlCache: Idl | null = null;
export async function loadIdl(): Promise<Idl> {
  if (idlCache) return idlCache;
  const res = await fetch("/staking.json");
  if (!res.ok) throw new Error("staking.json not found — run `npm run copy-idl`");
  idlCache = (await res.json()) as Idl;
  return idlCache;
}
export async function getProgram(connection: Connection, wallet: Wallet) {
  const idl = await loadIdl();
  const provider = new AnchorProvider(connection, wallet, { commitment: "confirmed" });
  return new Program(idl, provider);
}

export const beefMint = () => new PublicKey(BEEF_MINT);
export const stakeMint = () => new PublicKey(STAKE_MINT);
export const rewardMint = () => new PublicKey(REWARD_MINT);

export function pdas(programId: PublicKey, owner?: PublicKey) {
  const [config] = PublicKey.findProgramAddressSync([Buffer.from("config")], programId);
  const [vault] = PublicKey.findProgramAddressSync([Buffer.from("vault")], programId);
  const [rewardVault] = PublicKey.findProgramAddressSync([Buffer.from("reward_vault")], programId);
  const userInfo = owner
    ? PublicKey.findProgramAddressSync([Buffer.from("user"), owner.toBuffer()], programId)[0]
    : undefined;
  return { config, vault, rewardVault, userInfo };
}

export const toBase = (uiAmount: string) => new BN(Math.round(parseFloat(uiAmount || "0") * 10 ** DECIMALS));
export const fromBase = (base: bigint) => Number(base) / 10 ** DECIMALS;

// Prepend an ATA-creation ix if missing. programId must match the mint's token program.
async function ensureAta(
  connection: Connection, owner: PublicKey, mint: PublicKey, payer: PublicKey,
  tokenProgram: PublicKey, ixs: any[]
) {
  const ata = getAssociatedTokenAddressSync(mint, owner, false, tokenProgram);
  const info = await connection.getAccountInfo(ata);
  if (!info) ixs.push(createAssociatedTokenAccountInstruction(payer, ata, owner, mint, tokenProgram));
  return ata;
}

export async function readBalance(connection: Connection, mint: PublicKey, owner: PublicKey, tokenProgram: PublicKey) {
  try {
    const ata = getAssociatedTokenAddressSync(mint, owner, false, tokenProgram);
    const acc = await getAccount(connection, ata, undefined, tokenProgram);
    return acc.amount;
  } catch {
    return 0n;
  }
}

// ---- Emission mirror (must match reward::emission_between: anchor base + end_time) ----
function emissionBetween(cfg: any, aLast: bigint, bLast: bigint): bigint {
  const r0 = BigInt(cfg.initialRate.toString());
  const k = BigInt(cfg.decayPerSec.toString());
  const floor = BigInt(cfg.minRate.toString());
  const start = BigInt(cfg.startTime.toString());
  const anchor = BigInt(cfg.rateAnchorTime.toString());
  const end = BigInt(cfg.endTime.toString());
  let a = aLast > start ? aLast : start;
  let b = bLast;
  if (end > 0n && b > end) b = end;
  if (a < anchor) a = anchor;
  if (b <= a) return 0n;
  if (k === 0n) return r0 * (b - a);
  const g = r0 > floor ? r0 - floor : 0n;
  if (g === 0n) return floor * (b - a);
  const tFloor = anchor + g / k;
  const decayEnd = b < tFloor ? b : tFloor;
  const floorStart = a > tFloor ? a : tFloor;
  let total = 0n;
  if (decayEnd > a) {
    const aRel = a - anchor, eRel = decayEnd - anchor;
    total += (r0 * (eRel - aRel) * 2n - k * (eRel * eRel - aRel * aRel)) / 2n;
  }
  if (b > floorStart) total += floor * (b - floorStart);
  return total;
}

// Claimable reward now = stored pending_unclaimed + freshly-accrued (est.).
export function estimatePending(cfg: any, userInfo: any): bigint {
  const now = BigInt(Math.floor(Date.now() / 1000));
  const total = BigInt(cfg.totalStaked.toString());
  let acc = BigInt(cfg.accRewardPerShare.toString());
  const last = BigInt(cfg.lastRewardTime.toString());
  if (total > 0n) acc += (emissionBetween(cfg, last, now) * ACC_PRECISION) / total;
  const amount = BigInt(userInfo.amount.toString());
  const debt = BigInt(userInfo.rewardDebt.toString());
  const accrued = (amount * acc) / ACC_PRECISION;
  const fresh = accrued > debt ? accrued - debt : 0n;
  return BigInt(userInfo.pendingUnclaimed.toString()) + fresh;
}

// ---- Actions ----
type Ctx = { program: Program; connection: Connection; owner: PublicKey };

export async function doStake({ program, connection, owner }: Ctx, amount: BN) {
  const { config, vault, userInfo } = pdas(program.programId, owner);
  const ixs: any[] = [];
  const userBeefAta = await ensureAta(connection, owner, beefMint(), owner, CLASSIC_PROGRAM, ixs);
  const userStakeAta = await ensureAta(connection, owner, stakeMint(), owner, STAKE_PROGRAM, ixs);
  return program.methods.stake(amount).accountsStrict({
    user: owner, config, userInfo,
    beefMint: beefMint(), stakeMint: stakeMint(),
    userBeefAta, vault, userStakeAta,
    beefTokenProgram: CLASSIC_PROGRAM, stakeTokenProgram: STAKE_PROGRAM,
    systemProgram: SystemProgram.programId,
  }).preInstructions(ixs).rpc();
}

export async function doUnstake({ program, connection, owner }: Ctx, amount: BN) {
  const { config, vault, userInfo } = pdas(program.programId, owner);
  const ixs: any[] = [];
  const userBeefAta = await ensureAta(connection, owner, beefMint(), owner, CLASSIC_PROGRAM, ixs);
  const userStakeAta = await ensureAta(connection, owner, stakeMint(), owner, STAKE_PROGRAM, ixs);
  return program.methods.unstake(amount).accountsStrict({
    user: owner, config, userInfo,
    beefMint: beefMint(), stakeMint: stakeMint(),
    userBeefAta, vault, userStakeAta,
    beefTokenProgram: CLASSIC_PROGRAM, stakeTokenProgram: STAKE_PROGRAM,
  }).preInstructions(ixs).rpc();
}

export async function doClaim({ program, connection, owner }: Ctx) {
  const { config, rewardVault, userInfo } = pdas(program.programId, owner);
  const ixs: any[] = [];
  const userRewardAta = await ensureAta(connection, owner, rewardMint(), owner, CLASSIC_PROGRAM, ixs);
  return program.methods.claimRewards().accountsStrict({
    user: owner, config, userInfo,
    rewardMint: rewardMint(), rewardVault, userRewardAta,
    rewardTokenProgram: CLASSIC_PROGRAM,
  }).preInstructions(ixs).rpc();
}

export async function doFaucet({ program, connection, owner }: Ctx, amount: BN) {
  const { config } = pdas(program.programId, owner);
  const ixs: any[] = [];
  const userBeefAta = await ensureAta(connection, owner, beefMint(), owner, CLASSIC_PROGRAM, ixs);
  return program.methods.faucet(amount).accountsStrict({
    user: owner, config, beefMint: beefMint(), userBeefAta,
    beefTokenProgram: CLASSIC_PROGRAM,
  }).preInstructions(ixs).rpc();
}
