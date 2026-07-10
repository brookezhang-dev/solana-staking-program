import { useCallback, useEffect, useMemo, useState } from "react";
import { useConnection, useAnchorWallet } from "@solana/wallet-adapter-react";
import { WalletMultiButton } from "@solana/wallet-adapter-react-ui";
import type { Wallet } from "@coral-xyz/anchor";
import {
  BEEF_MINT,
  REWARD_MINT,
  STAKE_MINT,
  solscanTx,
} from "./config";
import {
  stakedMint,
  doClaim,
  doFaucet,
  doStake,
  doUnstake,
  estimatePending,
  fromBase,
  getProgram,
  isPaused,
  rewardMint,
  configPda,
  poolPda,
  readBalance,
  stakeReceiptMint,
  stakeAtaOf,
  toBase,
  userInfoPda,
  CLASSIC_PROGRAM,
  STAKE_PROGRAM,
} from "./staking";

const configured =
  ![BEEF_MINT, STAKE_MINT, REWARD_MINT].some((m) => m.startsWith("REPLACE"));

type Balances = { beef: bigint; stake: bigint; reward: bigint; pending: bigint };

export default function App() {
  const { connection } = useConnection();
  const wallet = useAnchorWallet();
  const owner = wallet?.publicKey;

  const [amount, setAmount] = useState("10");
  const [bal, setBal] = useState<Balances | null>(null);
  const [status, setStatus] = useState<{ msg: string; sig?: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [paused, setPaused] = useState(false);

  const programReady = useMemo(() => configured && !!wallet, [wallet]);

  const refresh = useCallback(async () => {
    if (!owner) return;
    try {
      const program = await getProgram(connection, wallet as Wallet);
      const config = configPda(program.programId);
      const pool = poolPda(program.programId);
      const [beef, stake, reward] = await Promise.all([
        readBalance(connection, stakedMint(), owner, CLASSIC_PROGRAM),
        readBalance(connection, stakeReceiptMint(), owner, STAKE_PROGRAM),
        readBalance(connection, rewardMint(), owner, CLASSIC_PROGRAM),
      ]);
      let pending = 0n;
      try {
        const cfg = await program.account.config.fetch(config);
        setPaused(isPaused(cfg));
        try {
          const p = await program.account.pool.fetch(pool);
          const ui = await program.account.userInfo.fetch(userInfoPda(program.programId, stakeAtaOf(owner)));
          pending = estimatePending(p, ui, stake);
        } catch {
          /* pool / userInfo not created yet → no pending */
        }
      } catch {
        /* config not initialized → leave defaults */
      }
      setBal({ beef, stake, reward, pending });
    } catch (e: any) {
      setStatus({ msg: `read error: ${e.message ?? e}` });
    }
  }, [connection, wallet, owner]);

  useEffect(() => {
    if (programReady) refresh();
  }, [programReady, refresh]);

  const run = async (
    label: string,
    fn: (ctx: any, amt?: any) => Promise<string>,
    withAmount: boolean
  ) => {
    if (!owner) return;
    setBusy(true);
    setStatus({ msg: `${label}…` });
    try {
      const program = await getProgram(connection, wallet as Wallet);
      const ctx = { program, connection, owner };
      const sig = withAmount ? await fn(ctx, toBase(amount)) : await fn(ctx);
      setStatus({ msg: `${label} confirmed`, sig });
      await refresh();
    } catch (e: any) {
      setStatus({ msg: `${label} failed: ${e.message ?? e}` });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="wrap">
      <h1>🥩 $BEEF Staking <span className="muted">· devnet</span></h1>
      <WalletMultiButton />

      {!configured && (
        <div className="warn card">
          Mints not configured. Run <code>npx ts-node scripts/setup-devnet.ts</code> from the
          repo root, then paste the printed addresses into <code>app/src/config.ts</code>.
        </div>
      )}

      {owner && (
        <>
          <div className="card">
            <div className="row"><span className="muted">$BEEF</span><span>{bal ? fromBase(bal.beef) : "—"}</span></div>
            <div className="row"><span className="muted">$STAKE (staked)</span><span>{bal ? fromBase(bal.stake) : "—"}</span></div>
            <div className="row"><span className="muted">$MILK (reward)</span><span>{bal ? fromBase(bal.reward) : "—"}</span></div>
            <div className="row"><span className="muted">Claimable $MILK (est.)</span><span>{bal ? fromBase(bal.pending) : "—"}</span></div>
          </div>

          {paused && (
            <div className="warn card">
              池子已暂停(Paused):Stake / Claim 已禁用,Unstake 仍可随时退出本金。
            </div>
          )}

          <div className="card">
            <label className="muted">Amount</label>
            <input value={amount} onChange={(e) => setAmount(e.target.value)} inputMode="decimal" />
            <div className="grid3">
              <button className="act" disabled={busy || !programReady || paused} onClick={() => run("Stake", doStake, true)}>Stake</button>
              <button className="act" disabled={busy || !programReady} onClick={() => run("Unstake", doUnstake, true)}>Unstake</button>
              <button className="act" disabled={busy || !programReady || paused} onClick={() => run("Claim", doClaim, false)}>Claim</button>
            </div>
            <button
              className="act ghost"
              disabled={busy || !programReady}
              onClick={() => run("领取测试 $BEEF", (ctx: any) => doFaucet(ctx, toBase("1000")), false)}
            >
              {bal && bal.beef === 0n ? "余额为 0 — 领取 1000 测试 $BEEF" : "领取 1000 测试 $BEEF"}
            </button>
          </div>
        </>
      )}

      {status && (
        <div className="card status">
          <div>{status.msg}</div>
          {status.sig && (
            <a href={solscanTx(status.sig)} target="_blank" rel="noreferrer">
              View on Solscan →
            </a>
          )}
        </div>
      )}
    </div>
  );
}
