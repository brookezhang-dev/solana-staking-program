# 设计与实现说明（Design & Implementation Notes）

> Reviewer 视角的技术说明。配合源码阅读即可理解方案。基于《Solana 质押程序 技术设计文档 v2.0（方案 A）》落地,并在末尾记录与该设计稿的**实现变更**。

| 项目 | 内容 |
|---|---|
| 框架 | Anchor 0.31.1 · Solana 4.0.x · 经典 SPL Token |
| 程序 ID | `6boJRbzGer4vYjprjSoAz879g68JRKXHvSsATsBRaZSq` |
| 网络 | localnet(测试) → devnet(验收) |
| 状态 | 功能完整:initialize / stake / unstake / claim_rewards + MasterChef O(1) 奖励 + 线性衰减排放 |

---

## 1. 一句话

用户存入 **$BEEF** 获得 1:1 的 **$STAKE** 凭证,质押期间按份额与时长累积 **$MILK**,可随时部分/全额赎回本金、独立领奖。奖励结算 O(1),排放速率随时间线性衰减至下限。

## 2. 账户模型

| 账户 | seeds | 作用 |
|---|---|---|
| `Config` PDA | `[b"config"]` | 全局配置 + 奖励池状态;**兼任金库授权与 $STAKE/$MILK 铸币权限** |
| `Vault` | `[b"vault"]` | 持有质押 $BEEF 的 token account,authority = Config PDA |
| `UserInfo` PDA | `[b"user", user]` | 每用户一个,存 `amount`(奖励份额镜像)+ `reward_debt` |

三种代币:$BEEF(输入,外部铸)、$STAKE(凭证,Config PDA 铸/用户烧)、$MILK(奖励,Config PDA 铸)。$STAKE/$MILK 的 mint authority 在创建时即设为 Config PDA,杜绝外部增发。每条指令用 `address = config.*_mint` / `mint == config.*_mint` 约束校验传入账户,防伪造代币。

`Config` 数据区 226 字节(账户 8+226=234);`UserInfo` 57 字节(账户 65)。

## 3. 指令流程

- **initialize**:创建 Config + Vault,校验 $STAKE/$MILK 铸币权已是 Config PDA,写入三个 mint、排放参数(`initial_rate`/`decay_per_sec`/`min_rate`)与 `start_time = last_reward_time = now`。
- **stake(amount)**:`update_pool` → 按旧份额结算 pending → 改 `amount`/`total_staked`、重置 `reward_debt` → `transfer` 用户$BEEF→vault(用户签名)→ `mint_to` $STAKE(PDA 签名)→ pending>0 则 `mint_to` $MILK(PDA 签名)。
- **unstake(amount)**:`update_pool` → 结算 pending → `checked_sub` 份额/总量(下溢报 `InsufficientStake`)、重置 `reward_debt` → `burn` $STAKE(用户签名,**赎回额度的真正守卫**)→ `transfer` vault→用户$BEEF(PDA 签名)→ 结算 pending $MILK。
- **claim_rewards()**:`update_pool` → `pending = amount·acc/ACC_PRECISION − reward_debt`,无则报 `NothingToClaim` → `mint_to` $MILK(PDA 签名)→ 重置 `reward_debt`。本金不动。

通用约定:`amount > 0`;改状态的指令第一步必 `update_pool`;遵循 checks-effects-interactions。Solana 交易原子,任一 CPI 失败整笔回滚,故先改状态后做 CPI 安全。

## 4. 三类 CPI 与签名者

| 场景 | 调用 | 签名者 | CpiContext |
|---|---|---|---|
| 质押转入 | `transfer` | 用户 | `new` |
| 赎回转出 | `transfer` | Config PDA | `new_with_signer` |
| 铸 $STAKE / $MILK | `mint_to` | Config PDA | `new_with_signer` |
| 烧 $STAKE | `burn` | 用户 | `new` |

PDA 签名 seeds:`&[&[b"config", &[config.bump]]]`。

## 5. 奖励算法(MasterChef O(1))

```
update_pool:  acc_reward_per_share += emission_between(last, now) · ACC_PRECISION / total_staked
pending:      amount · acc_reward_per_share / ACC_PRECISION − reward_debt
reward_debt:  份额变更后重置为 amount · acc_reward_per_share / ACC_PRECISION
```

