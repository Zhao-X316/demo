# Jiaofu Suite R4-E1 未来默认版本事务失败测试合同（2026-08-02）

## 1. 本批目标与边界

R4-E1 只补足 `upgrade_assessment_default_from_impact` 的事务中段失败特征证据，证明新作业版本及其题目已经写入后，默认版本选择写入失败会把本次事务内全部业务写、事件和审计一并回滚。

本批只允许修改：

- 本合同；
- `crates/module-exam/src/service/question_performance_tests.rs`。

本批禁止修改生产 Rust、Tauri command、DTO、SQL migration、Cargo 依赖、前端、老师工作流或公开模块路径；也不授权 R4-S1 生产拆分、提交、合并或发布。

## 2. 施工前冻结基线

- 分支：`codex/t2-artifacts`
- HEAD：`6072360049b09c3155726773039da139834294fe`
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`
- 展开 porcelain：169 条；staged：0
- `question_performance.rs` SHA-256：`4252cc5ea7fb64e3e835f9b7982d2046772d18cc94f7ea491f6a15d940e4e054`
- `question_performance_tests.rs` SHA-256：`88b107f2c861819c9220938832bd30ba334cbb50e1f4827941c3e4906d9f4616`
- `knowledge_commands.rs` SHA-256：`24e448222865bd22595600a817966fc7bd575793b96ad673ea880796aea22f22`
- `service/mod.rs` SHA-256：`77ce93c8f471f9a60309a38f43439fc40653825bd938867b8db4cd576730b119`
- `module-exam/src/lib.rs` SHA-256：`0b26882097478be45ba6284c64a37bdcf1b0f8e5e8937525cf2568d63d8dd915`
- module / Tauri / workspace Cargo SHA-256：`56e92305116957bfd1e4e84edf77b4976e0a1ea987507b4b0cc89aa20cf8b90e` / `402cb2db4c1f3d032ef5c7924df6afe4f2513518faf523e088b661fabdf2b5e3` / `b9708e762fdedb6b69ee641293aaed9305bf1f0a06a3af6466627fcda7309a20`
- module-exam migration 清单 SHA-256：`6ec2d7fd59ea651166e27828b8a7ff9e9d2fbe555724c193c19e9b007f49bf24`
- 改前专项：`cargo test -p module-exam question_performance -- --nocapture` 为 9/9 通过，另有 199 项被过滤。

当前脏工作树全部视为用户既有改动。本批不得暂存、提交、清理或覆盖其他文件。

## 3. 唯一新增测试

测试名固定为：

```text
future_default_upgrade_failure_rolls_back_version_items_selection_outbox_and_audit
```

测试使用既有 fixture 和公开 use-case，不增加生产测试钩子。步骤固定为：

1. 创建并确认一项 `future_only` 影响计划；
2. 在失败调用前记录 assessment version、assessment item、default selection、outbox、audit、attempt、grade decision、publication、learning evidence 的数量，以及 assessment `updated_at` 和当前默认版本；
3. 创建测试内 SQLite trigger：在 `exam_assessment_default_version_selections_v2` 的 `BEFORE INSERT` 阶段用 `RAISE(ABORT, ...)` 注入失败；
4. 调用原公开 use-case 并断言返回错误；该注入点位于新 version/items 写入并确认之后；
5. 删除 trigger；
6. 断言上述全部数量、聚合更新时间和当前默认版本与失败调用前完全一致；
7. 使用新的 request key 再次调用并成功，证明失败事务没有留下幂等键或半套业务状态；
8. 成功重试后只允许 version、item、selection、outbox、audit 各增加一条，历史 attempt、grade、publication、learning evidence 必须不变。

## 4. 验收门禁

R4-E1 至少通过：

```text
cargo test -p module-exam question_performance -- --nocapture
cargo test -p module-exam
rustfmt --edition 2021 --check crates/module-exam/src/service/question_performance_tests.rs
git diff --check
```

该测试文件施工前已是未跟踪且存在 8 处与当前 rustfmt 的历史排版差异。不得借 E1 顺手重排旧测试；若全文件 `rustfmt --check` 仍以相同 8 处退出 1，则以施工前后差异数相同、所有差异位置均在新测试区之外，以及施工前快照到当前文件的 `git diff --no-index --check` 无 whitespace 输出作为“本批未新增格式债务”的验收证据。

同时必须证明：

- 专项测试由 9/9 增为 10/10；
- `question_performance.rs`、Tauri、服务入口、crate 入口、Cargo 和 migration 哈希不变；
- HEAD、Git index 不变，staged 仍为 0；
- 展开 porcelain 只因新增本合同从 169 增为 170；原测试文件施工前已经是未跟踪文件，因此用 `/tmp` 只读快照与 `git diff --no-index` 证明本批唯一测试差异。

## 5. 完成语义

R4-E1 通过只说明事务失败回滚行为已经被可重复测试锁定。它不说明生产 use-case 已完成外移，也不证明老师真实减负、真实 provider、独立安装包、集成、发布或商业化。

只有本批全部门禁通过，下一批才允许按 R4-D 合同进入 R4-S1：机械外移单一 use-case，并保持原公开路径、SQL、事务、DTO 和调用者不变。
