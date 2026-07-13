# Solana Staking Program ($BEEF → $STAKE, rewards in $MILK)

**🔗 Live demo (devnet): https://app-inky-seven-86.vercel.app/**

A Solana (Anchor) DeFi staking program: deposit **$BEEF** to receive 1:1 **$STAKE**,
accrue **$MILK** rewards over time (MasterChef accumulator, O(1)), redeem principal
and claim rewards independently.

> **Status: v3.x, deployed to devnet.** Config/Pool two-tier architecture — one
> deployment hosts many pools. ~15 instructions: protocol (`initialize_config`,
> `set_pause`, `transfer_admin`), pool lifecycle (`create_pool`, `set_emission`,
> `withdraw_surplus`, `initialize_extra_account_meta_list`), user actions (`stake`,
> `unstake`, `claim_rewards`, `fund_rewards`, `register`, `close_user_info`), the
> Transfer Hook callback (`transfer_hook`), plus a devnet-only `faucet`.
> **$STAKE is TRANSFERABLE** — a Token-2022 **TransferHook** certificate; the
> token-account **balance is the ledger authority** and every transfer routes
> through the program's own hook, which settles both parties' rewards atomically.
> $MILK is an **external token** paid from a **prefunded per-pool RewardVault**
> (program never mints it). MasterChef O(1) rewards with configurable constant-rate,
> end_time-bounded emission.

## Architecture (v3.x — two-tier)

One deployment, many pools (Raydium `AmmConfig→Pool` / Orca `WhirlpoolsConfig→Whirlpool`):

```
Config (singleton, [b"config"])            admin · pool_count · paused
  └── Pool ([b"pool", config, staked_mint, reward_mint])   per-pair accounting
        ├── staked_vault  [b"staked_vault", pool]     (authority = Pool PDA)
        ├── reward_vault  [b"reward_vault", pool]      (authority = Pool PDA, prefunded)
        ├── $STAKE mint   (Token-2022 TransferHook; mint authority = Pool PDA)
        └── user_info     [b"user_info", pool, stake_token_account]
```

| Caller | Can call |
|---|---|
| **Admin** (`config.admin`) | `create_pool`, `set_emission`, `withdraw_surplus`, `set_pause`, `transfer_admin` |
| **Anyone** | `fund_rewards` (in-only), `register`, `initialize_extra_account_meta_list` (no `has_one = admin` check; harmless once-only setup, `init` fails on a second call) |
| **User** (own accounts) | `stake`, `unstake`, `claim_rewards`, `close_user_info` |
| **Token-2022** (CPI) | `transfer_hook` (settles rewards on every $STAKE transfer) |

Pool address = `findPDA([b"pool", config, staked_mint, reward_mint])` — clients derive it
statelessly; duplicates for a pair are impossible (PDA collision). Every user instruction
binds all child accounts to the pool (`pool.config == config`, pool-scoped vault/mint/user_info)
for cross-pool isolation.

## Live demo

