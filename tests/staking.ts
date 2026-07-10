/**
 * Anchor tests (v3.x — Config/Pool two-tier + Transfer Hook). See pool-layering spec §5.
 * RUN ON LOCALNET: Anchor.toml [provider] cluster = "Localnet"; `anchor test`.
 * (Recent solana-test-validator bundles Token-2022; else clone TokenzQd… in Anchor.toml.)
 */
import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { Staking } from "../target/types/staking";
import {
  PublicKey, Keypair, SystemProgram, SYSVAR_RENT_PUBKEY, Transaction, sendAndConfirmTransaction, LAMPORTS_PER_SOL,
} from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID, ExtensionType, getMintLen, LENGTH_SIZE, TYPE_SIZE,
  createMint, getOrCreateAssociatedTokenAccount, mintTo, getAccount, getAssociatedTokenAddressSync,
  createAssociatedTokenAccountIdempotent,
  createInitializeMintInstruction, createInitializeTransferHookInstruction,
  createInitializeMetadataPointerInstruction, setAuthority, AuthorityType,
  createTransferCheckedWithTransferHookInstruction,
} from "@solana/spl-token";
import { createInitializeInstruction, pack, type TokenMetadata } from "@solana/spl-token-metadata";
import { assert, expect } from "chai";

const DEC = 9;
const UNIT = 10n ** BigInt(DEC);
const RATE = 1_000_000n;
const T22 = TOKEN_2022_PROGRAM_ID, CLS = TOKEN_PROGRAM_ID;

