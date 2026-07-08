/**
 * Devnet setup (v3). Creates the three tokens, both vaults, initializes the pool,
 * and PREFUNDS the RewardVault with the exact total liability ∫[start,end] r(t)dt
 * so claims are always solvent (§1.5). Prints addresses for app/src/config.ts.
 *
 * Prereqs (from repo root):
 *   solana config set --url devnet && solana airdrop 2
 *   anchor build -- --features devnet-faucet && anchor deploy
 *   npx ts-node scripts/setup-devnet.ts
 *
 * ⚠ The $STAKE (Token-2022 NonTransferable + metadata) creation below is the
 * fiddliest part. Extensions MUST be initialized BEFORE initializeMint, and
 * metadata init needs the mint authority to sign — so we create with authority =
 * payer, set metadata, THEN hand mint authority to the Config PDA (initialize
 * requires it). If an spl-token API name differs in your version, adjust here.
 */
import fs from "fs";
import os from "os";
import path from "path";
import {
  AnchorProvider, BN, Program, Wallet, type Idl,
} from "@coral-xyz/anchor";
import {
  Connection, Keypair, PublicKey, SystemProgram, SYSVAR_RENT_PUBKEY,
  Transaction, sendAndConfirmTransaction,
} from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID, ExtensionType, getMintLen,
  LENGTH_SIZE, TYPE_SIZE,
  createInitializeMintInstruction,
  createInitializeNonTransferableMintInstruction,
  createInitializeMetadataPointerInstruction,
  setAuthority, AuthorityType,
  createMint, getOrCreateAssociatedTokenAccount, mintTo,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";
import { createInitializeInstruction, pack, type TokenMetadata } from "@solana/spl-token-metadata";

const DEC = 9;
const UNIT = 10n ** BigInt(DEC);

// Emission params (edit freely). end_time bounds the total liability.
// `: bigint` annotations stop TS narrowing to a literal type (else `k === 0n` errors).
const INITIAL_RATE: bigint = 1_000_000n; // $MILK base units / sec at anchor
const DECAY_PER_SEC: bigint = 1_000n;
const MIN_RATE: bigint = 1_000n;
const DURATION_SECS = 30 * 24 * 3600; // 30 days

function loadKeypair(): Keypair {
  const p = process.env.ANCHOR_WALLET || path.join(os.homedir(), ".config/solana/id.json");
  return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(p, "utf8"))));
}

