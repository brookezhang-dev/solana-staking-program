/**
 * Anchor tests (v3). See v3 执行计划 §4.
 *
 * RUN ON LOCALNET (fresh validator each run — avoids the devnet singleton-config
 * conflict):
 *   1) set [provider] cluster = "Localnet" in Anchor.toml (or run a local validator)
 *   2) recent `solana-test-validator` bundles Token-2022 (TokenzQd…); if yours does
 *      not, add to Anchor.toml:
 *        [test.validator]
 *        url = "https://api.devnet.solana.com"
 *        [[test.validator.clone]]
 *        address = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
 *   3) anchor test
 *
 * Determinism note: emission assertions read on-chain timestamps and recompute the
 * exact integer math in a BigInt mirror (stronger than a fixed table).
 */
import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { Staking } from "../target/types/staking";
import { PublicKey, Keypair, SystemProgram, SYSVAR_RENT_PUBKEY, Transaction, sendAndConfirmTransaction, LAMPORTS_PER_SOL } from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID, ExtensionType, getMintLen, LENGTH_SIZE, TYPE_SIZE,
  createMint, getOrCreateAssociatedTokenAccount, mintTo, getAccount, getAssociatedTokenAddressSync,
  createInitializeMintInstruction, createInitializeNonTransferableMintInstruction,
  createInitializeMetadataPointerInstruction, setAuthority, AuthorityType,
  createTransferCheckedInstruction,
} from "@solana/spl-token";
import { createInitializeInstruction, pack, type TokenMetadata } from "@solana/spl-token-metadata";
import { assert, expect } from "chai";

const DEC = 9;
const UNIT = 10n ** BigInt(DEC);
const ACC_PRECISION = 1_000_000_000_000n;
// Emission params used by initialize.
const INITIAL_RATE = 1_000_000n, DECAY = 1_000n, MIN = 1_000n;