`ACC_PRECISION = 1e12`。所有乘法用 `u128` 中间量 + `checked_*`,溢出报 `MathOverflow`,绝不静默回绕。整除余数(灰尘)留池,MasterChef 已知特性,可接受。

**动态排放(线性衰减)**:`r(t) = max(initial_rate − decay_per_sec·(t − start_time), min_rate)`。`emission_between(a,b)` 为闭式 O(1):在触底秒 `t_floor = start + (initial_rate − min_rate)/decay_per_sec` 把区间切成"衰减梯形段 + 恒定下限段"分别积分。`decay_per_sec = 0` 退化为恒定速率。**梯形面积整体一次性向下取整** `(2·r0·span − k·Δsq)/2`,保证截断永远偏少、**绝不超铸**(见第 8 节)。

## 6. 安全

权限隔离(金库/铸币权仅 Config PDA)、mint/owner/seeds 约束、`checked_*` 防溢出、`amount>0`、本金权威唯一(=$STAKE 销毁,无双账本)、原子顺序、`UserInfo` 用 seeds + `owner == user` 双重绑定、管理员权限最小(仅排放参数,无暂停/没收后门)。

## 7. 测试与前端

- `tests/staking.ts`:initialize / stake / 部分 unstake / `amount=0` / 超额赎回 / 多用户奖励。奖励断言用"读链上实际时间戳 + 独立 BigInt 镜像复算"做确定性校验(比硬编码 §7.3 的 150/50 更强,适配任意时间差)。
- 离线对照:30 万随机用例对"分数精确连续积分"验证衰减算法 **0 例超铸**、欠发 < `decay_per_sec+2`、随时间单调递减。
- 前端 `app/`:Vite + React + wallet-adapter(Phantom),devnet,stake/unstake/claim + 余额/待领估算 + Solscan 链接;运行时加载 IDL,自动补建 ATA。

---

## 8. 与 v2.0 设计稿的实现变更（评审重点）

1. **策略 A 的账户后果**:设计稿 §6.2/§6.3 选了"操作本金时立即铸发 pending $MILK"(策略 A),但其账户清单漏列了 milk 账户。实现据策略 A **给 `stake`/`unstake` 都补上了 `milk_mint` + `user_milk_ata`**;客户端在这两个操作前需保证用户的 $MILK ATA 存在(前端已自动补建)。若改用策略 B(累计 `pending` 字段、仅 claim 发放),可移除这两个账户、`UserInfo` 加一个 `pending: u64` 字段——已评估,本版采用 A。
2. **衰减面积取整修正(本轮发现的真实 bug)**:设计稿 §7.4 给的公式 `initial_rate·(b−a) − decay·(…)/2` 若对"被减的衰减项"单独取整,会把面积**向上**取整,导致每次最多超铸约 0.5 单位 $MILK。实现改为对整块面积一次性向下取整 `(2·r0·span − k·Δsq)/2`,保证**永不超铸**(偏差仅为偏少的灰尘)。
3. **框架版本**:Anchor 0.30+ → **0.31.1**;`ctx.bumps.<account>` 字段访问、IDL 内嵌 `address`(前端 `new Program(idl, provider)` 据此取 program id)。
4. **测试断言策略**:实时验证器无法精确控时,故不硬编码 150/50,改为对链上累加器步进与领取额做公式级确定性校验;如需复刻字面数值表,可引入 `solana-bankrun` 进行时钟穿越。

## 9. 已知限制(承自设计稿 §18)

$STAKE 为可转让经典 SPL Token。若用户在程序外直接转走 $STAKE,会造成奖励账本(`UserInfo.amount`)与 $STAKE 余额脱同步:转出方可能"无凭证仍计息",接收方"有凭证无法在程序内解奖"。**本金与金库始终安全**(赎回只认 burn,按销毁等额放款,不会被多取或掏空),受影响的仅是奖励归属。生产级修法:Token-2022 Non-Transferable 凭证(方案 B,推荐升级路径)或 Transfer Hook(方案 C)。本挑战定位 beginner、与参考课程一致,作为已记录、影响可控的限制接受。
