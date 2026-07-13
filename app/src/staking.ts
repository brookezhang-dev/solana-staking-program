import {
  AnchorProvider, BN, Program, type Idl, type Wallet,
} from "@coral-xyz/anchor";
import { Connection, PublicKey, SystemProgram, Transaction } from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID,
  getAssociatedTokenAddressSync, createAssociatedTokenAccountInstruction, getAccount,
  createTransferCheckedWithTransferHookInstruction,
} from "@solana/spl-token";
import { ACC_PRECISION, BEEF_MINT, STAKE_MINT, REWARD_MINT, DECIMALS } from "./config";

// Token programs per mint: $STAKE (receipt) is Token-2022 (TransferHook); staked/reward classic.
export const STAKE_PROGRAM = TOKEN_2022_PROGRAM_ID;
export const CLASSIC_PROGRAM = TOKEN_PROGRAM_ID;

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

// Semantics: staked = $BEEF (input), stakeReceipt = $STAKE (Token-2022 hook), reward = $MILK.
export const stakedMint = () => new PublicKey(BEEF_MINT);
export const stakeReceiptMint = () => new PublicKey(STAKE_MINT);
export const rewardMint = () => new PublicKey(REWARD_MINT);

// ---- PDA derivation (two-tier: config → pool → children) ----
export function configPda(programId: PublicKey) {
  return PublicKey.findProgramAddressSync([Buffer.from("config")], programId)[0];
}
export function poolPda(programId: PublicKey) {
  const config = configPda(programId);
  return PublicKey.findProgramAddressSync(
    [Buffer.from("pool"), config.toBuffer(), stakedMint().toBuffer(), rewardMint().toBuffer()],
    programId,
  )[0];
}
export function poolChildren(programId: PublicKey) {
  const config = configPda(programId);
  const pool = poolPda(programId);
  const [stakedVault] = PublicKey.findProgramAddressSync([Buffer.from("staked_vault"), pool.toBuffer()], programId);
  const [rewardVault] = PublicKey.findProgramAddressSync([Buffer.from("reward_vault"), pool.toBuffer()], programId);
  return { config, pool, stakedVault, rewardVault };
}
export function userInfoPda(programId: PublicKey, stakeAta: PublicKey) {
  const pool = poolPda(programId);
  return PublicKey.findProgramAddressSync([Buffer.from("user_info"), pool.toBuffer(), stakeAta.toBuffer()], programId)[0];
}
export const stakeAtaOf = (owner: PublicKey) =>
  getAssociatedTokenAddressSync(stakeReceiptMint(), owner, false, STAKE_PROGRAM);

export const toBase = (uiAmount: string) => new BN(Math.round(parseFloat(uiAmount || "0") * 10 ** DECIMALS));
export const fromBase = (base: bigint) => Number(base) / 10 ** DECIMALS;

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

// ---- Emission mirror (constant rate + [start,end] clamp), reads POOL fields ----
function emissionBetween(pool: any, aLast: bigint, bLast: bigint): bigint {
  const rate = BigInt(pool.rewardPerSec.toString());
  const start = BigInt(pool.startTime.toString());
  const end = BigInt(pool.endTime.toString());
  let a = aLast > start ? aLast : start;
  let b = bLast;
  if (end > 0n && b > end) b = end;
  return b > a ? rate * (b - a) : 0n;
}

// Claimable now = pending_unclaimed + freshly-accrued from balance (share = $STAKE balance).
export function estimatePending(pool: any, userInfo: any, balance: bigint): bigint {
  const now = BigInt(Math.floor(Date.now() / 1000));
  const total = BigInt(pool.totalStaked.toString());
  let acc = BigInt(pool.accRewardPerShare.toString());
  const last = BigInt(pool.lastRewardTime.toString());
  if (total > 0n) acc += (emissionBetween(pool, last, now) * ACC_PRECISION) / total;
  const debt = BigInt(userInfo.rewardDebt.toString());
  const accrued = (balance * acc) / ACC_PRECISION;
  const fresh = accrued > debt ? accrued - debt : 0n;
  return BigInt(userInfo.pendingUnclaimed.toString()) + fresh;
}

// pause flag lives on Config.
export const isPaused = (config: any) => Boolean(config?.paused);

// ---- Actions ----
type Ctx = { program: Program; connection: Connection; owner: PublicKey };

