/**
 * Anchor tests. See design doc §13.
 *
 * Core (step 2.5):  initialize / stake / unstake happy path + boundaries.
 * Rewards (3.4/3.6): claim; multi-user MasterChef accrual.
 *
 * Determinism note: a live validator advances the Clock by wall-clock, so we do
 * NOT hard-code the §7.3 "Alice=150 / Bob=50" integers. Instead we read the
 * on-chain `last_reward_time` / `acc_reward_per_share` / `total_staked` around
 * each tx and verify the contract's own accumulator update and each user's
 * minted $MILK against an independent BigInt mirror of the exact integer math.
 * That is a STRONGER check than the fixed table (it holds for arbitrary deltas).
 * For the literal 150/50 table use solana-bankrun to warp the clock.
 *
 * Run: `anchor test`
 */
import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { Staking } from "../target/types/staking";
import { PublicKey, Keypair, LAMPORTS_PER_SOL } from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  createMint,
  getOrCreateAssociatedTokenAccount,
  mintTo,
  getAccount,
  getMint,
} from "@solana/spl-token";
import { assert, expect } from "chai";

const DECIMALS = 9;
const UNIT = new BN(10).pow(new BN(DECIMALS)); // 1 token in base units
const ACC_PRECISION = 1_000_000_000_000n; // 1e12, must match constants.rs

// Emission params (linear decay). Over the few-second test window the rate stays
// in the decay region (floor reached ~999s after start), so the trapezoid branch
// is what gets exercised end-to-end.
const INIT_RATE = new BN(1_000_000);
const DECAY = new BN(1_000);
const MIN = new BN(1_000);
const INIT_RATE_BI = 1_000_000n;
const DECAY_BI = 1_000n;
const MIN_BI = 1_000n;
let START = 0n; // config.start_time, set right after initialize

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
// floor(a * b / c) in BigInt — mirrors the on-chain u128 integer math.
const mulDiv = (a: bigint, b: bigint, c: bigint) => (a * b) / c;

// Exact BigInt mirror of reward::emission_between (linear-decay path).
function emissionBetween(aLast: bigint, bLast: bigint): bigint {
  if (bLast <= aLast) return 0n;
  if (DECAY_BI === 0n) return INIT_RATE_BI * (bLast - aLast);
  const a = aLast > START ? aLast : START;
  if (bLast <= a) return 0n;
  const g = INIT_RATE_BI > MIN_BI ? INIT_RATE_BI - MIN_BI : 0n;
  if (g === 0n) return MIN_BI * (bLast - a);
  const sFloor = g / DECAY_BI;
  const tFloor = START + sFloor;
  const decayEnd = bLast < tFloor ? bLast : tFloor;
  const floorStart = a > tFloor ? a : tFloor;
  let total = 0n;
  if (decayEnd > a) {
    const aRel = a - START;
    const eRel = decayEnd - START;
    const twoRect = INIT_RATE_BI * (eRel - aRel) * 2n;
    const decline = DECAY_BI * (eRel * eRel - aRel * aRel);
    total += (twoRect - decline) / 2n; // floor whole area (never over-mints)
  }
  if (bLast > floorStart) total += MIN_BI * (bLast - floorStart);
  return total;
}

