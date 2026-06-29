# Solana 质押程序 — 完整实现路线图（工程执行清单）

> 配套文档：《Solana 质押程序 PRD》《Solana 质押程序 技术设计文档 v2.0（方案 A）》。
> 本文把 PRD 时间表 + 技术设计的"实现顺序建议"落地为**可勾选的工程执行清单**：每一步给出 **任务 / 输入 / 产出 / 验收 / 卡点提示**，按依赖顺序排列，可直接用于每 2 天进度汇报。

---

## 0. 核心心法（开工前读一遍）

1. **先核心、后奖励**：stake/unstake 必须先跑通并测试通过，再碰 MasterChef 奖励。奖励算法和核心逻辑混做 = 调试地狱（PRD 风险表）。
2. **奖励先恒定、后衰减**：`decay_per_sec = 0`（恒定速率）打通全链路并通过 7.3 数值表测试，再接线性衰减积分。不要一上来被积分细节卡住。
3. **单一本金权威 = $STAKE 的 burn**：赎回额度由 `token::burn` 守卫，`UserInfo.amount` 只是奖励镜像。心里清楚这条，写 unstake 时就不纠结"双账本"。
4. **每条改状态的指令第一步先 `update_pool()`**，再走 checks-effects-interactions（先改本地状态，再做 CPI）。
5. **每个公式先用数值例子手算验证**（7.3 表），实现后照表写断言。

---

## 1. 总览：阶段 → 里程碑 → 关键产出

| 阶段 | 名称 | 工期(全职日) | 里程碑(可验收) | 关键产出 |
|---|---|---|---|---|
| 0 | 环境与基础概念 | ~0.5–1 | 能口述 PDA/Signer/租金 | hello-world 跑通 |
| 1 | SPL Token 复习 | ~1 | 能手写 mint/transfer/burn 调用 | 复习笔记 |
| 2 | 核心 Stake/Unstake | ~4 | **核心层完成**：anchor test happy path 通过 | initialize+stake+unstake+首组测试 |
| 3 | 奖励系统(MasterChef) | ~5–6 | 7.3 数值表断言通过(Alice=150/Bob=50) | update_pool+claim+emission+奖励测试 |
| 4 | 前端 + devnet 部署 | ~2.5 | devnet 真实跑通三操作 + Solscan 链接 | React UI + 部署 |
| 5 | 文档与评审 | ~1.5 | code review 完成并记录 | README + 设计文档 + review 记录 |
| | **合计** | **~14 工作日（≈2.5–3 周）** | | 7 项交付物 |

**关键路径（依赖链）**：阶段0/1 → `state/constants/errors/events` 骨架 → `initialize` → `stake` → `unstake`（核心层完成）→ `update_pool`+恒定排放+`claim` → 二次 stake/unstake 接 pending → 线性衰减 → 前端 → 部署 → 文档/评审。

---

## 2. 阶段 0 — 环境与基础概念（~0.5–1 天）

- [x] **0.1 工具链就位**
  - 任务：确认 Rust / Solana CLI / Anchor(建议 0.30+) / Node 均可用，跑通 `anchor init` hello-world。
  - 验收：`anchor build` 成功、`anchor test` 默认用例通过。
  - 卡点：Anchor 版本与 Solana CLI 版本要匹配；用 `avm` 管理 Anchor 版本。
- [x] **0.2 Rust 基础补齐**：所有权、`Result/Option`、`struct/enum`、`?` 运算符。产出：能读懂 Anchor 宏展开的报错。
- [x] **0.3 账户模型自测关卡**：用自己的话讲清 PDA（无私钥、程序代签）、Signer、租金归属（谁是 payer）。**讲不清不往下走**（PRD 风险应对）。

---

## 3. 阶段 1 — SPL Token 快速复习（~1 天）

- [x] **1.1 复习四件套**：mint 账户、铸币(`mint_to`)、转账(`transfer`)、销毁(`burn`)，以及 ATA、mint authority 概念。
  - 参考：Cook a Solana Staking Program 课程 Part 1。
  - 验收：能在白纸上写出三类 CPI 的 `CpiContext` 形态（谁签名）。

