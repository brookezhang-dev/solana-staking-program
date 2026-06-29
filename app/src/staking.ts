import {
  AnchorProvider,
  BN,
  Program,
  type Idl,
  type Wallet,
} from "@coral-xyz/anchor";
import { Connection, PublicKey, SystemProgram } from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  getAssociatedTokenAddressSync,
  createAssociatedTokenAccountInstruction,
  getAccount,
} from "@solana/spl-token";
import { ACC_PRECISION, BEEF_MINT, MILK_MINT, STAKE_MINT, DECIMALS } from "./config";

// ---- IDL loaded at runtime from /staking.json (copied from target/idl by `npm run copy-idl`) ----
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
  // Anchor 0.30+ reads the program id from idl.address.
  return new Program(idl, provider);
}

// ---- PDAs (same seeds as on-chain, design doc §5.1) ----
export function pdas(programId: PublicKey, owner?: PublicKey) {
  const [config] = PublicKey.findProgramAddressSync([Buffer.from("config")], programId);
  const [vault] = PublicKey.findProgramAddressSync([Buffer.from("vault")], programId);
  const userInfo = owner
    ? PublicKey.findProgramAddressSync([Buffer.from("user"), owner.toBuffer()], programId)[0]
    : undefined;
  return { config, vault, userInfo };
}

export const beefMint = () => new PublicKey(BEEF_MINT);
export const stakeMint = () => new PublicKey(STAKE_MINT);
export const milkMint = () => new PublicKey(MILK_MINT);

export const toBase = (uiAmount: string) =>
  new BN(Math.round(parseFloat(uiAmount || "0") * 10 ** DECIMALS));
export const fromBase = (base: bigint) => Number(base) / 10 ** DECIMALS;

// Prepend an ATA-creation ix if the account doesn't exist yet.
async function ensureAta(
  connection: Connection,
  owner: PublicKey,
  mint: PublicKey,
  payer: PublicKey,
  ixs: any[]
) {
  const ata = getAssociatedTokenAddressSync(mint, owner);
  const info = await connection.getAccountInfo(ata);
  if (!info) ixs.push(createAssociatedTokenAccountInstruction(payer, ata, owner, mint));
  return ata;
}

// ---- Balances ----
export async function readBalance(connection: Connection, mint: PublicKey, owner: PublicKey) {
  try {
    const ata = getAssociatedTokenAddressSync(mint, owner);
    const acc = await getAccount(connection, ata);
    return acc.amount; // bigint, base units
  } catch {
    return 0n;
  }
}

// ---- Emission mirror (must match reward::emission_between) ----
function emissionBetween(cfg: any, aLast: bigint, bLast: bigint): bigint {
  if (bLast <= aLast) return 0n;
  const r0 = BigInt(cfg.initialRate.toString());
  const k = BigInt(cfg.decayPerSec.toString());
  const floor = BigInt(cfg.minRate.toString());
  const start = BigInt(cfg.startTime.toString());
  if (k === 0n) return r0 * (bLast - aLast);
  const a = aLast > start ? aLast : start;
  if (bLast <= a) return 0n;
  const g = r0 > floor ? r0 - floor : 0n;
  if (g === 0n) return floor * (bLast - a);
  const tFloor = start + g / k;
  const decayEnd = bLast < tFloor ? bLast : tFloor;
  const floorStart = a > tFloor ? a : tFloor;
  let total = 0n;
  if (decayEnd > a) {
    const aRel = a - start;
    const eRel = decayEnd - start;
    const twoRect = r0 * (eRel - aRel) * 2n;
    const decline = k * (eRel * eRel - aRel * aRel);
    total += (twoRect - decline) / 2n;
  }
  if (bLast > floorStart) total += floor * (bLast - floorStart);
  return total;
}

// Estimate claimable $MILK now (on-chain value is authoritative at claim time).
export function estimatePending(cfg: any, userInfo: any): bigint {
  const now = BigInt(Math.floor(Date.now() / 1000));
  const total = BigInt(cfg.totalStaked.toString());
  let acc = BigInt(cfg.accRewardPerShare.toString());
  const last = BigInt(cfg.lastRewardTime.toString());
  if (total > 0n) acc += (emissionBetween(cfg, last, now) * ACC_PRECISION) / total;
  const amount = BigInt(userInfo.amount.toString());
  const debt = BigInt(userInfo.rewardDebt.toString());
  const accrued = (amount * acc) / ACC_PRECISION;
  return accrued > debt ? accrued - debt : 0n;
}

// ---- Actions ----
type Ctx = { program: Program; connection: Connection; owner: PublicKey };

export async function doStake({ program, connection, owner }: Ctx, amount: BN) {
  const { config, vault, userInfo } = pdas(program.programId, owner);
  const ixs: any[] = [];
  const userBeefAta = await ensureAta(connection, owner, beefMint(), owner, ixs);
  const userStakeAta = await ensureAta(connection, owner, stakeMint(), owner, ixs);
  const userMilkAta = await ensureAta(connection, owner, milkMint(), owner, ixs);
  return program.methods
    .stake(amount)
    .accountsStrict({
      user: owner,
      config,
      userInfo,
      userBeefAta,
      vault,
      userStakeAta,
      stakeMint: stakeMint(),
      milkMint: milkMint(),
      userMilkAta,
      tokenProgram: TOKEN_PROGRAM_ID,
      systemProgram: SystemProgram.programId,
    })
    .preInstructions(ixs)
    .rpc();
}

export async function doUnstake({ program, connection, owner }: Ctx, amount: BN) {
  const { config, vault, userInfo } = pdas(program.programId, owner);
  const ixs: any[] = [];
  const userBeefAta = await ensureAta(connection, owner, beefMint(), owner, ixs);
  const userStakeAta = await ensureAta(connection, owner, stakeMint(), owner, ixs);
  const userMilkAta = await ensureAta(connection, owner, milkMint(), owner, ixs);
  return program.methods
    .unstake(amount)
    .accountsStrict({
      user: owner,
      config,
      userInfo,
      userBeefAta,
      vault,
      userStakeAta,
      stakeMint: stakeMint(),
      milkMint: milkMint(),
      userMilkAta,
      tokenProgram: TOKEN_PROGRAM_ID,
    })
    .preInstructions(ixs)
    .rpc();
}

export async function doFaucet({ program, connection, owner }: Ctx, amount: BN) {
  const { config } = pdas(program.programId, owner);
  const ixs: any[] = [];
  const userBeefAta = await ensureAta(connection, owner, beefMint(), owner, ixs);
  return program.methods
    .faucet(amount)
    .accountsStrict({
      user: owner,
      config,
      beefMint: beefMint(),
      userBeefAta,
      tokenProgram: TOKEN_PROGRAM_ID,
    })
    .preInstructions(ixs)
    .rpc();
}

export async function doClaim({ program, connection, owner }: Ctx) {
  const { config, userInfo } = pdas(program.programId, owner);
  const ixs: any[] = [];
  const userMilkAta = await ensureAta(connection, owner, milkMint(), owner, ixs);
  return program.methods
    .claimRewards()
    .accountsStrict({
      user: owner,
      config,
      userInfo,
      milkMint: milkMint(),
      userMilkAta,
      tokenProgram: TOKEN_PROGRAM_ID,
    })
    .preInstructions(ixs)
    .rpc();
}