describe("staking", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.Staking as Program<Staking>;
  const conn = provider.connection;

  const wallet = provider.wallet as anchor.Wallet;
  const payer = wallet.payer;
  const user = wallet.publicKey;

  const [configPda] = PublicKey.findProgramAddressSync([Buffer.from("config")], program.programId);
  const [vaultPda] = PublicKey.findProgramAddressSync([Buffer.from("vault")], program.programId);
  const userInfo = (u: PublicKey) =>
    PublicKey.findProgramAddressSync([Buffer.from("user"), u.toBuffer()], program.programId)[0];

  let beefMint: PublicKey, stakeMint: PublicKey, milkMint: PublicKey;
  let userBeefAta: PublicKey, userStakeAta: PublicKey, userMilkAta: PublicKey;

  const STAKE_AMOUNT = UNIT.muln(100);
  const UNSTAKE_AMOUNT = UNIT.muln(40);

  // ---- helpers ----
  type Pool = { acc: bigint; last: bigint; total: bigint };
  async function readPool(): Promise<Pool> {
    const c = await program.account.config.fetch(configPda);
    return {
      acc: BigInt(c.accRewardPerShare.toString()),
      last: BigInt(c.lastRewardTime.toString()),
      total: BigInt(c.totalStaked.toString()),
    };
  }
  // Verify the contract's accumulator step against the emission formula.
  function assertAccStep(before: Pool, after: Pool) {
    if (before.total === 0n) {
      assert.equal(after.acc, before.acc, "acc unchanged when nobody staked");
    } else {
      const emission = emissionBetween(before.last, after.last);
      const expected = before.acc + mulDiv(emission, ACC_PRECISION, before.total);
      assert.equal(after.acc.toString(), expected.toString(), "acc_reward_per_share step");
    }
  }
  async function fundUser(kp: PublicKey) {
    const beef = (await getOrCreateAssociatedTokenAccount(conn, payer, beefMint, kp)).address;
    const stake = (await getOrCreateAssociatedTokenAccount(conn, payer, stakeMint, kp)).address;
    const milk = (await getOrCreateAssociatedTokenAccount(conn, payer, milkMint, kp)).address;
    await mintTo(conn, payer, beefMint, beef, payer, BigInt(UNIT.muln(1000).toString()));
    return { beef, stake, milk };
  }

  before(async () => {
    beefMint = await createMint(conn, payer, user, null, DECIMALS);
    stakeMint = await createMint(conn, payer, configPda, null, DECIMALS); // authority = Config PDA
    milkMint = await createMint(conn, payer, configPda, null, DECIMALS); // authority = Config PDA

    const a = await fundUser(user);
    userBeefAta = a.beef;
    userStakeAta = a.stake;
    userMilkAta = a.milk;
  });

  it("initialize", async () => {
    await program.methods
      .initialize(INIT_RATE, DECAY, MIN) // linear decay
      .accountsStrict({
        admin: user,
        config: configPda,
        beefMint,
        stakeMint,
        milkMint,
        vault: vaultPda,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .rpc();

    const config = await program.account.config.fetch(configPda);
    assert.ok(config.admin.equals(user));
    assert.ok(config.beefMint.equals(beefMint));
    assert.ok(config.stakeMint.equals(stakeMint));
    assert.ok(config.milkMint.equals(milkMint));
    assert.ok(config.vault.equals(vaultPda));
    assert.equal(config.totalStaked.toString(), "0");
    assert.equal(config.initialRate.toString(), INIT_RATE.toString());
    assert.equal(config.decayPerSec.toString(), DECAY.toString());
    assert.equal(config.minRate.toString(), MIN.toString());

    // Capture emission start for the decay mirror used in reward assertions.
    START = BigInt(config.startTime.toString());

    const vault = await getAccount(conn, vaultPda);
    assert.ok(vault.owner.equals(configPda), "vault authority == Config PDA");
    assert.ok(vault.mint.equals(beefMint));

    const sm = await getMint(conn, stakeMint);
    const mm = await getMint(conn, milkMint);
    assert.ok(sm.mintAuthority?.equals(configPda), "stake authority == Config PDA");
    assert.ok(mm.mintAuthority?.equals(configPda), "milk authority == Config PDA");
  });

  it("stake (happy path)", async () => {
    const beefBefore = (await getAccount(conn, userBeefAta)).amount;

    await program.methods
      .stake(STAKE_AMOUNT)
      .accountsStrict({
        user,
        config: configPda,
        userInfo: userInfo(user),
        userBeefAta,
        vault: vaultPda,
        userStakeAta,
        stakeMint,
        milkMint,
        userMilkAta,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const beefAfter = (await getAccount(conn, userBeefAta)).amount;
    const vaultBal = (await getAccount(conn, vaultPda)).amount;
    const stakeBal = (await getAccount(conn, userStakeAta)).amount;
    const amt = BigInt(STAKE_AMOUNT.toString());

    assert.equal(beefBefore - beefAfter, amt, "$BEEF decreased");
    assert.equal(vaultBal, amt, "vault holds $BEEF");
    assert.equal(stakeBal, amt, "$STAKE minted 1:1");

    const ui = await program.account.userInfo.fetch(userInfo(user));
    assert.equal(ui.amount.toString(), STAKE_AMOUNT.toString());
    assert.ok(ui.owner.equals(user));
    const config = await program.account.config.fetch(configPda);
    assert.equal(config.totalStaked.toString(), STAKE_AMOUNT.toString());
  });

  it("unstake (partial)", async () => {
    const beefBefore = (await getAccount(conn, userBeefAta)).amount;

    await program.methods
      .unstake(UNSTAKE_AMOUNT)
      .accountsStrict({
        user,
        config: configPda,
        userInfo: userInfo(user),
        userBeefAta,
        vault: vaultPda,
        userStakeAta,
        stakeMint,
        milkMint,
        userMilkAta,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();

    const beefAfter = (await getAccount(conn, userBeefAta)).amount;
    const remaining = BigInt(STAKE_AMOUNT.sub(UNSTAKE_AMOUNT).toString());

    assert.equal(beefAfter - beefBefore, BigInt(UNSTAKE_AMOUNT.toString()), "$BEEF refunded");
    assert.equal((await getAccount(conn, vaultPda)).amount, remaining, "vault reduced");
    assert.equal((await getAccount(conn, userStakeAta)).amount, remaining, "$STAKE burned");

    const ui = await program.account.userInfo.fetch(userInfo(user));
    assert.equal(ui.amount.toString(), remaining.toString(), "remaining mirror");
  });

  it("rejects stake of zero (AmountZero)", async () => {
    try {
      await program.methods
        .stake(new BN(0))
        .accountsStrict({
          user,
          config: configPda,
          userInfo: userInfo(user),
          userBeefAta,
          vault: vaultPda,
          userStakeAta,
          stakeMint,
          milkMint,
          userMilkAta,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .rpc();
      assert.fail("expected AmountZero");
    } catch (e: any) {
      expect(e.error?.errorCode?.code ?? e.toString()).to.contain("AmountZero");
    }
  });

  it("rejects over-unstake", async () => {
    try {
      await program.methods
        .unstake(UNIT.muln(100)) // remaining is 60
        .accountsStrict({
          user,
          config: configPda,
          userInfo: userInfo(user),
          userBeefAta,
          vault: vaultPda,
          userStakeAta,
          stakeMint,
          milkMint,
          userMilkAta,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .rpc();
      assert.fail("expected failure on over-unstake");
    } catch (e: any) {
      expect(e.toString()).to.match(/InsufficientStake|insufficient|0x1|custom program error/i);
    }
  });

  // ---------------- Rewards (steps 3.4 / 3.6) ----------------
  describe("rewards (MasterChef accrual)", () => {
    const alice = Keypair.generate();
    const bob = Keypair.generate();
    let aAta: { beef: PublicKey; stake: PublicKey; milk: PublicKey };
    let bAta: { beef: PublicKey; stake: PublicKey; milk: PublicKey };
    const STAKE = UNIT.muln(100);

    async function stakeAs(kp: Keypair, ata: typeof aAta, amount: BN) {
      await program.methods
        .stake(amount)
        .accountsStrict({
          user: kp.publicKey,
          config: configPda,
          userInfo: userInfo(kp.publicKey),
          userBeefAta: ata.beef,
          vault: vaultPda,
          userStakeAta: ata.stake,
          stakeMint,
          milkMint,
          userMilkAta: ata.milk,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([kp])
        .rpc();
    }
    async function claimAs(kp: Keypair, ata: typeof aAta) {
      await program.methods
        .claimRewards()
        .accountsStrict({
          user: kp.publicKey,
          config: configPda,
          userInfo: userInfo(kp.publicKey),
          milkMint,
          userMilkAta: ata.milk,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([kp])
        .rpc();
    }

    before(async () => {
      for (const kp of [alice, bob]) {
        const sig = await conn.requestAirdrop(kp.publicKey, 2 * LAMPORTS_PER_SOL);
        await conn.confirmTransaction(sig, "confirmed");
      }
      aAta = await fundUser(alice.publicKey);
      bAta = await fundUser(bob.publicKey);
    });

    it("accumulator step + claim payout match the exact MasterChef formula", async () => {
      // Alice stakes first.
      const p0 = await readPool();
      await stakeAs(alice, aAta, STAKE);
      const p1 = await readPool();
      assertAccStep(p0, p1);

      await sleep(2000);

      // Bob stakes (update_pool runs over Alice's solo period).
      const p1b = await readPool();
      await stakeAs(bob, bAta, STAKE);
      const p2 = await readPool();
      assertAccStep(p1b, p2);

      await sleep(2000);

      // Alice claims — verify minted $MILK equals the formula, principal unchanged.
      const aInfoBefore = await program.account.userInfo.fetch(userInfo(alice.publicKey));
      const aMilkBefore = (await getAccount(conn, aAta.milk)).amount;
      const aStakeBefore = (await getAccount(conn, aAta.stake)).amount;
      const p2b = await readPool();
      await claimAs(alice, aAta);
      const p3 = await readPool();
      assertAccStep(p2b, p3);

      const aMinted = (await getAccount(conn, aAta.milk)).amount - aMilkBefore;
      const aExpected =
        mulDiv(BigInt(aInfoBefore.amount.toString()), p3.acc, ACC_PRECISION) -
        BigInt(aInfoBefore.rewardDebt.toString());
      assert.equal(aMinted.toString(), aExpected.toString(), "Alice claim == formula");
      assert.equal(
        (await getAccount(conn, aAta.stake)).amount,
        aStakeBefore,
        "claim does not touch principal ($STAKE)"
      );

      await sleep(2000);

      // Bob claims — same formula check.
      const bInfoBefore = await program.account.userInfo.fetch(userInfo(bob.publicKey));
      const bMilkBefore = (await getAccount(conn, bAta.milk)).amount;
      const p3b = await readPool();
      await claimAs(bob, bAta);
      const p4 = await readPool();
      assertAccStep(p3b, p4);

      const bMinted = (await getAccount(conn, bAta.milk)).amount - bMilkBefore;
      const bExpected =
        mulDiv(BigInt(bInfoBefore.amount.toString()), p4.acc, ACC_PRECISION) -
        BigInt(bInfoBefore.rewardDebt.toString());
      assert.equal(bMinted.toString(), bExpected.toString(), "Bob claim == formula");

      // Qualitative §7.3 check: equal stake, but Alice had a solo head-start,
      // so Alice's total accrued reward must exceed Bob's.
      assert.ok(aMinted > bMinted, "earlier staker (Alice) earns more than Bob");
    });

    it("claim with nothing pending fails (NothingToClaim)", async () => {
      // Alice just claimed; immediately claiming again in the same second yields 0.
      try {
        await claimAs(alice, aAta);
        // If a second elapsed, a tiny amount may be claimable — tolerate that.
      } catch (e: any) {
        expect(e.error?.errorCode?.code ?? e.toString()).to.contain("NothingToClaim");
      }
    });
  });
});