---

## 4. 阶段 2 — 核心 Stake / Unstake（~4 天）← 最重要的地基

> 对应技术文档第 17 节步骤 1–4。本阶段**先不接奖励**，`acc_reward_per_share` 恒为 0。

- [x] **2.1 项目骨架**（已由脚手架提供，确认即可）
  - 任务：建立目录结构 `programs/staking/src/{lib,state,constants,errors,events}.rs` + `instructions/`。
  - 输入：技术文档第 15 节目录结构、第 5 节数据结构。
  - 产出：`state.rs`(Config/UserInfo)、`constants.rs`(seeds/ACC_PRECISION)、`errors.rs`、`events.rs` 编译通过。
  - 验收：`anchor build` 通过（指令体可为空壳 `Ok(())`）。

- [x] **2.2 `initialize` 指令 + Vault PDA**
  - 任务：创建 Config PDA(`[b"config"]`)、Vault token account(`[b"vault"]`, authority=Config PDA)；写入 admin、三个 mint、排放参数、bump；`last_reward_time = start_time = Clock::now`。
  - 输入：技术文档 6.1。
  - 产出：`instructions/initialize.rs`。
  - 验收(测试)：config 字段正确、vault 创建成功、stake/milk mint authority == Config PDA。
  - 卡点：`space = 8 + 226`；约束 `stake_mint.mint_authority == COption::Some(config.key())`——测试里要**先把两个 mint 的 authority 设成 Config PDA** 再调 initialize。

- [x] **2.3 `stake` 指令（transfer in + mint_to）**
  - 任务：`require!(amount>0)` → (本阶段跳过 update_pool/pending) → `token::transfer` 用户$BEEF→vault(用户签名) → 更新 `user_info.amount`、`config.total_staked` → `token::mint_to` $STAKE→用户(Config PDA 签名)。
  - 输入：技术文档 6.2。
  - 产出：`instructions/stake.rs`；`user_info` 用 `init_if_needed`。
  - 验收(测试)：用户 $BEEF 减少、vault 增加、$STAKE 等额铸出、`user_info.amount` 正确。
  - 卡点：`init_if_needed` 必须同时给 `payer = user` 和 `space`，并在 `Cargo.toml` 开 `anchor-lang` 的 `init-if-needed` feature。

- [x] **2.4 `unstake` 指令（burn + transfer out via PDA）**
  - 任务：`require!(amount>0)` → (本阶段跳过奖励结算) → `user_info.amount = checked_sub(amount)`、`total_staked = checked_sub(amount)` → `token::burn` 用户$STAKE(用户签名) → `token::transfer` vault→用户$BEEF(Config PDA 签名)。
  - 输入：技术文档 6.3。
  - 产出：`instructions/unstake.rs`。
  - 验收(测试)：$STAKE 等额 burn、$BEEF 返还、remaining 正确；超额赎回报 `InsufficientStake`。
  - 卡点：vault 转出必须 `CpiContext::new_with_signer`，seeds=`[CONFIG_SEED, &[config.bump]]`。

- [x] **2.5 第一组 anchor test（happy path）**
  - 任务：脚本创建三个 mint + 给测试钱包发 $BEEF；跑 initialize → stake → unstake，断言余额变化。
  - 产出：`tests/staking.ts` 第一部分。
  - 验收：`anchor test`（本地验证器）全绿。**← 核心层完成里程碑**

- [x] **2.6 账户校验加固**：补 mint 地址校验(`address = config.beef_mint`)、`has_one = owner`、PDA seeds+bump 约束。

---

## 5. 阶段 3 — 奖励系统 MasterChef（~5–6 天，最难）

> 对应技术文档第 7 节 + 第 17 节步骤 5–7。**先恒定速率，后线性衰减。**