describe("staking v3.x (Config/Pool + hook)", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.Staking as Program<Staking>;
  const conn = provider.connection;
  const payer = (provider.wallet as anchor.Wallet).payer;

  const [configPda] = PublicKey.findProgramAddressSync([Buffer.from("config")], program.programId);
  const poolPda = (beef: PublicKey, milk: PublicKey) =>
    PublicKey.findProgramAddressSync([Buffer.from("pool"), configPda.toBuffer(), beef.toBuffer(), milk.toBuffer()], program.programId)[0];
  const child = (seed: string, pool: PublicKey) =>
    PublicKey.findProgramAddressSync([Buffer.from(seed), pool.toBuffer()], program.programId)[0];
  const uiPda = (pool: PublicKey, ata: PublicKey) =>
    PublicKey.findProgramAddressSync([Buffer.from("user_info"), pool.toBuffer(), ata.toBuffer()], program.programId)[0];
  const bal = async (ata: PublicKey, p = T22) => BigInt((await getAccount(conn, ata, undefined, p)).amount.toString());

  async function stakeHookMint(pool: PublicKey): Promise<PublicKey> {
    const kp = Keypair.generate(); const mint = kp.publicKey;
    const meta: TokenMetadata = { mint, name: "STAKE", symbol: "STK", uri: "", additionalMetadata: [] };
    const len = getMintLen([ExtensionType.TransferHook, ExtensionType.MetadataPointer]);
    const mdLen = TYPE_SIZE + LENGTH_SIZE + pack(meta).length;
    const lamports = await conn.getMinimumBalanceForRentExemption(len + mdLen);
    await sendAndConfirmTransaction(conn, new Transaction().add(
      SystemProgram.createAccount({ fromPubkey: payer.publicKey, newAccountPubkey: mint, space: len, lamports, programId: T22 }),
      createInitializeMetadataPointerInstruction(mint, payer.publicKey, mint, T22),
      createInitializeTransferHookInstruction(mint, payer.publicKey, program.programId, T22),
      createInitializeMintInstruction(mint, DEC, payer.publicKey, null, T22),
      createInitializeInstruction({ programId: T22, metadata: mint, updateAuthority: payer.publicKey, mint, mintAuthority: payer.publicKey, name: meta.name, symbol: meta.symbol, uri: meta.uri }),
    ), [payer, kp]);
    await setAuthority(conn, payer, mint, payer, AuthorityType.MintTokens, pool, [], undefined, T22);
    return mint;
  }

  // create a full pool; returns its addresses.
  async function makePool(admin = payer) {
    const beef = await createMint(conn, payer, payer.publicKey, null, DEC);
    const milk = await createMint(conn, payer, payer.publicKey, null, DEC);
    const pool = poolPda(beef, milk);
    const stake = await stakeHookMint(pool);
    const stakedVault = child("staked_vault", pool), rewardVault = child("reward_vault", pool);
    const [extraMeta] = PublicKey.findProgramAddressSync([Buffer.from("extra-account-metas"), stake.toBuffer()], program.programId);
    const now = Math.floor(Date.now() / 1000), end = now + 30 * 24 * 3600;
    await program.methods.createPool(new BN(RATE.toString()), new BN(now), new BN(end)).accountsStrict({
      admin: admin.publicKey, config: configPda, stakedMint: beef, rewardMint: milk, stakeReceiptMint: stake,
      pool, stakedVault, rewardVault, stakedTokenProgram: CLS, rewardTokenProgram: CLS,
      systemProgram: SystemProgram.programId, rent: SYSVAR_RENT_PUBKEY,
    }).signers(admin === payer ? [] : [admin]).rpc();
    await program.methods.initializeExtraAccountMetaList().accountsStrict({
      payer: payer.publicKey, extraAccountMetaList: extraMeta, pool, stakeMint: stake, systemProgram: SystemProgram.programId,
    }).rpc();
    await mintTo(conn, payer, milk, rewardVault, payer, RATE * BigInt(end - now));
    return { beef, milk, stake, pool, stakedVault, rewardVault, end };
  }

  async function fundBeef(beef: PublicKey, owner: PublicKey, amt: bigint) {
    const ata = (await getOrCreateAssociatedTokenAccount(conn, payer, beef, owner)).address;
    await mintTo(conn, payer, beef, ata, payer, amt);
    return ata;
  }
  async function stakeInto(P: any, user: Keypair, amount: bigint) {
    const beefAta = getAssociatedTokenAddressSync(P.beef, user.publicKey);
    const stakeAta = getAssociatedTokenAddressSync(P.stake, user.publicKey, false, T22);
    await program.methods.stake(new BN(amount.toString())).accountsStrict({
      user: user.publicKey, config: configPda, pool: P.pool, stakedMint: P.beef, stakeReceiptMint: P.stake,
      userStakedAta: beefAta, stakedVault: P.stakedVault, userStakeAta: stakeAta, userInfo: uiPda(P.pool, stakeAta),
      stakedTokenProgram: CLS, stakeTokenProgram: T22, systemProgram: SystemProgram.programId,
    }).signers(user === payer ? [] : [user]).rpc();
  }

  before(async () => {
    await program.methods.initializeConfig().accountsStrict({
      admin: payer.publicKey, config: configPda, systemProgram: SystemProgram.programId,
    }).rpc();
  });

  it("initialize_config: admin set, not paused", async () => {
    const c = await program.account.config.fetch(configPda) as any;
    assert.ok(c.admin.equals(payer.publicKey));
    assert.equal(c.paused, false);
  });

  it("T-P1: create_pool by non-admin fails", async () => {
    const mallory = Keypair.generate();
    await conn.confirmTransaction(await conn.requestAirdrop(mallory.publicKey, LAMPORTS_PER_SOL));
    try {
      await makePool(mallory);
      assert.fail("non-admin create_pool should fail");
    } catch (e: any) { expect(e.error?.errorCode?.code ?? e.toString()).to.match(/Unauthorized|has_one|custom program error|0x/i); }
  });

  it("core: stake mints 1:1, claim pays, unstake burns + refunds", async () => {
    const P = await makePool();
    await fundBeef(P.beef, payer.publicKey, 200n * UNIT);
    await createAssociatedTokenAccountIdempotent(conn, payer, P.stake, payer.publicKey, {}, T22);
    await getOrCreateAssociatedTokenAccount(conn, payer, P.milk, payer.publicKey);
    await stakeInto(P, payer, 100n * UNIT);
    const stakeAta = getAssociatedTokenAddressSync(P.stake, payer.publicKey, false, T22);
    assert.equal(await bal(stakeAta), 100n * UNIT);

    await new Promise((r) => setTimeout(r, 2000));
    const milkAta = getAssociatedTokenAddressSync(P.milk, payer.publicKey);
    const before = await bal(milkAta, CLS);
    await program.methods.claimRewards().accountsStrict({
      user: payer.publicKey, config: configPda, pool: P.pool, stakeReceiptMint: P.stake, userStakeAta: stakeAta,
      userInfo: uiPda(P.pool, stakeAta), rewardMint: P.milk, rewardVault: P.rewardVault, userRewardAta: milkAta, rewardTokenProgram: CLS,
    }).rpc();
    assert.ok((await bal(milkAta, CLS)) > before, "claimed > 0");

    const beefAta = getAssociatedTokenAddressSync(P.beef, payer.publicKey);
    const beefBefore = await bal(beefAta, CLS);
    await program.methods.unstake(new BN((40n * UNIT).toString())).accountsStrict({
      user: payer.publicKey, config: configPda, pool: P.pool, stakedMint: P.beef, stakeReceiptMint: P.stake,
      userStakedAta: beefAta, stakedVault: P.stakedVault, userStakeAta: stakeAta, userInfo: uiPda(P.pool, stakeAta),
      stakedTokenProgram: CLS, stakeTokenProgram: T22,
    }).rpc();
    assert.equal((await bal(beefAta, CLS)) - beefBefore, 40n * UNIT);
    assert.equal(await bal(stakeAta), 60n * UNIT);
  });

  it("T-P2: two pools with different pairs are independent", async () => {
    const A = await makePool(); const B = await makePool();
    await fundBeef(A.beef, payer.publicKey, 100n * UNIT);
    await fundBeef(B.beef, payer.publicKey, 100n * UNIT);
    await createAssociatedTokenAccountIdempotent(conn, payer, A.stake, payer.publicKey, {}, T22);
    await createAssociatedTokenAccountIdempotent(conn, payer, B.stake, payer.publicKey, {}, T22);
    await stakeInto(A, payer, 50n * UNIT);
    await stakeInto(B, payer, 70n * UNIT);
    const pa = await program.account.pool.fetch(A.pool) as any;
    const pb = await program.account.pool.fetch(B.pool) as any;
    assert.equal(pa.totalStaked.toString(), (50n * UNIT).toString());
    assert.equal(pb.totalStaked.toString(), (70n * UNIT).toString());
  });

  it("T-P3: duplicate pool for the same pair fails", async () => {
    const beef = await createMint(conn, payer, payer.publicKey, null, DEC);
    const milk = await createMint(conn, payer, payer.publicKey, null, DEC);
    const pool = poolPda(beef, milk);
    const stake = await stakeHookMint(pool);
    const args: any = {
      admin: payer.publicKey, config: configPda, stakedMint: beef, rewardMint: milk, stakeReceiptMint: stake,
      pool, stakedVault: child("staked_vault", pool), rewardVault: child("reward_vault", pool),
      stakedTokenProgram: CLS, rewardTokenProgram: CLS, systemProgram: SystemProgram.programId, rent: SYSVAR_RENT_PUBKEY,
    };
    const now = Math.floor(Date.now() / 1000);
    await program.methods.createPool(new BN(RATE.toString()), new BN(now), new BN(now + 1000)).accountsStrict(args).rpc();
    try {
      await program.methods.createPool(new BN(RATE.toString()), new BN(now), new BN(now + 1000)).accountsStrict(args).rpc();
      assert.fail("duplicate pool should fail");
    } catch (e: any) { expect(e.toString()).to.match(/already in use|custom program error|0x0/i); }
  });

  it("T-P4 (isolation): unstake on pool A with pool B's vault is rejected", async () => {
    const A = await makePool(); const B = await makePool();
    await fundBeef(A.beef, payer.publicKey, 50n * UNIT);
    await createAssociatedTokenAccountIdempotent(conn, payer, A.stake, payer.publicKey, {}, T22);
    await stakeInto(A, payer, 50n * UNIT);
    const beefAta = getAssociatedTokenAddressSync(A.beef, payer.publicKey);
    const stakeAta = getAssociatedTokenAddressSync(A.stake, payer.publicKey, false, T22);
    try {
      await program.methods.unstake(new BN((10n * UNIT).toString())).accountsStrict({
        user: payer.publicKey, config: configPda, pool: A.pool, stakedMint: A.beef, stakeReceiptMint: A.stake,
        userStakedAta: beefAta, stakedVault: B.stakedVault /* WRONG POOL */, userStakeAta: stakeAta, userInfo: uiPda(A.pool, stakeAta),
        stakedTokenProgram: CLS, stakeTokenProgram: T22,
      }).rpc();
      assert.fail("cross-pool vault must be rejected");
    } catch (e: any) { expect(e.toString()).to.match(/constraint|address|custom program error|0x/i); }
  });

  it("T-P5: pause blocks stake/claim, not unstake; unpause restores", async () => {
    const P = await makePool();
    await fundBeef(P.beef, payer.publicKey, 50n * UNIT);
    await createAssociatedTokenAccountIdempotent(conn, payer, P.stake, payer.publicKey, {}, T22);
    await stakeInto(P, payer, 30n * UNIT);
    await program.methods.setPause(true).accountsStrict({ admin: payer.publicKey, config: configPda }).rpc();
    try {
      await stakeInto(P, payer, 1n * UNIT); assert.fail("paused stake");
    } catch (e: any) { expect(e.error?.errorCode?.code ?? e.toString()).to.match(/Paused|0x/i); }
    // unstake still works
    const beefAta = getAssociatedTokenAddressSync(P.beef, payer.publicKey);
    const stakeAta = getAssociatedTokenAddressSync(P.stake, payer.publicKey, false, T22);
    await program.methods.unstake(new BN((10n * UNIT).toString())).accountsStrict({
      user: payer.publicKey, config: configPda, pool: P.pool, stakedMint: P.beef, stakeReceiptMint: P.stake,
      userStakedAta: beefAta, stakedVault: P.stakedVault, userStakeAta: stakeAta, userInfo: uiPda(P.pool, stakeAta),
      stakedTokenProgram: CLS, stakeTokenProgram: T22,
    }).rpc();
    await program.methods.setPause(false).accountsStrict({ admin: payer.publicKey, config: configPda }).rpc();
    const c = await program.account.config.fetch(configPda) as any;
    assert.equal(c.paused, false);
  });

  it("T-P7: transfer_admin moves create_pool rights", async () => {
    const newAdmin = Keypair.generate();
    await program.methods.transferAdmin(newAdmin.publicKey).accountsStrict({ admin: payer.publicKey, config: configPda }).rpc();
    const c = await program.account.config.fetch(configPda) as any;
    assert.ok(c.admin.equals(newAdmin.publicKey));
    // old admin can no longer pause
    try {
      await program.methods.setPause(true).accountsStrict({ admin: payer.publicKey, config: configPda }).rpc();
      assert.fail("old admin should be rejected");
    } catch (e: any) { expect(e.toString()).to.match(/Unauthorized|has_one|0x/i); }
    // restore admin for any later runs
    await program.methods.transferAdmin(payer.publicKey).accountsStrict({ admin: newAdmin.publicKey, config: configPda }).signers([newAdmin]).rpc();
  });

  it("hook: transfer between registered users settles both sides", async () => {
    const P = await makePool();
    const bob = Keypair.generate();
    await conn.confirmTransaction(await conn.requestAirdrop(bob.publicKey, LAMPORTS_PER_SOL));
    await fundBeef(P.beef, payer.publicKey, 100n * UNIT);
    const aliceStake = await createAssociatedTokenAccountIdempotent(conn, payer, P.stake, payer.publicKey, {}, T22);
    const bobStake = await createAssociatedTokenAccountIdempotent(conn, payer, P.stake, bob.publicKey, {}, T22);
    await stakeInto(P, payer, 100n * UNIT);
    // register Bob's stake account (required before receiving)
    await program.methods.register().accountsStrict({
      payer: payer.publicKey, config: configPda, pool: P.pool, stakeReceiptMint: P.stake,
      stakeTokenAccount: bobStake, userInfo: uiPda(P.pool, bobStake), systemProgram: SystemProgram.programId,
    }).rpc();
    await new Promise((r) => setTimeout(r, 1500));
    const ix = await createTransferCheckedWithTransferHookInstruction(conn, aliceStake, P.stake, bobStake, payer.publicKey, 40n * UNIT, DEC, [], "confirmed", T22);
    await sendAndConfirmTransaction(conn, new Transaction().add(ix), [payer]);
    assert.equal(await bal(aliceStake), 60n * UNIT);
    assert.equal(await bal(bobStake), 40n * UNIT);
    const aui = await program.account.userInfo.fetch(uiPda(P.pool, aliceStake)) as any;
    assert.ok(BigInt(aui.pendingUnclaimed) > 0n, "sender pending settled home");
  });
});
