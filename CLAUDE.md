# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Solana Anchor DeFi staking program: deposit **$BEEF** → receive 1:1 **$STAKE** receipt token, accrue **$MILK** rewards over time (MasterChef O(1) accumulator), redeem principal and claim rewards independently.

**Status (as of 2026-06-26):** Core layer complete through step 2.5. `initialize`, `stake`, and `unstake` are fully implemented and tested. `reward.rs` helpers (`update_pool`, `pending_reward`, `reward_debt_for`, constant-rate `emission_between`) are implemented. `claim_rewards` is a stub (step 3.4). Next: wire `update_pool` into `stake`/`unstake` (step 3.5), then implement `claim_rewards`. Follow `实现路线图-IMPLEMENTATION-ROADMAP.md` for the full sequence.

## Commands

```bash
# Install JS dependencies
pnpm install

# Build the program
anchor build

# Run all tests (local validator)
anchor test

# Run a single test file
pnpm exec ts-mocha -p ./tsconfig.json -t 1000000 tests/staking.ts

# Lint TypeScript
pnpm lint        # check
pnpm lint:fix    # auto-fix

# Deploy to devnet
solana config set --url devnet
solana airdrop 2
anchor deploy
```

After first `anchor build`, copy the program ID from `anchor keys list` and update both `declare_id!()` in `programs/staking/src/lib.rs` AND `[programs.*]` in `Anchor.toml`.

## Source Layout

```
programs/staking/src/
  lib.rs               # #[program] entrypoints → dispatch to handlers
  state.rs             # Config + UserInfo account structs with SPACE constants
  constants.rs         # ACC_PRECISION (1e12), PDA seeds (CONFIG_SEED, VAULT_SEED, USER_SEED)
  errors.rs            # StakingError enum
  events.rs            # StakeEvent, UnstakeEvent, ClaimEvent
  instructions/
    initialize.rs      # Creates Config + Vault PDAs, sets emission params
    stake.rs           # Transfer $BEEF in, mint $STAKE out (core layer done; rewards TODO 3.5)
    unstake.rs         # Burn $STAKE, refund $BEEF from vault (core layer done; rewards TODO 3.5)
    claim.rs           # STUB — handler body commented out (TODO 3.4)
    reward.rs          # Shared helpers: update_pool, pending_reward, reward_debt_for,
                       #   emission_between (constant-rate done; decay branch TODO 3.7)
    mod.rs             # re-exports
tests/
  staking.ts           # Core layer tests: initialize, stake, unstake, boundary cases
```

## Architecture

### Account Model

- **`Config` PDA** (`seeds = [b"config"]`): global pool state + emission params. Doubles as the vault authority AND the $STAKE/$MILK mint authority. Stores `acc_reward_per_share` (the MasterChef accumulator) and `last_reward_time`.
- **`UserInfo` PDA** (`seeds = [b"user", user_pubkey]`): per-user reward mirror. `amount` is NOT the authoritative principal—it mirrors the user's $STAKE balance for reward calculation. The real principal guard is the $STAKE burn in `unstake`.
- **`Vault` PDA** (`seeds = [b"vault"]`): token account holding deposited $BEEF, authority = Config PDA.

### Three Token Types

| Token | Role | Mint Authority |
|-------|------|----------------|
| $BEEF | Input (user-owned) | External |
| $STAKE | Receipt 1:1 with $BEEF | Config PDA |
| $MILK | Reward | Config PDA |

### Instruction Flow

1. **`initialize`**: Creates Config + Vault PDAs, sets emission params (`initial_rate`, `decay_per_sec`, `min_rate`), records `start_time`. $STAKE/$MILK mint authorities must already be set to Config PDA before calling (the test `before()` block does this).
2. **`stake(amount)`**: Transfer $BEEF user→vault (user signs) → update `user_info.amount` + `config.total_staked` → mint $STAKE to user (Config PDA signs). **TODO 3.5**: prepend `update_pool` + pending settlement.
3. **`unstake(amount)`**: `update_pool` → settle pending → `checked_sub` on mirror amounts → burn $STAKE (user signs, THE guard) → transfer $BEEF vault→user (Config PDA signs via `new_with_signer`). **TODO 3.5**: reward wiring.
4. **`claim_rewards()`**: `update_pool` → compute pending → mint $MILK to user (Config PDA signs) → reset `reward_debt`. **TODO 3.4**: implement handler body.

### CPI Signing Pattern

- **Transfer in** (user $BEEF → vault): `CpiContext::new(...)` — user is the signer
- **Transfer out** (vault → user $BEEF) and **mint_to** ($STAKE/$MILK): `CpiContext::new_with_signer(...)` with `signer_seeds = &[&[CONFIG_SEED, &[config.bump]]]`
- **Burn** ($STAKE): `CpiContext::new(...)` — user is the signer

### Reward Math (MasterChef O(1))

```
update_pool:  acc_reward_per_share += emission * ACC_PRECISION / total_staked
pending:      amount * acc_reward_per_share / ACC_PRECISION - reward_debt
reward_debt:  reset to  amount * acc_reward_per_share / ACC_PRECISION  after any share change
```

`ACC_PRECISION = 1_000_000_000_000` (1e12). All intermediate values use `u128` with `checked_*` arithmetic — never allow silent wraps.

`update_pool` must be the **first call** in any instruction that mutates `total_staked` or `acc_reward_per_share`. Settle pending against the OLD `amount` before updating the share.

### Emission

`emission_between(a, b)` in `reward.rs` computes $MILK emitted over `[a, b]`:
- `decay_per_sec == 0`: constant rate → `initial_rate * (b - a)` (**implemented**)
- `decay_per_sec > 0`: `r(t) = max(initial_rate - decay_per_sec*(t - start_time), min_rate)` — split at `t_floor` into trapezoid + floor segment (**TODO 3.7**; currently falls back to constant rate)

### Implementation Order (Critical)

Per the roadmap: **core first, rewards second; constant rate before decay.**

- [x] `unstake` handler (step 2.4) — no rewards yet
- [x] First happy-path tests (step 2.5)
- [ ] `claim_rewards` (step 3.4) with constant emission
- [ ] Wire `update_pool` into `stake`/`unstake` (step 3.5)
- [ ] Multi-user timing test asserting Alice=150, Bob=50 (step 3.6)
- [ ] Linear decay in `emission_between` (step 3.7)
- [ ] Boundary/overflow tests (step 3.8)

## Key Constraints

- `init_if_needed` on `UserInfo` requires both `payer` and `space` in the constraint, plus the `init-if-needed` feature in `Cargo.toml`.
- `UserInfo.amount` is a reward-share mirror only — do not treat it as the authoritative principal. The real guard is the $STAKE burn (if the user lacks $STAKE, the burn fails and the entire tx reverts).
- For `unstake`'s vault transfer: must use `CpiContext::new_with_signer` with Config PDA seeds.
- Always use `checked_add`/`checked_sub`/`checked_mul`/`checked_div` — never unwrap arithmetic.
- When settling pending rewards in `stake`/`unstake` (step 3.5): compute pending against the OLD `user_info.amount` and OLD `acc_reward_per_share` BEFORE updating either; reset `reward_debt` AFTER.