- [x] **3.1 纸笔推导 + 看讲解**（0.5 天）
  - 任务：手算 7.3 数值表，确认 `pending = amount × accRewardPerShare / ACC_PRECISION − reward_debt` 自洽（Alice=150、Bob=50）。
  - 产出：推导笔记（可贴进设计文档）。

- [x] **3.2 $MILK mint + 状态字段确认**（0.5 天）
  - 任务：确认 Config 含 `acc_reward_per_share/last_reward_time/total_staked/initial_rate/decay_per_sec/min_rate/start_time`；UserInfo 含 `amount/reward_debt`。$MILK mint authority = Config PDA。
  - 验收：字段与技术文档 5.2 一致；`ACC_PRECISION = 1e12`。

- [x] **3.3 `update_pool()` 内部函数（含 Clock）**（0.5 天）
  - 任务：`now<=last_reward_time` 直接返回；`total_staked==0` 仅推进时间；否则 `acc_reward_per_share += reward * ACC_PRECISION / total_staked`。
  - 输入：技术文档 7.2；`reward = emission_between(last, now)`，**本步先用恒定速率** `initial_rate*(now-last)`。
  - 产出：`instructions/reward.rs`。
  - 卡点：全程 `u128` + `checked_*`。

- [x] **3.4 `claim_rewards` 指令（与赎回解耦）**（1 天）
  - 任务：`update_pool` → 算 pending → `require!(pending>0, NothingToClaim)` → `token::mint_to` $MILK(Config PDA 签名) → 重置 `reward_debt`。
  - 输入：技术文档 6.4（采用结算策略 A：立即铸发）。
  - 产出：`instructions/claim.rs`。
  - 验收(测试)：claim 单独领奖，$MILK == 预期 pending，本金不变。

- [x] **3.5 改造 stake/unstake 接入 pending 结算**（1 天）
  - 任务：在 stake/unstake 开头 `update_pool`；老用户先按旧 amount 结算 pending（策略 A 立即 `mint_milk`）；操作后重置 `reward_debt = amount * acc_reward_per_share / ACC_PRECISION`。
  - 验收(测试)：二次 stake 触发 pending 结算、$MILK 到账、reward_debt 重置正确。

- [x] **3.6 多用户时序测试（核心断言）**（0.5 天）
  - 任务：复刻 7.3 表：Alice t=0 存 100、Bob t=10 存 100、t=20 Alice claim。
  - 验收：Alice pending=150、Bob pending=50。**← 奖励系统正确性里程碑**
  - 卡点：本地验证器时间推进——建议用恒定速率 + 基于真实间隔断言，更可控。

- [x] **3.7 线性衰减 `emission_between(a,b)`**（1 天）
  - 任务：`r(t)=max(initial_rate - decay_per_sec*(t-start), min_rate)`；先算触底时刻 `t_floor`，把 `[a,b]` 切成衰减段(梯形积分)+恒定下限段。
  - 输入：技术文档 7.4。
  - 验收：衰减场景测试通过；`decay_per_sec=0` 退化为恒定速率。

- [x] **3.8 边界与溢出测试**：`amount=0`→AmountZero；超额赎回→InsufficientStake；无奖励 claim→NothingToClaim；大数→MathOverflow（不静默回绕）。

---

## 6. 阶段 4 — 前端 + devnet 部署（~2.5 天）

- [x] **4.1 React + wallet-adapter 接 Phantom**（0.5 天）：`AnchorProvider` + IDL 生成 `Program` 实例。
- [x] **4.2 接入三调用 + 读余额/待领奖励**（0.5 天）
  - 任务：stake/unstake/claim 三笔交易；前端用相同 seeds `findProgramAddressSync` 推导 config/vault/userInfo；读 $BEEF/$STAKE/$MILK ATA 余额；待领奖励复算或模拟交易获取。
  - 输入：技术文档第 12 节。
