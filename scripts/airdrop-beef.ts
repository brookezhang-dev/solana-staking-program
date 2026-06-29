/**
 * Mint test $BEEF to any wallet so it can stake (the $BEEF mint authority is your
 * setup wallet, ~/.config/solana/id.json).
 *
 * Run from repo root:
 *   npx ts-node scripts/airdrop-beef.ts <RECIPIENT_PUBKEY> [amount]
 *   # e.g. npx ts-node scripts/airdrop-beef.ts HYC3...zRSi 1000
 *
 * amount is in whole $BEEF (default 1000).
 */
import fs from "fs";
import os from "os";
import path from "path";
import { AnchorProvider, Program, Wallet, type Idl } from "@coral-xyz/anchor";
import { Connection, Keypair, PublicKey } from "@solana/web3.js";
import { getOrCreateAssociatedTokenAccount, mintTo } from "@solana/spl-token";

const DECIMALS = 9;

function loadKeypair(): Keypair {
  const p = process.env.ANCHOR_WALLET || path.join(os.homedir(), ".config/solana/id.json");
  return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(p, "utf8"))));
}

async function main() {
  const recipientArg = process.argv[2];
  if (!recipientArg) throw new Error("usage: ts-node scripts/airdrop-beef.ts <RECIPIENT_PUBKEY> [amount]");
  const recipient = new PublicKey(recipientArg);
  const whole = BigInt(process.argv[3] ?? "1000");

  const idl = JSON.parse(
    fs.readFileSync(path.join(__dirname, "../target/idl/staking.json"), "utf8")
  ) as Idl & { address: string };
  const programId = new PublicKey(idl.address);

  const payer = loadKeypair(); // = $BEEF mint authority
  const connection = new Connection("https://api.devnet.solana.com", "confirmed");
  const provider = new AnchorProvider(connection, new Wallet(payer), { commitment: "confirmed" });
  const program = new Program(idl, provider) as any;

  const [configPda] = PublicKey.findProgramAddressSync([Buffer.from("config")], programId);
  const config: any = await program.account.config.fetch(configPda);
  const beefMint: PublicKey = config.beefMint;

  const ata = await getOrCreateAssociatedTokenAccount(connection, payer, beefMint, recipient);
  const amount = whole * BigInt(10) ** BigInt(DECIMALS);
  await mintTo(connection, payer, beefMint, ata.address, payer, amount);

  console.log(`✅ minted ${whole} $BEEF to ${recipient.toBase58()}`);
  console.log(`   $BEEF mint: ${beefMint.toBase58()}`);
  console.log(`   recipient ATA: ${ata.address.toBase58()}`);
}

main().then(
  () => process.exit(0),
  (e) => {
    console.error(e);
    process.exit(1);
  }
);
