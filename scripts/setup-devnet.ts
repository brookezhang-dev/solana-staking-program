/**
 * Devnet setup (v3.x — Config/Pool two-tier). Creates the three tokens, initializes
 * the protocol Config, creates ONE pool (+ its two vaults), initializes the hook's
 * ExtraAccountMetaList, and prefunds the reward vault with the exact total liability.
 * Prints addresses for app/src/config.ts.
 *
 * Prereqs (from repo root):
 *   solana config set --url devnet && solana airdrop 2
 *   anchor build && anchor deploy && (cd app && npm run copy-idl)
 *   npx ts-node scripts/setup-devnet.ts
 *
 * Order matters: $STAKE mint authority AND $BEEF mint authority must be handed to the
 * POOL PDA before create_pool (create_pool asserts $STAKE authority == pool; the faucet
 * needs $BEEF authority == pool). create_pool must precede initialize_extra_account_meta_list.
 */
import fs from "fs";
import os from "os";
import path from "path";
import { AnchorProvider, BN, Program, Wallet, type Idl } from "@coral-xyz/anchor";
import {
  Connection, Keypair, PublicKey, SystemProgram, SYSVAR_RENT_PUBKEY,
  Transaction, sendAndConfirmTransaction,
} from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID, ExtensionType, getMintLen, LENGTH_SIZE, TYPE_SIZE,
  createInitializeMintInstruction, createInitializeTransferHookInstruction,
  createInitializeMetadataPointerInstruction, setAuthority, AuthorityType,
  createMint, getOrCreateAssociatedTokenAccount, mintTo,
} from "@solana/spl-token";
import { createInitializeInstruction, pack, type TokenMetadata } from "@solana/spl-token-metadata";

const DEC = 9;
const UNIT = 10n ** BigInt(DEC);
const REWARD_PER_SEC: bigint = 1_000_000n;
const DURATION_SECS = 30 * 24 * 3600;

function loadKeypair(): Keypair {
  const p = process.env.ANCHOR_WALLET || path.join(os.homedir(), ".config/solana/id.json");
  return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(p, "utf8"))));
}