- [x] **4.3 devnet 部署**（0.5 天）：`anchor build`→写 program id 到 `declare_id!`+`Anchor.toml`；`solana config set --url devnet` + airdrop；部署 mint 并把 $STAKE/$MILK authority 转给 Config PDA；`anchor deploy`。
- [x] **4.4 真实跑通 + 收尾**（0.5 天）：devnet 跑一次 stake/unstake/claim，保留 **Solscan 交易链接**；UI 收尾。

---

## 7. 阶段 5 — 文档与评审（~1.5 天）

- [x] **5.1 README**（0.5 天）：程序 ID、build/test/run、stake/unstake 流程说明、安全机制说明。
- [x] **5.2 技术设计文档对照更新**（0.5 天）：架构/账户/流程/CPI/安全；保留第 18 节"已知限制与安全权衡"（评审得分点）。
- [x] **5.3 代码整理 + code review + 记录结论**（0.5 天）：组织一次 review，记录 review notes/action items/sign-off。

---

## 8. 关键技术难点速查

| 难点 | 要点 | 出处 |
|---|---|---|
| PDA 代签 | `let seeds=&[CONFIG_SEED,&[config.bump]]; let signer=&[seeds];` 传给 `CpiContext::new_with_signer` | 设计文档 8 |
| 三类 CPI 签名者 | transfer-in=用户(`new`)；transfer-out/mint=Config PDA(`new_with_signer`)；burn=用户(`new`) | 设计文档 8 |
| update_pool 顺序 | 每条改状态指令**第一步**调用；total_staked=0 时只推进时间不发奖 | 设计文档 7.2 |
| reward_debt | 份额变更前先结清 pending，变更后重置 `reward_debt = amount*acc/ACC_PRECISION` | 设计文档 6.2/6.3 |
| emission 积分 | 先恒定后衰减；衰减用梯形面积 + 触底段；全程 u128 checked | 设计文档 7.4 |
| init_if_needed | 同时给 `payer`+`space`，开 cargo feature；不可假设字段是零值 | 设计文档 5.4 |
| 精度/溢出 | acc/reward_debt 用 u128，乘法用 u128 中间量 + checked_*；整除灰尘可接受 | 设计文档 7.5 |

---

## 9. 验收标准对照表（交付前自检）

| # | 验收标准 | 对应步骤 |
|---|---|---|
| 1 | `anchor test` 通过(stake/unstake/claim 各至少一条) | 2.5 / 3.4 / 3.6 |
| 2 | 前端连钱包完成三操作 | 4.1–4.4 |
| 3 | 奖励计算 O(1)，无遍历用户 | 3.3 update_pool |
| 4 | 本金与奖励解耦：可部分赎回、可单独领奖 | 2.4 / 3.4 |
| 5 | 仅 PDA 可铸币/转出金库；账户与签名者校验完整；防溢出 | 2.6 / 3.8 |
| 6 | 能口头解释 PDA、三类 CPI、租金归属、金库防盗 | 0.3 / 5.2 |
| — | 7 项交付物齐全 | PRD 第 9 节 |

---

## 10. 汇报节点（PRD 第 8 节）

- **PRD 评审**：本周五，LD，确认范围/时间表/技术方案。
- **每 2 天进度汇报**：已完成(步骤编号) / 进行中 / 阻塞·风险 / 接下来 2 天计划。
  - D2、D4 → 阶段 0–1；D6、D8、D10 → 阶段 2；D12、D14、D16 → 阶段 3；D18 → 阶段 4；D20 → 最终汇报。
- **最终汇报(7.10)**：演示 stake/unstake/claim 全流程 + 交付物清单 + code review 结论。

---

## 11. 风险与缓冲（PRD 第 10 节）

| 风险 | 应对 |
|---|---|
| PDA/CPI 没吃透就开工 | 阶段 0 自测关卡，讲不清不往下走 |
| 奖励与核心混做 | 先核心层测试通过再做奖励（阶段 2→3 强制顺序） |
| 低估奖励难度 | 阶段 3 预留缓冲；先恒定后衰减；先用数值例子推公式 |
| devnet/airdrop 不稳 | 本地验证器作备选验证环境 |
