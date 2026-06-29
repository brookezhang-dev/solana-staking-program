# Code Review 记录 — Solana Staking Program

| 项目 | 内容 |
|---|---|
| 评审对象 | Anchor 程序 + 测试 + 前端(`programs/staking`、`tests/`、`app/`、`scripts/`) |
| 程序 ID | `6boJRbzGer4vYjprjSoAz879g68JRKXHvSsATsBRaZSq` |
| 提交人 | brooke |
| 评审人 | _(待填:LD / mentor / peer)_ |
| 日期 | _(待填)_ |
| 结论 | ☐ 通过 ☐ 通过(带 action items) ☐ 需返工 |

> 本文先做一轮**自评**(对照验收标准 + 记录已发现/已修问题),评审会上据此走查,评审人补充意见与签收。

---

## 1. 验收标准对照(自评)

| # | 验收标准 | 自评 | 证据 |
|---|---|---|---|
| 1 | `anchor test` 通过(stake/unstake/claim 各一条) | ☑ | `tests/staking.ts`:initialize/stake/unstake/claim + 多用户 |
| 2 | 前端连钱包完成三操作 | ☑ | `app/`(Phantom·devnet),三按钮 + Solscan 链接 |
| 3 | 奖励 O(1),无遍历用户 | ☑ | `reward::update_pool` 仅更新两个全局量 |
| 4 | 本金与奖励解耦:可部分赎回、可单独领奖 | ☑ | `unstake` 支持部分;`claim_rewards` 独立指令 |
| 5 | 仅 PDA 可铸币/转出金库;账户与签名者校验完整;防溢出 | ☑ | CPI 签名表;`address`/`mint`/`seeds` 约束;全程 `checked_*` |
| 6 | 能口头解释 PDA、三类 CPI、租金归属、金库防盗 | ☑ | 见 DESIGN-NOTES §2/§4/§6 |
| 7 | 交付物齐全 | ☑ | 源码/测试/前端/脚本/README/设计文档/本记录 |
| 进阶 | 动态排放(随时间衰减) | ☑ | `emission_between` 线性衰减闭式积分 |

## 2. 走查清单(评审会逐项确认)

- [ ] **PDA 与签名**:Config 兼任金库+铸币权;转出/铸造均 `new_with_signer`,seeds 正确。
- [ ] **三类 CPI**:transfer(in 用户 / out PDA)、mint_to(PDA)、burn(用户)签名者无误。
- [ ] **MasterChef 顺序**:改状态指令第一步 `update_pool`;pending 按旧份额结算;`reward_debt` 变更后重置。
- [ ] **本金权威**:赎回额度由 $STAKE burn 守卫;`UserInfo.amount` 仅作镜像 `checked_sub`。
- [ ] **数值安全**:乘法走 u128;`checked_*` 全覆盖;`amount > 0`;无静默回绕。
- [ ] **校验约束**:mint 地址、token account 归属/币种、seeds+bump、`owner == user`。
- [ ] **衰减积分**:触底切段;面积整体下取整(不超铸);`decay=0` 退化恒定。
- [ ] **租金**:Config/Vault 由 admin 付;UserInfo 由 user 付(`init_if_needed` 含 payer+space)。
- [ ] **前端**:PDA seeds 与链上一致;自动补建 ATA;待领估算与链上口径一致。

## 3. 已发现并已修复的问题

| ID | 严重度 | 问题 | 修复 | 状态 |
|---|---|---|---|---|
| R-1 | **中** | 衰减梯形面积对"被减项"单独整除,导致面积向上取整,每次最多**超铸约 0.5 单位 $MILK** | 改为整块面积一次性向下取整 `(2·r0·span − k·Δsq)/2`,保证永不超铸 | ✅ 已修 |
| R-2 | 低 | Anchor 0.31 下显式传入可解析账户会触发类型报错 | 测试/前端统一用 `accountsStrict` | ✅ 已修 |
| R-3 | 低 | 策略 A 下 `stake`/`unstake` 设计稿漏列 milk 账户 | 两指令补 `milk_mint`+`user_milk_ata`;前端自动补建 ATA | ✅ 已修 |

**验证手段**:账户结构↔测试/前端逐字段一致性脚本(全过);30 万随机用例对"分数精确连续积分"校验衰减(0 例超铸、欠发 < decay+2、单调递减);§7.3 数值表 BigInt 镜像复算 = Alice 150 / Bob 50。

## 4. 已知限制(接受,非缺陷)

- **$STAKE 可转让导致奖励账本脱同步**(DESIGN-NOTES §9 / 设计稿 §18):本金与金库安全无虞,仅影响奖励归属;升级路径为 Token-2022 Non-Transferable(方案 B)。
- **整除灰尘**:每股极小余数留池不可领,MasterChef 已知特性。
- **on-chain 溢出用例**:真实 u64 溢出难以在链上构造,以 `checked_*` 契约 + 离线对照覆盖,未写专门链上用例。

## 5. Action Items(评审人填)

- [ ] _…_
- [ ] _…_

## 6. 签收

| 角色 | 姓名 | 结论 | 备注 |
|---|---|---|---|
| 评审人 | | ☐ 通过 ☐ 带条件通过 ☐ 返工 | |
| 提交人 | brooke | | |
