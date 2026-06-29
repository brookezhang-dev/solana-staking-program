/**
 * Attach Metaplex Token Metadata to $STAKE and $MILK so wallets show names
 * instead of "Unknown token".
 *
 * Requires the program to include `create_token_metadata` (rebuild + redeploy):
 *   anchor build && anchor deploy
 *   (cd app && npm run copy-idl)        # refresh the IDL the script/app load
 *
 * Run from repo root:
 *   npx ts-node scripts/add-metadata.ts
 *
 * $BEEF is the external input token (its mint authority is your wallet, not the
 * program), so name it separately if desired — out of scope here.
 */
import fs from "fs";
import os from "os";
import path from "path";
import { AnchorProvider, Program, Wallet, type Idl } from "@coral-xyz/anchor";
import { Connection, Keypair, PublicKey, SYSVAR_RENT_PUBKEY, SystemProgram } from "@solana/web3.js";

// Metaplex Token Metadata program (same address on all clusters).
const TOKEN_METADATA_PROGRAM_ID = new PublicKey(
  "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"
);

// Edit names/symbols/URIs here. URI can be "" (no off-chain JSON/image).
const TOKENS = [
  { which: "stake", name: "Staking Receipt", symbol: "STAKE", uri: "" },
  { which: "milk", name: "Milk Reward", symbol: "MILK", uri: "" },
];

function loadKeypair(): Keypair {
  const p = process.env.ANCHOR_WALLET || path.join(os.homedir(), ".config/solana/id.json");
  return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(p, "utf8"))));
}

function metadataPda(mint: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("metadata"), TOKEN_METADATA_PROGRAM_ID.toBuffer(), mint.toBuffer()],
    TOKEN_METADATA_PROGRAM_ID
  )[0];
}

async function main() {
  const idl = JSON.parse(
    fs.readFileSync(path.join(__dirname, "../target/idl/staking.json"), "utf8")
  ) as Idl & { address: string };
  const programId = new PublicKey(idl.address);

  const payer = loadKeypair();
  const connection = new Connection("https://api.devnet.solana.com", "confirmed");
  const provider = new AnchorProvider(connection, new Wallet(payer), { commitment: "confirmed" });
  // Cast to any: the IDL is loaded generically (no generated TS types), so the
  // typed account/methods namespaces aren't known at compile time.
  const program = new Program(idl, provider) as any;

  const [configPda] = PublicKey.findProgramAddressSync([Buffer.from("config")], programId);
  const config: any = await program.account.config.fetch(configPda);
  const mintOf: Record<string, PublicKey> = {
    stake: config.stakeMint,
    milk: config.milkMint,
  };

  for (const t of TOKENS) {
    const mint = mintOf[t.which];
    const metadata = metadataPda(mint);
    const exists = await connection.getAccountInfo(metadata);
    if (exists) {
      console.log(`• ${t.symbol}: metadata already exists, skipping (${metadata.toBase58()})`);
      continue;
    }
    const sig = await program.methods
      .createTokenMetadata(t.name, t.symbol, t.uri)
      .accountsStrict({
        admin: payer.publicKey,
        config: configPda,
        mint,
        metadata,
        tokenMetadataProgram: TOKEN_METADATA_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
        rent: SYSVAR_RENT_PUBKEY,
      })
      .rpc();
    console.log(`✅ ${t.symbol} metadata set — ${mint.toBase58()}\n   tx: ${sig}`);
  }

  console.log("\nReconnect the wallet / refresh the explorer to see the names.");
}

main().then(
  () => process.exit(0),
  (e) => {
    console.error(e);
    process.exit(1);
  }
);
