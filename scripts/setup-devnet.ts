/**
 * Devnet setup (roadmap 4.3): create the three mints, hand $STAKE/$MILK mint
 * authority to the Config PDA, run `initialize`, and fund the payer with $BEEF.
 *
 * Prereqs:
 *   solana config set --url devnet
 *   solana airdrop 2
 *   anchor build && anchor deploy     # program id already in declare_id!/Anchor.toml
 *
 * Run from repo root:
 *   npx ts-node scripts/setup-devnet.ts
 *
 * Then paste the printed mint addresses into app/src/config.ts.
 */
import fs from "fs";
import os from "os";
import path from "path";
import { AnchorProvider, BN, Program, Wallet, type Idl } from "@coral-xyz/anchor";
import { Connection, Keypair, PublicKey, SystemProgram, SYSVAR_RENT_PUBKEY } from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  createMint,
  getOrCreateAssociatedTokenAccount,
  mintTo,
} from "@solana/spl-token";

const DECIMALS = 9;
const INITIAL_RATE = new BN(1_000_000); // $MILK base units / sec
const DECAY_PER_SEC = new BN(1_000);
const MIN_RATE = new BN(1_000);

function loadKeypair(): Keypair {
  const p = process.env.ANCHOR_WALLET || path.join(os.homedir(), ".config/solana/id.json");
  return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(p, "utf8"))));
}

async function main() {
  const idl = JSON.parse(
    fs.readFileSync(path.join(__dirname, "../target/idl/staking.json"), "utf8")
  ) as Idl & { address: string };
  const programId = new PublicKey(idl.address);

  const payer = loadKeypair();
  const connection = new Connection("https://api.devnet.solana.com", "confirmed");
  const provider = new AnchorProvider(connection, new Wallet(payer), { commitment: "confirmed" });
  const program = new Program(idl, provider);

  const [configPda] = PublicKey.findProgramAddressSync([Buffer.from("config")], programId);
  const [vaultPda] = PublicKey.findProgramAddressSync([Buffer.from("vault")], programId);

  console.log("payer    :", payer.publicKey.toBase58());
  console.log("program  :", programId.toBase58());
  console.log("config   :", configPda.toBase58());

  // 1) Create mints. $STAKE/$MILK authority = Config PDA (only the program can mint).
  const beefMint = await createMint(connection, payer, payer.publicKey, null, DECIMALS);
  const stakeMint = await createMint(connection, payer, configPda, null, DECIMALS);
  const milkMint = await createMint(connection, payer, configPda, null, DECIMALS);

  // 2) initialize the pool.
  await program.methods
    .initialize(INITIAL_RATE, DECAY_PER_SEC, MIN_RATE)
    .accountsStrict({
      admin: payer.publicKey,
      config: configPda,
      beefMint,
      stakeMint,
      milkMint,
      vault: vaultPda,
      tokenProgram: TOKEN_PROGRAM_ID,
      systemProgram: SystemProgram.programId,
      rent: SYSVAR_RENT_PUBKEY,
    })
    .rpc();

  // 3) Fund the payer with 1000 $BEEF to play with.
  const beefAta = await getOrCreateAssociatedTokenAccount(
    connection, payer, beefMint, payer.publicKey
  );
  await mintTo(
    connection, payer, beefMint, beefAta.address, payer,
    BigInt(1000) * BigInt(10) ** BigInt(DECIMALS)
  );

  console.log("\n✅ initialized. Paste into app/src/config.ts:\n");
  console.log(`export const BEEF_MINT  = "${beefMint.toBase58()}";`);
  console.log(`export const STAKE_MINT = "${stakeMint.toBase58()}";`);
  console.log(`export const MILK_MINT  = "${milkMint.toBase58()}";`);
}

main().then(
  () => process.exit(0),
  (e) => {
    console.error(e);
    process.exit(1);
  }
);
