/**
 * ONE-TIME: transfer the $BEEF mint authority from your wallet to the Config PDA,
 * so the on-chain `faucet` instruction (program-signed) can mint test $BEEF.
 *
 * Run ONCE (from repo root), after deploying the program that contains `faucet`:
 *   npx ts-node scripts/beef-authority-to-pda.ts
 *
 * Your wallet must currently be the $BEEF mint authority (it is, if you ran
 * setup-devnet.ts). After this, only the program can mint $BEEF.
 */
import fs from "fs";
import os from "os";
import path from "path";
import { Program, Wallet, AnchorProvider, type Idl } from "@coral-xyz/anchor";
import { Connection, Keypair, PublicKey } from "@solana/web3.js";
import { AuthorityType, setAuthority } from "@solana/spl-token";

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
  const program = new Program(idl, provider) as any;

  const [configPda] = PublicKey.findProgramAddressSync([Buffer.from("config")], programId);
  const config: any = await program.account.config.fetch(configPda);
  const beefMint: PublicKey = config.beefMint;

  await setAuthority(
    connection,
    payer,
    beefMint,
    payer, // current authority
    AuthorityType.MintTokens,
    configPda // new authority = Config PDA
  );

  console.log(`✅ $BEEF (${beefMint.toBase58()}) mint authority → Config PDA ${configPda.toBase58()}`);
  console.log("The on-chain faucet can now mint $BEEF. (Your wallet can no longer mint it directly.)");
}

main().then(
  () => process.exit(0),
  (e) => {
    console.error(e);
    process.exit(1);
  }
);