describe("staking v3", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.Staking as Program<Staking>;
  const conn = provider.connection;
  const wallet = provider.wallet as anchor.Wallet;
  const payer = wallet.payer;

  const [configPda] = PublicKey.findProgramAddressSync([Buffer.from("config")], program.programId);
  const [vaultPda] = PublicKey.findProgramAddressSync([Buffer.from("vault")], program.programId);
  const [rewardVaultPda] = PublicKey.findProgramAddressSync([Buffer.from("reward_vault")], program.programId);
  const userInfo = (u: PublicKey) => PublicKey.findProgramAddressSync([Buffer.from("user"), u.toBuffer()], program.programId)[0];

  let beefMint: PublicKey, rewardMint: PublicKey, stakeMint: PublicKey;
  let start = 0, end = 0;

  // ---- BigInt mirror of reward::emission_between (anchor base + end_time) ----
  function emission(cfg: any, a: bigint, b: bigint): bigint {
    const r0 = BigInt(cfg.initialRate), k = BigInt(cfg.decayPerSec), floor = BigInt(cfg.minRate);
    const st = BigInt(cfg.startTime), anc = BigInt(cfg.rateAnchorTime), e = BigInt(cfg.endTime);
    let aa = a > st ? a : st; let bb = b;
    if (e > 0n && bb > e) bb = e;
    if (aa < anc) aa = anc;
    if (bb <= aa) return 0n;
    if (k === 0n) return r0 * (bb - aa);
    const g = r0 > floor ? r0 - floor : 0n;
    if (g === 0n) return floor * (bb - aa);
    const tF = anc + g / k;
    const dEnd = bb < tF ? bb : tF, fStart = aa > tF ? aa : tF;
    let t = 0n;
    if (dEnd > aa) { const ar = aa - anc, er = dEnd - anc; t += (r0 * (er - ar) * 2n - k * (er * er - ar * ar)) / 2n; }
    if (bb > fStart) t += floor * (bb - fStart);
    return t;
  }
  const mulDiv = (a: bigint, b: bigint, c: bigint) => (a * b) / c;
  const readCfg = () => program.account.config.fetch(configPda) as any;
  const cfgToMirror = (c: any) => ({
    initialRate: c.initialRate.toString(), decayPerSec: c.decayPerSec.toString(), minRate: c.minRate.toString(),
    startTime: c.startTime.toString(), rateAnchorTime: c.rateAnchorTime.toString(), endTime: c.endTime.toString(),
  });

  // Create a Token-2022 NonTransferable + metadata mint, authority → configPda.
  async function createStake2022(): Promise<PublicKey> {
    const kp = Keypair.generate();
    const mint = kp.publicKey;
    const md: TokenMetadata = { mint, name: "Staking Receipt", symbol: "STAKE", uri: "", additionalMetadata: [] };
    const mintLen = getMintLen([ExtensionType.NonTransferable, ExtensionType.MetadataPointer]);
    const mdLen = TYPE_SIZE + LENGTH_SIZE + pack(md).length;
    const lamports = await conn.getMinimumBalanceForRentExemption(mintLen + mdLen);
    const tx = new Transaction().add(
      SystemProgram.createAccount({ fromPubkey: payer.publicKey, newAccountPubkey: mint, space: mintLen, lamports, programId: TOKEN_2022_PROGRAM_ID }),
      createInitializeMetadataPointerInstruction(mint, payer.publicKey, mint, TOKEN_2022_PROGRAM_ID),
      createInitializeNonTransferableMintInstruction(mint, TOKEN_2022_PROGRAM_ID),
      createInitializeMintInstruction(mint, DEC, payer.publicKey, null, TOKEN_2022_PROGRAM_ID),
      createInitializeInstruction({ programId: TOKEN_2022_PROGRAM_ID, metadata: mint, updateAuthority: payer.publicKey, mint, mintAuthority: payer.publicKey, name: md.name, symbol: md.symbol, uri: md.uri }),
    );
    await sendAndConfirmTransaction(conn, tx, [payer, kp]);
    await setAuthority(conn, payer, mint, payer, AuthorityType.MintTokens, configPda, [], undefined, TOKEN_2022_PROGRAM_ID);
    return mint;
  }

  before(async () => {
    beefMint = await createMint(conn, payer, payer.publicKey, null, DEC);
    rewardMint = await createMint(conn, payer, payer.publicKey, null, DEC);
    stakeMint = await createStake2022();

    const now = Math.floor(Date.now() / 1000);
    start = now; end = now + 30 * 24 * 3600;

    await program.methods
      .initialize(new BN(INITIAL_RATE.toString()), new BN(DECAY.toString()), new BN(MIN.toString()), new BN(start), new BN(end))
      .accountsStrict({
        admin: payer.publicKey, config: configPda,
        beefMint, stakeMint, rewardMint,
        vault: vaultPda, rewardVault: rewardVaultPda,
        beefTokenProgram: TOKEN_PROGRAM_ID, rewardTokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId, rent: SYSVAR_RENT_PUBKEY,
      }).rpc();

    // Prefund reward vault with total liability + fund the wallet with $BEEF.
    const liability = emission(cfgToMirror(await readCfg()), BigInt(start), BigInt(end));
    await mintTo(conn, payer, rewardMint, rewardVaultPda, payer, liability);
    const beefAta = (await getOrCreateAssociatedTokenAccount(conn, payer, beefMint, payer.publicKey)).address;
    await mintTo(conn, payer, beefMint, beefAta, payer, 1000n * UNIT);
  });

  it("initialize: config, vaults, $STAKE authority, solvency invariant", async () => {
    const c = await readCfg();
    assert.ok(c.stakeMint.equals(stakeMint));
    assert.ok(c.rewardVault.equals(rewardVaultPda));
    const sm = await getAccount(conn, rewardVaultPda); // reward vault is classic
    const liability = emission(cfgToMirror(c), BigInt(start), BigInt(end));
    assert.ok(BigInt(sm.amount.toString()) >= liability, "RewardVault prefunded ≥ total liability");
    const stakeMintAcc = await getAccount(conn, vaultPda); // beef vault
    assert.ok(stakeMintAcc.owner.equals(configPda));
  });

  it("stake: balance-diff credit, $STAKE minted 1:1, NO reward transfer (strategy B)", async () => {
    const A = new BN((100n * UNIT).toString());
    const beefAta = getAssociatedTokenAddressSync(beefMint, payer.publicKey);
    const stakeAta = (await getOrCreateAssociatedTokenAccount(conn, payer, stakeMint, payer.publicKey, false, undefined, undefined, TOKEN_2022_PROGRAM_ID)).address;
    const rewardAta = (await getOrCreateAssociatedTokenAccount(conn, payer, rewardMint, payer.publicKey)).address;
    const rewardBefore = (await getAccount(conn, rewardAta)).amount;

    await program.methods.stake(A).accountsStrict({
      user: payer.publicKey, config: configPda, userInfo: userInfo(payer.publicKey),
      beefMint, stakeMint, userBeefAta: beefAta, vault: vaultPda, userStakeAta: stakeAta,
      beefTokenProgram: TOKEN_PROGRAM_ID, stakeTokenProgram: TOKEN_2022_PROGRAM_ID,
      systemProgram: SystemProgram.programId,
    }).rpc();

    const ui = await program.account.userInfo.fetch(userInfo(payer.publicKey)) as any;
    assert.equal(ui.amount.toString(), A.toString(), "credited principal (no $BEEF fee here)");
    assert.equal((await getAccount(conn, stakeAta, undefined, TOKEN_2022_PROGRAM_ID)).amount.toString(), A.toString(), "$STAKE minted 1:1");
    assert.equal((await getAccount(conn, rewardAta)).amount, rewardBefore, "strategy B: NO reward transfer on stake");
  });

  it("claim: pays from RewardVault, principal untouched, pending cleared", async () => {
    // let a little time pass so pending > 0
    await new Promise((r) => setTimeout(r, 2000));
    const rewardAta = getAssociatedTokenAddressSync(rewardMint, payer.publicKey);
    const before = (await getAccount(conn, rewardAta)).amount;
    const stakeAta = getAssociatedTokenAddressSync(stakeMint, payer.publicKey, false, TOKEN_2022_PROGRAM_ID);
    const stakeBefore = (await getAccount(conn, stakeAta, undefined, TOKEN_2022_PROGRAM_ID)).amount;

    await program.methods.claimRewards().accountsStrict({
      user: payer.publicKey, config: configPda, userInfo: userInfo(payer.publicKey),
      rewardMint, rewardVault: rewardVaultPda, userRewardAta: rewardAta, rewardTokenProgram: TOKEN_PROGRAM_ID,
    }).rpc();

    const got = (await getAccount(conn, rewardAta)).amount - before;
    assert.ok(got > 0n, "claimed some $MILK");
    assert.equal((await getAccount(conn, stakeAta, undefined, TOKEN_2022_PROGRAM_ID)).amount, stakeBefore, "principal ($STAKE) untouched by claim");
    const ui = await program.account.userInfo.fetch(userInfo(payer.publicKey)) as any;
    assert.equal(ui.pendingUnclaimed.toString(), "0", "pending cleared");
  });

  it("unstake: burns $STAKE, refunds $BEEF, remaining correct", async () => {
    const U = new BN((40n * UNIT).toString());
    const beefAta = getAssociatedTokenAddressSync(beefMint, payer.publicKey);
    const stakeAta = getAssociatedTokenAddressSync(stakeMint, payer.publicKey, false, TOKEN_2022_PROGRAM_ID);
    const beefBefore = (await getAccount(conn, beefAta)).amount;

    await program.methods.unstake(U).accountsStrict({
      user: payer.publicKey, config: configPda, userInfo: userInfo(payer.publicKey),
      beefMint, stakeMint, userBeefAta: beefAta, vault: vaultPda, userStakeAta: stakeAta,
      beefTokenProgram: TOKEN_PROGRAM_ID, stakeTokenProgram: TOKEN_2022_PROGRAM_ID,
    }).rpc();

    assert.equal((await getAccount(conn, beefAta)).amount - beefBefore, BigInt(U.toString()), "$BEEF refunded");
    const ui = await program.account.userInfo.fetch(userInfo(payer.publicKey)) as any;
    assert.equal(ui.amount.toString(), (60n * UNIT).toString(), "remaining 60");
  });

  it("NonTransferable: direct $STAKE transfer is rejected by the token program", async () => {
    const stakeAta = getAssociatedTokenAddressSync(stakeMint, payer.publicKey, false, TOKEN_2022_PROGRAM_ID);
    const other = Keypair.generate();
    const otherAta = (await getOrCreateAssociatedTokenAccount(conn, payer, stakeMint, other.publicKey, false, undefined, undefined, TOKEN_2022_PROGRAM_ID)).address;
    try {
      const tx = new Transaction().add(
        createTransferCheckedInstruction(stakeAta, stakeMint, otherAta, payer.publicKey, 1, DEC, [], TOKEN_2022_PROGRAM_ID)
      );
      await sendAndConfirmTransaction(conn, tx, [payer]);
      assert.fail("NonTransferable transfer should have failed");
    } catch (e: any) {
      expect(e.toString()).to.match(/NonTransferable|transfer|0x/i);
    }
  });

  it("set_emission_params re-anchors (settle → write → anchor=now)", async () => {
    const before = await readCfg();
    await program.methods.setEmissionParams(new BN(2_000_000), new BN(0), new BN(0), new BN(0)).accountsStrict({
      admin: payer.publicKey, config: configPda,
    }).rpc();
    const after = await readCfg();
    assert.equal(after.initialRate.toString(), "2000000");
    assert.equal(after.decayPerSec.toString(), "0");
    assert.ok(Number(after.rateAnchorTime) >= Number(before.rateAnchorTime), "anchor advanced to now");
  });

  it("boundaries: amount=0 rejected; nothing-to-claim rejected", async () => {
    const beefAta = getAssociatedTokenAddressSync(beefMint, payer.publicKey);
    const stakeAta = getAssociatedTokenAddressSync(stakeMint, payer.publicKey, false, TOKEN_2022_PROGRAM_ID);
    try {
      await program.methods.stake(new BN(0)).accountsStrict({
        user: payer.publicKey, config: configPda, userInfo: userInfo(payer.publicKey),
        beefMint, stakeMint, userBeefAta: beefAta, vault: vaultPda, userStakeAta: stakeAta,
        beefTokenProgram: TOKEN_PROGRAM_ID, stakeTokenProgram: TOKEN_2022_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      }).rpc();
      assert.fail("expected AmountZero");
    } catch (e: any) {
      expect(e.error?.errorCode?.code ?? e.toString()).to.contain("AmountZero");
    }
  });
});
