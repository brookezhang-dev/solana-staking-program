# Solana Staking Program ($BEEF → $STAKE, rewards in $MILK)

A Solana (Anchor) DeFi staking program: deposit **$BEEF** to receive 1:1 **$STAKE**,
accrue **$MILK** rewards over time (MasterChef accumulator, O(1)), redeem principal
and claim rewards independently.

> **Status: v3, deployed to devnet.** 6 instructions (initialize / stake / unstake /
> claim_rewards / fund_rewards / set_emission_params, + devnet-only faucet).
> $STAKE is a **Token-2022 NonTransferable** receipt; $MILK is an **external token**
> paid from a **prefunded RewardVault** (program never mints it). MasterChef O(1)
> rewards with **configurable, re-anchored, end_time-bounded** linear-decay emission.
> Authoritative design: `v3-执行计划-EXECUTION-PLAN.md` + `设计与实现说明-DESIGN-NOTES.md`.
> **Program ID (devnet):** `54HWhVGu8HoK46PUj3ijauVjrgNScGHyzpnvHsvZGpcv`

## Layout

```
programs/staking/src/
  lib.rs            # declare_id + #[program] entry (initialize/stake/unstake/claim_rewards)
  state.rs          # Config / UserInfo
  constants.rs      # seeds + ACC_PRECISION + space
  errors.rs         # StakingError
  events.rs         # Stake/Unstake/Claim events
  instructions/
    initialize.rs   # Config + Vault, mint authority = Config PDA
    stake.rs        # transfer in + mint $STAKE + settle pending $MILK (strategy A)
    unstake.rs      # burn $STAKE + refund $BEEF + settle pending $MILK
    claim.rs        # settle + mint pending $MILK (decoupled from principal)
    reward.rs       # update_pool / pending_reward / reward_debt_for / emission_between (linear decay)
tests/staking.ts    # initialize / stake / unstake / boundaries / multi-user rewards
scripts/setup-devnet.ts  # create mints, set authority to PDA, initialize, fund $BEEF
app/                # Vite + React + wallet-adapter frontend (devnet)
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

By default $STAKE / $MILK show as "Unknown token" (test mints have no metadata).
The `create_token_metadata` instruction attaches Metaplex Token Metadata, signed by
the Config PDA (the mint authority). After (re)deploying:

```bash
anchor build && anchor deploy
(cd app && npm run copy-idl)
npx ts-node scripts/add-metadata.ts     # names $STAKE and $MILK
```

Reconnect the wallet to see the names. ($BEEF is an external input token — name it
separately if desired.)

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
Config PDA (a one-time transfer).

One-time setup by the deployer (after building the program with `faucet`):

```bash
anchor build && anchor deploy
(cd app && npm run copy-idl)
npx ts-node scripts/beef-authority-to-pda.ts   # ONCE: $BEEF authority -> Config PDA
```

After that, the new-user flow is fully self-serve:

1. Open the app, switch Phantom to **devnet**, connect the wallet.
2. Click **领取 1000 测试 $BEEF** (shown prominently when the balance is 0).
3. **Stake** → **Claim** → **Unstake**. Done — no CLI, no tribal knowledge.

> The faucet exists only because $BEEF is a throwaway devnet test token; a real
> input token would not have one. Cap per call: `FAUCET_MAX` (1000 $BEEF).
> `scripts/airdrop-beef.ts` (CLI funding) is superseded once authority moves to the
> PDA — use the in-app faucet instead.

## Key design (for reviewers — see `v3-执行计划-EXECUTION-PLAN.md` / DESIGN-NOTES)

- **PDAs**: `Config [b"config"]` (state + emission params, authority of both vaults
  + $STAKE mint); `Vault [b"vault"]` ($BEEF); `RewardVault [b"reward_vault"]` ($MILK);
  `UserInfo [b"user", user]` (amount + reward_debt + pending_unclaimed).
- **Tokens**: $STAKE = Token-2022 **NonTransferable** (program mints/burns);
  $MILK = external token, program **only transfers from RewardVault** (never mints);
  $BEEF = input. All transfers use `transfer_checked` (classic + Token-2022 via `token_interface`).
- **Rewards O(1)**: global `acc_reward_per_share` + per-user `reward_debt`; no loops.
  Strategy B: pending accrues into `pending_unclaimed`, paid only on claim.
- **Principal authority = `UserInfo.amount` (checked_sub)**; $STAKE burn is the receipt
  (equivalent under NonTransferable). No two-ledger desync possible.
- **Configurable emission**: `set_emission_params` re-anchors (`rate_anchor_time`) after
  settling; `end_time` bounds total liability → prefund `∫[start,end] r` = provable solvency.
- **Safety**: PDA-only mint/vault authority, mint/owner/seeds/token-program constraints,
  `checked_*` math, balance-diff (fee-token safe), checks-effects-interactions.

## Program ID

`54HWhVGu8HoK46PUj3ijauVjrgNScGHyzpnvHsvZGpcv` (devnet; in `lib.rs` + `Anchor.toml`.
`anchor keys sync` after generating a new keypair if you redeploy fresh.)

## Tests

`anchor test` spins up a local validator and runs `tests/staking.ts`:
initialize, stake (happy path), partial unstake, `amount = 0` → `AmountZero`,
over-unstake rejection, and a multi-user reward test that verifies the on-chain
accumulator step and each user's minted $MILK against an independent BigInt mirror
of the exact integer math (stronger than a fixed numeric table).