export async function doStake({ program, connection, owner }: Ctx, amount: BN) {
  const { config, pool, stakedVault } = poolChildren(program.programId);
  const ixs: any[] = [];
  const userStakedAta = await ensureAta(connection, owner, stakedMint(), owner, CLASSIC_PROGRAM, ixs);
  const userStakeAta = await ensureAta(connection, owner, stakeReceiptMint(), owner, STAKE_PROGRAM, ixs);
  const userInfo = userInfoPda(program.programId, userStakeAta);
  return program.methods.stake(amount).accountsStrict({
    user: owner, config, pool,
    stakedMint: stakedMint(), stakeReceiptMint: stakeReceiptMint(),
    userStakedAta, stakedVault, userStakeAta, userInfo,
    stakedTokenProgram: CLASSIC_PROGRAM, stakeTokenProgram: STAKE_PROGRAM,
    systemProgram: SystemProgram.programId,
  }).preInstructions(ixs).rpc();
}

export async function doUnstake({ program, connection, owner }: Ctx, amount: BN) {
  const { config, pool, stakedVault } = poolChildren(program.programId);
  const ixs: any[] = [];
  const userStakedAta = await ensureAta(connection, owner, stakedMint(), owner, CLASSIC_PROGRAM, ixs);
  const userStakeAta = await ensureAta(connection, owner, stakeReceiptMint(), owner, STAKE_PROGRAM, ixs);
  const userInfo = userInfoPda(program.programId, userStakeAta);
  return program.methods.unstake(amount).accountsStrict({
    user: owner, config, pool,
    stakedMint: stakedMint(), stakeReceiptMint: stakeReceiptMint(),
    userStakedAta, stakedVault, userStakeAta, userInfo,
    stakedTokenProgram: CLASSIC_PROGRAM, stakeTokenProgram: STAKE_PROGRAM,
  }).preInstructions(ixs).rpc();
}

export async function doClaim({ program, connection, owner }: Ctx) {
  const { config, pool, rewardVault } = poolChildren(program.programId);
  const ixs: any[] = [];
  const userStakeAta = await ensureAta(connection, owner, stakeReceiptMint(), owner, STAKE_PROGRAM, ixs);
  const userInfo = userInfoPda(program.programId, userStakeAta);
  const userRewardAta = await ensureAta(connection, owner, rewardMint(), owner, CLASSIC_PROGRAM, ixs);
  return program.methods.claimRewards().accountsStrict({
    user: owner, config, pool,
    stakeReceiptMint: stakeReceiptMint(), userStakeAta, userInfo,
    rewardMint: rewardMint(), rewardVault, userRewardAta,
    rewardTokenProgram: CLASSIC_PROGRAM,
  }).preInstructions(ixs).rpc();
}

export async function doFaucet({ program, connection, owner }: Ctx, amount: BN) {
  const { config, pool } = poolChildren(program.programId);
  const ixs: any[] = [];
  const userStakedAta = await ensureAta(connection, owner, stakedMint(), owner, CLASSIC_PROGRAM, ixs);
  return program.methods.faucet(amount).accountsStrict({
    user: owner, config, pool, stakedMint: stakedMint(), userStakedAta,
    stakedTokenProgram: CLASSIC_PROGRAM,
  }).preInstructions(ixs).rpc();
}

// Transfer $STAKE to another wallet. $STAKE is a Token-2022 TransferHook token, so the
// transfer routes through our program's hook (settling both parties' rewards). The
// recipient must have a UserInfo — we auto-create its ATA + `register` in the same tx
// (register-before-receive), then send the hook-aware transfer.
export async function doTransfer({ program, connection, owner }: Ctx, recipientStr: string, amount: BN) {
  const recipient = new PublicKey(recipientStr.trim());
  const { config, pool } = poolChildren(program.programId);
  const mint = stakeReceiptMint();
  const srcAta = getAssociatedTokenAddressSync(mint, owner, false, STAKE_PROGRAM);
  const dstAta = getAssociatedTokenAddressSync(mint, recipient, false, STAKE_PROGRAM);

  const ixs: any[] = [];
  if (!(await connection.getAccountInfo(dstAta))) {
    ixs.push(createAssociatedTokenAccountInstruction(owner, dstAta, recipient, mint, STAKE_PROGRAM));
  }
  const dstUserInfo = userInfoPda(program.programId, dstAta);
  if (!(await connection.getAccountInfo(dstUserInfo))) {
    const reg = await program.methods.register().accountsStrict({
      payer: owner, config, pool, stakeReceiptMint: mint, stakeTokenAccount: dstAta,
      userInfo: dstUserInfo, systemProgram: SystemProgram.programId,
    }).instruction();
    ixs.push(reg);
  }
  // Resolves pool + both UserInfo PDAs from the on-chain ExtraAccountMetaList.
  const xfer = await createTransferCheckedWithTransferHookInstruction(
    connection, srcAta, mint, dstAta, owner, BigInt(amount.toString()), DECIMALS, [], "confirmed", STAKE_PROGRAM,
  );
  ixs.push(xfer);

  const tx = new Transaction().add(...ixs);
  return (program.provider as AnchorProvider).sendAndConfirm!(tx);
}
