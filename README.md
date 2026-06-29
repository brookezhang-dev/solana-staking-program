# Solana Staking Program ($BEEF → $STAKE, rewards in $MILK)

A Solana (Anchor) DeFi staking program: deposit **$BEEF** to receive 1:1 **$STAKE**,
accrue **$MILK** rewards over time (MasterChef accumulator, O(1)), redeem principal
and claim rewards independently.

> **Status: feature-complete.** initialize / stake / unstake / claim_rewards are
> implemented with MasterChef O(1) rewards and linear-decay emission. Tests cover
> the happy paths, boundaries, and multi-user accrual. A devnet frontend lives in
> `app/`. See `实现路线图-IMPLEMENTATION-ROADMAP.md` for the build history and
> `设计与实现说明-DESIGN-NOTES.md` for the reviewer-facing design.

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

## Key design (for reviewers — see Technical Design Document)

- **PDAs**: `Config [b"config"]` doubles as vault authority + $STAKE/$MILK mint
  authority; `Vault [b"vault"]`; `UserInfo [b"user", user]`.
- **Three CPIs**: `transfer` (stake in: user-signed; unstake out: PDA-signed),
  `mint_to` ($STAKE/$MILK, PDA-signed), `burn` ($STAKE, user-signed).
- **Rewards O(1)**: global `acc_reward_per_share` + per-user `reward_debt`; no loops.
- **Principal authority = $STAKE burn**: redeem amount is guarded by the burn;
  `UserInfo.amount` is a reward mirror only (known limit + upgrade path: design doc §18).
- **Safety**: PDA-only mint/vault authority, mint/owner/seeds constraints,
  `checked_*` math, `amount > 0`, checks-effects-interactions.

## Program ID

`6boJRbzGer4vYjprjSoAz879g68JRKXHvSsATsBRaZSq` (declared in `lib.rs` and `Anchor.toml`;
re-run `anchor keys list` and update both if you redeploy under a new key).

## Tests

`anchor test` spins up a local validator and runs `tests/staking.ts`:
initialize, stake (happy path), partial unstake, `amount = 0` → `AmountZero`,
over-unstake rejection, and a multi-user reward test that verifies the on-chain
accumulator step and each user's minted $MILK against an independent BigInt mirror
of the exact integer math (stronger than a fixed numeric table).
