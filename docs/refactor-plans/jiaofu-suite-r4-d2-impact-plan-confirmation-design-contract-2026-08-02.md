# Jiaofu Suite R4-D2 影响计划确认 Use-case 拆分设计合同（2026-08-02）

## 1. 合同性结论

R4-D2 只做首拆闭环后的重新盘点，不修改生产 Rust、测试、Tauri、SQL/迁移、Cargo、前端或业务数据。按调用图、事务所有权、私有 helper 内聚度和失败覆盖筛选后，下一候选固定为：

```text
question_performance::confirm_question_impact_plan
```

不得因为 `assessment.rs`、`subjective.rs` 或其他文件更大而改变候选，也不得把 E2 失败测试与 S2 生产外移合并施工。

## 2. 改前冻结基线

- 分支：`codex/t2-artifacts`
- HEAD：`6072360049b09c3155726773039da139834294fe`
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`
- staged：0；展开 porcelain：173
- 父模块：2,330 行，SHA-256 `e7ec100aad1f7d6c1fb796e25425ccd6730ed967370f4f0c153759d2d6303af7`
- 测试文件：1,210 行，SHA-256 `0b001785de89ae03b6dc3e65ed44fc725aec805d09d133af659a3f8c6d5fd05a`
- `knowledge_commands.rs` SHA-256：`24e448222865bd22595600a817966fc7bd575793b96ad673ea880796aea22f22`
- `service/mod.rs` / crate `lib.rs` SHA-256：`77ce93c8f471f9a60309a38f43439fc40653825bd938867b8db4cd576730b119` / `0b26882097478be45ba6284c64a37bdcf1b0f8e5e8937525cf2568d63d8dd915`
- module / Tauri / workspace Cargo SHA-256：`56e92305116957bfd1e4e84edf77b4976e0a1ea987507b4b0cc89aa20cf8b90e` / `402cb2db4c1f3d032ef5c7924df6afe4f2513518faf523e088b661fabdf2b5e3` / `b9708e762fdedb6b69ee641293aaed9305bf1f0a06a3af6466627fcda7309a20`
- migration 文件哈希清单 SHA-256：`6ec2d7fd59ea651166e27828b8a7ff9e9d2fbe555724c193c19e9b007f49bf24`
- 当前专项：10/10；`module-exam`：209/209；workspace：482/482；Tauri：74/74。

## 3. 重新盘点与取舍

| 候选 | 结果 | 依据 |
|---|---|---|
| `question_performance::confirm_question_impact_plan` | 选中 | 一个生产 caller、一个公开入口、一个 transaction owner；三个专属私有 helper 连续；只写计划/任务/outbox/audit；可在 late outbox 阶段注入失败 |
| `question_performance::prepare_question_impact_review_cases` | 后置 | 同样单事务，但包含 unpublished/published 两类 case 生成与更宽的来源快照校验；先完成更小的计划确认边界 |
| `question_performance::resolve/publish_*` | 拒绝为当前批 | 会触及评分 revision、发布与 evidence 派生，历史副作用更宽 |
| `assessment.rs` / `subjective.rs` | 继续冻结 | 多个公开入口与跨模块 transaction helper，终审、发布、evidence 职责交织 |
| `answer_source.rs` / `objective.rs` / `papers.rs` | 继续冻结 | 多 wrapper/运行入口或多阶段状态机，不具备当前候选同等的单入口连续边界 |
| `answer_sheet.rs` / blueprint 系列 | 后续可再评估 | 有单事务候选，但当前聚合压力和本轮上下文连续性弱于 `question_performance` |

## 4. 候选机械边界

父模块当前第 957～1199 行为一个 243 行连续块，SHA-256：

```text
be4b3ca94e5751eb1160f2f504de7b70fdcf82ddc07d1999cb93853e6507f139
```

该块只包含：

1. `validate_action`；
2. `plan_by_id`；
3. `task_scopes`；
4. `confirm_question_impact_plan`。

三个 helper 在生产代码中只服务该公开 use-case。唯一生产 caller 为 `src-tauri/src/knowledge_commands.rs`；其余调用均为模块测试。候选允许读取父模块中的 `load_target`、`preview_question_version_impact`、DTO、常量和公共基础设施，但不允许改变其可见 API 或业务含义。

## 5. 事务与历史边界

候选必须保持：

- 事务前校验 owner、request key、题目版本、preview hash、action 和老师身份；
- 相同 request key + 相同 request hash 幂等返回原计划，不同 hash 拒绝；
- preview 漂移在开启事务前拒绝；
- 一个 `conn.transaction()`、一个 `tx.commit()`；
- 同一事务依次创建 impact plan、零到多条 impact task、outbox 和 audit；
- `future_only` 不创建 task，另外两种 action 只冻结对应 attempt/item/publication scope；
- 不改 assessment binding、grade decision、publication、learning evidence 或 profile snapshot；
- SQL、错误文案、payload、event type、audit note 和写入顺序不得在结构外移批中改变。

## 6. E2 失败测试前置

当前测试已覆盖成功、幂等、三种 action、preview 漂移和历史成绩/证据不变，但没有证明 plan/task 已写后 late outbox 失败会整笔回滚。

下一批 R4-E2 只允许在现有 `question_performance_tests.rs` 增加一个测试：

```text
impact_plan_outbox_failure_rolls_back_plan_tasks_audit_and_history
```

测试必须：

1. 使用 `recalculate_unpublished` 形成一条 task；
2. 冻结 plan、task、outbox、audit、grade decision、publication、learning evidence、profile snapshot 计数；
3. 对 `outbox_events` 中 `event_type='k1.question_version_impact.planned'` 注入 `BEFORE INSERT` abort，此时 plan 和 task 已在事务内写入；
4. 调用公开 use-case 并得到错误；
5. 删除 trigger 后证明全部八类计数与调用前一致；
6. 使用同一 request key 重试成功，只新增一套 plan/task/outbox/audit，四类历史事实继续不变；
7. 专项由 10/10 增为 11/11。

E2 不修改生产、Tauri、migration、Cargo 或前端。E2 通过后才允许另立 R4-S2 合同。

## 7. S2 预定边界（尚未授权）

若 E2 通过，S2 可考虑新增私有模块：

```text
question_performance/impact_plan_confirmation.rs
```

只把上述 243 行连续块机械外移，父模块保留一个私有 module 声明和原公开函数重导出。必须用施工前源块 hash、机械预期父文件、完整工程矩阵和独立 V2 复核证明同源；不得同时移动 preview、review-case prepare/resolve/publish 或 DTO。

## 8. D2 验收与非目标

D2 验收只要求：

- 重新盘点证据可复现；
- 当前 10/10 专项仍通过；
- 所有冻结哈希、HEAD/index/staged 保持；
- 展开 porcelain 只因本合同 173→174；
- `git diff --check` 与本合同 no-index whitespace 通过。

D2 不证明 E2、S2、整个 `question_performance.rs`、整个 R4、R5、老师减负、真实 provider、真实材料权利、独立 `.app`、集成或发布。
