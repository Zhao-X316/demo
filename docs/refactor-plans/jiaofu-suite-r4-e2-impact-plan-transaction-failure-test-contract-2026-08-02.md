# Jiaofu Suite R4-E2 影响计划事务晚期失败测试合同（2026-08-02）

## 1. 本批唯一目标

R4-E2 只在 `crates/module-exam/src/service/question_performance_tests.rs` 增加一个事务失败测试，锁定 `confirm_question_impact_plan` 在 plan/task 已写后的 late-outbox 失败回滚。生产 Rust、Tauri、service/lib、SQL/迁移、Cargo、前端和既有测试内容全部冻结。

## 2. 改前冻结基线

- 分支：`codex/t2-artifacts`
- HEAD：`6072360049b09c3155726773039da139834294fe`
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`
- staged：0；展开 porcelain：174
- 父模块 SHA-256：`e7ec100aad1f7d6c1fb796e25425ccd6730ed967370f4f0c153759d2d6303af7`
- 测试文件：1,210 行，SHA-256 `0b001785de89ae03b6dc3e65ed44fc725aec805d09d133af659a3f8c6d5fd05a`
- `knowledge_commands.rs` SHA-256：`24e448222865bd22595600a817966fc7bd575793b96ad673ea880796aea22f22`
- `service/mod.rs` / crate `lib.rs` SHA-256：`77ce93c8f471f9a60309a38f43439fc40653825bd938867b8db4cd576730b119` / `0b26882097478be45ba6284c64a37bdcf1b0f8e5e8937525cf2568d63d8dd915`
- module / Tauri / workspace Cargo SHA-256：`56e92305116957bfd1e4e84edf77b4976e0a1ea987507b4b0cc89aa20cf8b90e` / `402cb2db4c1f3d032ef5c7924df6afe4f2513518faf523e088b661fabdf2b5e3` / `b9708e762fdedb6b69ee641293aaed9305bf1f0a06a3af6466627fcda7309a20`
- migration 文件哈希清单 SHA-256：`6ec2d7fd59ea651166e27828b8a7ff9e9d2fbe555724c193c19e9b007f49bf24`
- 改前专项：10/10；`module-exam`：209/209。

## 3. 允许的测试

测试名固定为：

```text
impact_plan_outbox_failure_rolls_back_plan_tasks_audit_and_history
```

测试必须使用真实公开 use-case 和当前 fixture：

1. 获取当前 preview；
2. 使用 `recalculate_unpublished`，确保成功路径会创建一条 impact task；
3. 冻结以下八类计数：impact plan、impact task、outbox、audit、grade decision、publication、learning evidence、profile evidence link；
4. 对 `outbox_events` 创建带 event-type 条件的 `BEFORE INSERT` abort trigger；
5. 调用公开 use-case，故障点发生在 plan/task 已写之后；
6. 删除 trigger，断言八类计数全部回到调用前；
7. 用完全相同的 request key 与 payload 重试成功；
8. 断言 plan/task/outbox/audit 各新增一，四类历史事实不变。

禁止通过 test-only 生产分支、feature flag、mock transaction 或修改业务 SQL 实现测试。

## 4. 验证矩阵

```text
cargo test -p module-exam question_performance -- --nocapture
cargo test -p module-exam
cargo check --manifest-path src-tauri/Cargo.toml
git diff --check
```

目标：专项 10/10→11/11，模块 209/209→210/210。另需证明：

- 新增测试区 rustfmt 通过；若全测试文件仍有施工前既存差异，必须做前后同源对比，不能借机重排旧测试；
- 父模块、Tauri、service/lib、migration 和三份 Cargo 哈希不变；
- HEAD/index/staged 不变；
- 展开 porcelain 只因 E2 合同 174→175；测试文件在批前已是未跟踪路径；
- 测试文件改前/改后 no-index diff 只包含本测试且 whitespace 无输出。

## 5. 后续边界

E2 通过后只允许另立 R4-S2 合同，将 D2 冻结的 243 行连续块机械外移到私有 `impact_plan_confirmation.rs`。不得在 E2 同批修改生产，不得顺带移动 preview、case prepare/resolve/publish、DTO 或其他 Rust 模块。

E2 不证明 S2、整个 R4、R5、老师减负、真实 provider、真实材料权利、独立 `.app`、集成或发布。
