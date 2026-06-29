# Frontend (Vite + React + wallet-adapter, devnet)

Roadmap stage 4. Connect Phantom and stake / unstake / claim against the deployed
program on devnet. See Technical Design Document §12.

## Prerequisites

The program must be built and deployed to devnet, and the mints created:

```bash
# from repo root
solana config set --url devnet
solana airdrop 2
anchor build && anchor deploy
npx ts-node scripts/setup-devnet.ts     # creates mints + initializes the pool
```

Copy the three mint addresses it prints into `src/config.ts`.

## Run

```bash
cd app
npm install
npm run dev        # also copies target/idl/staking.json -> public/staking.json
```

Open the printed localhost URL, switch Phantom to **devnet**, connect, and use
Stake / Unstake / Claim. Each confirmed tx shows a Solscan link.

## How it works

- `main.tsx` — wallet-adapter providers (Phantom) pointed at devnet.
- `config.ts` — the three mint addresses + `ACC_PRECISION` (must match the program).
- `staking.ts` — loads the IDL at runtime (`/staking.json`), derives the `config` /
  `vault` / `userInfo` PDAs with the same seeds as on-chain, ensures the user's
  ATAs exist (prepended instructions), and builds the stake / unstake / claim txs
  with `accountsStrict`. `estimatePending` mirrors `reward::emission_between` to
  preview claimable $MILK (the on-chain value is authoritative at claim time).
- `App.tsx` — balances, amount input, three action buttons, tx status + Solscan link.

> Strategy A: staking requires a $MILK ATA (pending is minted on every principal
> op). The app auto-creates all three ATAs on first use, so no manual setup needed.
