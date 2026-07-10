# Changelog

## v3.x — single-vault → Config/Pool two-tier

One deployment can now host many independent staking pools (Raydium/Orca pattern).

- **Tier 1 — `Config`** (singleton, seeds `[b"config"]`): `admin`, `pool_count`,
  global `paused`. Admin-gated: `create_pool`, `set_pause`, `transfer_admin`,
  `set_emission`, `withdraw_surplus`.
- **Tier 2 — `Pool`** (seeds `[b"pool", config, staked_mint, reward_mint]`): all
  per-pool accounting (accumulator, emission, totals, vault/mint addresses). One pool
  per mint pair — duplicates impossible by PDA construction; clients derive statelessly.
- **Child accounts re-keyed under pool**: `staked_vault [b"staked_vault", pool]`,
  `reward_vault [b"reward_vault", pool]`, `user_info [b"user_info", pool, stake_ata]`.
  The **Pool PDA** is the authority for its vaults and $STAKE mint (per-pool dual role).
- **`initialize` split** → `initialize_config` (once) + `create_pool` (admin, repeatable).
- **Cross-pool isolation**: every user instruction binds all child accounts to `pool`
  (`pool.config == config`, `address = pool.staked_vault`, pool-scoped `user_info` seeds).
- **Pause**: global `Config.paused` (replaces per-pool status); blocks stake/claim,
  never unstake. `withdraw_surplus` / `close_user_info` retained, now pool-scoped.
- **$STAKE receipt mint** stays externally created (Token-2022 TransferHook, authority
  = Pool PDA), validated + recorded in `create_pool`.
- Emission model unchanged (per-second `reward_per_sec` + `start/end_time` + `total_emitted`).

**Non-goals** (future work): multiple Config instances, per-pool admins, owner/pauser split.

**Migration**: breaking layout change — fresh deploy + fresh `setup-devnet.ts`.


## v3.1 — Transferable $STAKE via Token-2022 Transfer Hook

Reverses the v3 soulbound decision: $STAKE becomes a transferable, balance-authoritative
certificate, with reward settlement enforced on every transfer by a Transfer Hook.

**Deltas vs v3**

- **Mint**: `NonTransferable` → `TransferHook` extension pointing at this program.
  `initialize` now validates the hook program_id == `crate::ID` (was: NT check).
- **Ledger authority**: deleted `UserInfo.amount`. The $STAKE **token-account balance**
  is now the single principal/share authority. `total_staked` retained (== supply).
- **UserInfo re-keyed**: seeds `[b"user_info", stake_token_account]` (was `[b"user", owner]`);
  fields now `token_account / reward_debt / pending_unclaimed / bump / reserved[32]`.
- **New instructions**:
  - `register` — create a token account's `UserInfo` (required before it can *receive*).
  - `initialize_extra_account_meta_list` — one-time hook account-metas setup per mint.
  - `transfer_hook` (`execute`) — dispatched via Anchor `fallback`; settles sender +
    receiver (strategy B, records only; no vault movement). Guarded by the Token-2022
    `transferring` flag (rejects direct calls).
- **stake / unstake / claim**: read the $STAKE ATA balance instead of `UserInfo.amount`.
  unstake guard = burn + explicit `amount <= balance`. claim now takes the stake ATA.
- **Interest semantics (locked)**: interest does NOT travel with the token — on transfer
  the sender's accrued pending is settled into the sender's `pending_unclaimed`; the
  receiver accrues only from receipt.
- **Frontend/scripts**: setup creates a TransferHook mint + initializes the meta list;
  the app derives `UserInfo` from the stake ATA and estimates pending from balance.

**Dependencies**: `spl-transfer-hook-interface = 0.9.0`, `spl-tlv-account-resolution =
0.9.0` (match the versions already resolved by anchor-spl 0.31.1 / spl-token-2022 6.0 —
no toolchain conflict).

**Migration note**: this is a breaking on-chain layout change (mint extension + UserInfo
seeds/fields). Deploy under a **fresh program keypair** and re-run `setup-devnet.ts`.