- **App (devnet):** https://app-inky-seven-86.vercel.app/
- **Program (devnet):** `54HWhVGu8HoK46PUj3ijauVjrgNScGHyzpnvHsvZGpcv`
  · [Solscan](https://solscan.io/account/54HWhVGu8HoK46PUj3ijauVjrgNScGHyzpnvHsvZGpcv?cluster=devnet)
- **Docs (match current code):** `CHANGELOG.md` (what changed and why),
  `全项目实现说明-CODE-WALKTHROUGH.md` and `实现讲解-HOW-IT-WORKS.md` (code-level
  walkthrough of the two-tier architecture + Transfer Hook).
  > ⚠️ `Solana质押程序-技术设计文档-v3.md` predates the Config/Pool two-tier +
  > Transfer Hook rework and describes an older (single-pool, non-transferable
  > $STAKE) design — do not use it as a reference for the current code.

Quick start: open the app, switch Phantom to **devnet**, connect a wallet, click
**领取测试 $BEEF** (self-serve faucet), then Stake → Claim → Unstake.

## Layout

```
programs/staking/src/
  lib.rs              # declare_id + #[program] entry: routes every instruction to its
                       #   handler below, plus fallback() dispatching Token-2022's hook Execute
  state.rs            # Config (protocol singleton) / Pool (per mint-pair) / UserInfo (per $STAKE account)
  constants.rs        # PDA seeds + ACC_PRECISION + faucet cap
  errors.rs           # StakingError
  events.rs           # PoolCreated/Stake/Unstake/Claim/Fund/EmissionSet/... events
  instructions/
    initialize.rs     # initialize_config — one-time protocol Config (admin, pool_count, paused)
    create_pool.rs    # create_pool — admin-gated: new Pool + its two vaults for a mint pair
    admin.rs          # set_pause / transfer_admin / set_emission / withdraw_surplus (admin-gated)
    stake.rs          # transfer $BEEF in + mint $STAKE + settle pending (strategy B: record only)
    unstake.rs        # burn $STAKE + refund $BEEF + settle pending (never blocked by pause)
    claim.rs          # settle + pay pending_unclaimed from the prefunded RewardVault (no minting)
    fund.rs           # fund_rewards — anyone tops up a pool's RewardVault (in-only)
    register.rs       # register — create UserInfo for a $STAKE account (required before it can receive)
    close_user.rs     # close_user_info — reclaim rent once balance == 0 && pending_unclaimed == 0
    hook.rs           # initialize_extra_account_meta_list + transfer_hook execute (settles both
                       #   parties' rewards on every $STAKE transfer)
    token_ext.rs       # Token-2022 extension checks (TransferHook presence/target)
    reward.rs          # update_pool / pending_reward / reward_debt_for / emission_between (constant rate)
    faucet.rs           # devnet-only self-serve $BEEF faucet (feature-gated, off for mainnet builds)
tests/staking.ts      # protocol/pool lifecycle, cross-pool isolation, pause semantics, transfer-hook settlement
scripts/setup-devnet.ts   # create mints (incl. $STAKE's Token-2022 metadata + TransferHook), hand
                          #   authorities to the Pool PDA, initialize_config, create_pool, prefund RewardVault
app/                  # Vite + React + wallet-adapter frontend (devnet)
```

## Build / Test / Run

```bash
# 1. install deps
yarn            # or npm install

# 2. set program id (after first build)
anchor keys list                 # copy the staking program id
#   -> paste into declare_id!() in lib.rs AND [programs.*] in Anchor.toml

# 3. build + test on a local validator
anchor build
anchor test

# 4. devnet
solana config set --url devnet
solana airdrop 2
anchor deploy
npx ts-node scripts/setup-devnet.ts   # creates mints, sets authority to PDA, initializes
```

## Token names in wallets (optional)

$STAKE now gets a name/symbol automatically: `scripts/setup-devnet.ts` creates it with
Token-2022's native **Metadata + MetadataPointer** extensions set at mint-creation time
(no separate on-chain instruction or script needed — the program does not carry a
`create_token_metadata` handler in v3.x). $MILK is meant to be an arbitrary external
reward token (any existing SPL/Token-2022 mint), so this repo doesn't name it; $BEEF is
likewise an external input token — name either separately if you want, outside this repo.

## Frontend (devnet)

A Vite + React + wallet-adapter app lives in `app/` (stage 4). After deploying and
running `scripts/setup-devnet.ts`, paste the printed mint addresses into
`app/src/config.ts`, then:

```bash
cd app && npm install && npm run dev
```

Connect Phantom (devnet) and use Stake / Unstake / Claim; each tx links to Solscan.
See `app/README.md` for details.

## Trying it with ANY wallet (self-serve faucet)

A brand-new wallet has 0 $BEEF, so it can't stake yet. The program exposes a
permissionless, capped `faucet` instruction so anyone can mint test $BEEF from the
UI — no manual funding needed. This requires the $BEEF mint authority to be the
**Pool PDA** for that pool; `scripts/setup-devnet.ts` already does this one-time
transfer as part of its normal setup (step 4), so no separate script is needed.

The new-user flow is fully self-serve once a pool exists:

1. Open the app, switch Phantom to **devnet**, connect the wallet.
2. Click **领取 1000 测试 $BEEF** (shown prominently when the balance is 0).
3. **Stake** → **Claim** → **Unstake**. Done — no CLI, no tribal knowledge.

> The faucet exists only because $BEEF is a throwaway devnet test token; a real
> input token would not have one. Cap per call: `FAUCET_MAX` (1000 $BEEF).
> `scripts/airdrop-beef.ts` (CLI funding) is an alternative if you'd rather fund a
> wallet from the CLI than click the in-app faucet.

## Key design (for reviewers — see `CHANGELOG.md` / `全项目实现说明-CODE-WALKTHROUGH.md` / `实现讲解-HOW-IT-WORKS.md`)

- **Two-tier PDAs**: `Config [b"config"]` (protocol singleton — `admin`, `pool_count`,
  global `paused`; holds no per-pool state) → `Pool [b"pool", config, staked_mint,
  reward_mint]` (one per mint pair; owns `staked_vault [b"staked_vault", pool]`,
  `reward_vault [b"reward_vault", pool]`, and is the mint authority of that pool's
  $STAKE) → `UserInfo [b"user_info", pool, stake_token_account]` (reward bookkeeping
  only, keyed by token account so the transfer hook can settle both sides of a transfer).
  The **Pool PDA** — not Config — signs for its vaults and $STAKE mint.
- **Tokens**: $STAKE = Token-2022 **TransferHook** certificate, one distinct mint per
  pool (transferable; program mints/burns); $MILK = external token, program **only
  transfers from that pool's RewardVault** (never mints); $BEEF = input. All transfers
  use `transfer_checked` via `token_interface`.
- **Rewards O(1)**: per-pool `acc_reward_per_share` + per-account `reward_debt`; no loops.
  Strategy B: pending accrues into `pending_unclaimed`, paid only on `claim_rewards`
  (stake/unstake never touch the RewardVault, so principal ops can't be blocked by it).
- **Principal authority = the $STAKE token-account BALANCE** (no separate ledger field).
  Every transfer CPIs the program's own hook `execute` (via `fallback`), which settles
  both parties' `pending_unclaimed`/`reward_debt` against the pool's accumulator — so
  balance and reward bookkeeping can never desync. **Register-before-receive**: a token
  account must call `register` (creates its `UserInfo`) before it can receive a transfer;
  `stake` auto-registers the staker via `init_if_needed`.
- **Cross-pool isolation**: every user/admin instruction binds all child accounts back to
  one `pool` (`pool.config == config`, `address = pool.staked_vault`/`pool.reward_vault`,
  pool-scoped `user_info` seeds) — a vault or user_info from a different pool is rejected
  by account constraints before the handler runs.
- **Configurable emission**: `set_emission` settles at the OLD rate (`update_pool`) before
  writing the new `reward_per_sec`/`end_time`, so a rate change only affects the future;
  `end_time` bounds total liability → prefund `∫[start,end] r` = provable solvency.
  `withdraw_surplus` can only pull `reward_vault.amount − (total_emitted − total_claimed)`.
- **Admin power is bounded**: admin can create pools / retune emission / pause / withdraw
  surplus / transfer adminship, but can never touch vault principal, freeze `unstake`
  (pause blocks `stake`/`claim`, never `unstake`), or claw back already-settled rewards.
- **Safety**: PDA-only mint/vault authority, mint/owner/seeds/token-program constraints,
  `checked_*` math, balance-diff accounting (fee-token safe), checks-effects-interactions.

## Program ID

`54HWhVGu8HoK46PUj3ijauVjrgNScGHyzpnvHsvZGpcv` (devnet; in `lib.rs` + `Anchor.toml`.
`anchor keys sync` after generating a new keypair if you redeploy fresh.)

## Tests

`anchor test` spins up a local validator and runs `tests/staking.ts`. Coverage:

- **Core happy path**: stake mints 1:1, claim pays out pending, unstake burns + refunds.
- **Two-tier / pool lifecycle**: create_pool by a non-admin fails (T-P1); two pools for
  different mint pairs track totals independently (T-P2); a duplicate pool for the same
  pair fails via PDA collision (T-P3); **cross-pool isolation** — unstaking on pool A
  while passing pool B's vault is rejected by account constraints (T-P4); global pause
  blocks stake/claim but not unstake, then unpause restores (T-P5); `transfer_admin`
  moves admin rights and the old admin is rejected afterwards (T-P7).
- **Transfer Hook**: a transfer between two `register`-ed $STAKE accounts settles both
  parties' rewards atomically (verified via the sender's `pending_unclaimed` increasing).

Reward-math assertions read the on-chain accumulator/timestamps and mirror the exact
integer arithmetic in BigInt on the client side, rather than hardcoding a numeric table.