// TS mirror of reward::emission_between over [start, end] at initial params
// (rate_anchor_time == start_time == start). Used to size the prefund.
function totalLiability(start: number, end: number): bigint {
  if (end <= start) return 0n;
  const a = BigInt(start), b = BigInt(end), anchor = BigInt(start);
  const r0 = INITIAL_RATE, k = DECAY_PER_SEC, floor = MIN_RATE;
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

async function main() {
  const idl = JSON.parse(fs.readFileSync(path.join(__dirname, "../target/idl/staking.json"), "utf8")) as Idl & { address: string };
  const programId = new PublicKey(idl.address);
  const payer = loadKeypair();
  const connection = new Connection("https://api.devnet.solana.com", "confirmed");
  const provider = new AnchorProvider(connection, new Wallet(payer), { commitment: "confirmed" });
  const program = new Program(idl, provider) as any;

  const [configPda] = PublicKey.findProgramAddressSync([Buffer.from("config")], programId);
  const [vaultPda] = PublicKey.findProgramAddressSync([Buffer.from("vault")], programId);
  const [rewardVaultPda] = PublicKey.findProgramAddressSync([Buffer.from("reward_vault")], programId);

  // 1) $BEEF: classic SPL, authority = payer for initial mint, then handed to
  //    Config PDA so the devnet faucet can mint. Fund the payer first.
  const beefMint = await createMint(connection, payer, payer.publicKey, null, DEC);
  const beefAta = await getOrCreateAssociatedTokenAccount(connection, payer, beefMint, payer.publicKey);
  await mintTo(connection, payer, beefMint, beefAta.address, payer, 1000n * UNIT);
  await setAuthority(connection, payer, beefMint, payer, AuthorityType.MintTokens, configPda);

  // 2) $MILK: classic SPL, authority stays EXTERNAL (payer). Program never mints it.
  const rewardMint = await createMint(connection, payer, payer.publicKey, null, DEC);

  // 3) $STAKE: Token-2022 with NonTransferable + Metadata. Create with authority =
  //    payer, set metadata, then transfer mint authority to Config PDA.
  const stakeKp = Keypair.generate();
  const stakeMint = stakeKp.publicKey;
  const md: TokenMetadata = { mint: stakeMint, name: "Staking Receipt", symbol: "STAKE", uri: "", additionalMetadata: [] };
  const mintLen = getMintLen([ExtensionType.NonTransferable, ExtensionType.MetadataPointer]);
  const mdLen = TYPE_SIZE + LENGTH_SIZE + pack(md).length;
  const lamports = await connection.getMinimumBalanceForRentExemption(mintLen + mdLen);
  const tx = new Transaction().add(
    SystemProgram.createAccount({ fromPubkey: payer.publicKey, newAccountPubkey: stakeMint, space: mintLen, lamports, programId: TOKEN_2022_PROGRAM_ID }),
    createInitializeMetadataPointerInstruction(stakeMint, payer.publicKey, stakeMint, TOKEN_2022_PROGRAM_ID),
    createInitializeNonTransferableMintInstruction(stakeMint, TOKEN_2022_PROGRAM_ID),
    createInitializeMintInstruction(stakeMint, DEC, payer.publicKey, null, TOKEN_2022_PROGRAM_ID),
    createInitializeInstruction({ programId: TOKEN_2022_PROGRAM_ID, metadata: stakeMint, updateAuthority: payer.publicKey, mint: stakeMint, mintAuthority: payer.publicKey, name: md.name, symbol: md.symbol, uri: md.uri }),
  );
  await sendAndConfirmTransaction(connection, tx, [payer, stakeKp]);
  // hand $STAKE mint authority to the program PDA (required by initialize)
  await setAuthority(connection, payer, stakeMint, payer, AuthorityType.MintTokens, configPda, [], undefined, TOKEN_2022_PROGRAM_ID);

  // 4) initialize the pool.
  const now = Math.floor(Date.now() / 1000);
  const start = now;
  const end = now + DURATION_SECS;
  await program.methods
    .initialize(new BN(INITIAL_RATE.toString()), new BN(DECAY_PER_SEC.toString()), new BN(MIN_RATE.toString()), new BN(start), new BN(end))
    .accountsStrict({
      admin: payer.publicKey,
      config: configPda,
      beefMint, stakeMint, rewardMint,
      vault: vaultPda, rewardVault: rewardVaultPda,
      beefTokenProgram: TOKEN_PROGRAM_ID,
      rewardTokenProgram: TOKEN_PROGRAM_ID,
      systemProgram: SystemProgram.programId,
      rent: SYSVAR_RENT_PUBKEY,
    })
    .rpc();

  // 5) Prefund RewardVault with the exact total liability (guarantees solvency).
  const liability = totalLiability(start, end);
  await mintTo(connection, payer, rewardMint, rewardVaultPda, payer, liability);

  console.log("\n✅ v3 pool initialized + prefunded.\n");
  console.log(`total liability funded: ${liability} base units ($MILK)`);
  console.log("\nPaste into app/src/config.ts:\n");
  console.log(`export const BEEF_MINT   = "${beefMint.toBase58()}";`);
  console.log(`export const STAKE_MINT  = "${stakeMint.toBase58()}";`);
  console.log(`export const REWARD_MINT = "${rewardMint.toBase58()}";`);
}

main().then(() => process.exit(0), (e) => { console.error(e); process.exit(1); });