async function main() {
  const idl = JSON.parse(fs.readFileSync(path.join(__dirname, "../target/idl/staking.json"), "utf8")) as Idl & { address: string };
  const programId = new PublicKey(idl.address);
  const payer = loadKeypair();
  const connection = new Connection("https://api.devnet.solana.com", "confirmed");
  const provider = new AnchorProvider(connection, new Wallet(payer), { commitment: "confirmed" });
  const program = new Program(idl, provider) as any;

  const [configPda] = PublicKey.findProgramAddressSync([Buffer.from("config")], programId);

  // 1) $BEEF (classic) + $MILK reward (classic, external authority).
  const beefMint = await createMint(connection, payer, payer.publicKey, null, DEC);
  const beefAta = await getOrCreateAssociatedTokenAccount(connection, payer, beefMint, payer.publicKey);
  await mintTo(connection, payer, beefMint, beefAta.address, payer, 1000n * UNIT);
  const rewardMint = await createMint(connection, payer, payer.publicKey, null, DEC);

  // 2) $STAKE: Token-2022 with TransferHook (→ this program) + Metadata.
  const stakeKp = Keypair.generate();
  const stakeMint = stakeKp.publicKey;
  const md: TokenMetadata = { mint: stakeMint, name: "Staking Receipt", symbol: "STAKE", uri: "", additionalMetadata: [] };
  const mintLen = getMintLen([ExtensionType.TransferHook, ExtensionType.MetadataPointer]);
  const mdLen = TYPE_SIZE + LENGTH_SIZE + pack(md).length;
  const lamports = await connection.getMinimumBalanceForRentExemption(mintLen + mdLen);
  await sendAndConfirmTransaction(connection, new Transaction().add(
    SystemProgram.createAccount({ fromPubkey: payer.publicKey, newAccountPubkey: stakeMint, space: mintLen, lamports, programId: TOKEN_2022_PROGRAM_ID }),
    createInitializeMetadataPointerInstruction(stakeMint, payer.publicKey, stakeMint, TOKEN_2022_PROGRAM_ID),
    createInitializeTransferHookInstruction(stakeMint, payer.publicKey, programId, TOKEN_2022_PROGRAM_ID),
    createInitializeMintInstruction(stakeMint, DEC, payer.publicKey, null, TOKEN_2022_PROGRAM_ID),
    createInitializeInstruction({ programId: TOKEN_2022_PROGRAM_ID, metadata: stakeMint, updateAuthority: payer.publicKey, mint: stakeMint, mintAuthority: payer.publicKey, name: md.name, symbol: md.symbol, uri: md.uri }),
  ), [payer, stakeKp]);

  // 3) Pool PDA + children (derivable now).
  const [poolPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("pool"), configPda.toBuffer(), beefMint.toBuffer(), rewardMint.toBuffer()], programId);
  const [stakedVault] = PublicKey.findProgramAddressSync([Buffer.from("staked_vault"), poolPda.toBuffer()], programId);
  const [rewardVault] = PublicKey.findProgramAddressSync([Buffer.from("reward_vault"), poolPda.toBuffer()], programId);
  const [extraMeta] = PublicKey.findProgramAddressSync([Buffer.from("extra-account-metas"), stakeMint.toBuffer()], programId);

  // 4) Hand $STAKE + $BEEF mint authority to the Pool PDA.
  await setAuthority(connection, payer, stakeMint, payer, AuthorityType.MintTokens, poolPda, [], undefined, TOKEN_2022_PROGRAM_ID);
  await setAuthority(connection, payer, beefMint, payer, AuthorityType.MintTokens, poolPda);

  // 5) initialize_config (once).
  await program.methods.initializeConfig().accountsStrict({
    admin: payer.publicKey, config: configPda, systemProgram: SystemProgram.programId,
  }).rpc();

  // 6) create_pool.
  const now = Math.floor(Date.now() / 1000);
  const start = now, end = now + DURATION_SECS;
  await program.methods.createPool(new BN(REWARD_PER_SEC.toString()), new BN(start), new BN(end)).accountsStrict({
    admin: payer.publicKey, config: configPda,
    stakedMint: beefMint, rewardMint, stakeReceiptMint: stakeMint,
    pool: poolPda, stakedVault, rewardVault,
    stakedTokenProgram: TOKEN_PROGRAM_ID, rewardTokenProgram: TOKEN_PROGRAM_ID,
    systemProgram: SystemProgram.programId, rent: SYSVAR_RENT_PUBKEY,
  }).rpc();

  // 7) initialize the hook's ExtraAccountMetaList.
  await program.methods.initializeExtraAccountMetaList().accountsStrict({
    payer: payer.publicKey, extraAccountMetaList: extraMeta, pool: poolPda, stakeMint,
    systemProgram: SystemProgram.programId,
  }).rpc();

  // 8) Prefund reward vault with exact total liability.
  const liability = REWARD_PER_SEC * BigInt(end - start);
  await mintTo(connection, payer, rewardMint, rewardVault, payer, liability);

  console.log("\n✅ v3.x Config/Pool initialized + prefunded.\n");
  console.log(`pool: ${poolPda.toBase58()}`);
  console.log(`total liability funded: ${liability} base units ($MILK)`);
  console.log("\nPaste into app/src/config.ts:\n");
  console.log(`export const BEEF_MINT   = "${beefMint.toBase58()}";`);
  console.log(`export const STAKE_MINT  = "${stakeMint.toBase58()}";`);
  console.log(`export const REWARD_MINT = "${rewardMint.toBase58()}";`);
}

main().then(() => process.exit(0), (e) => { console.error(e); process.exit(1); });
