# Jiaofu Suite R4-S2 影响计划确认 Use-case 机械外移合同（2026-08-02）

## 1. 本批唯一生产变化

R4-S2 只把 R4-D2 冻结、R4-E2 已补失败证据的 243 行连续块从：

```text
crates/module-exam/src/service/question_performance.rs
```

机械外移到新的私有模块：

```text
crates/module-exam/src/service/question_performance/impact_plan_confirmation.rs
```

父模块只新增私有 module 声明与：

```text
pub use impact_plan_confirmation::confirm_question_impact_plan;
```

不得改变公开路径、函数签名、SQL、事务、校验顺序、错误文案、payload、event/audit、DTO、Tauri caller 或测试内容。

## 2. 改前冻结基线

- 分支：`codex/t2-artifacts`
- HEAD：`6072360049b09c3155726773039da139834294fe`
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`
- staged：0；展开 porcelain：175
- 父模块：2,330 行，SHA-256 `e7ec100aad1f7d6c1fb796e25425ccd6730ed967370f4f0c153759d2d6303af7`
- E2 测试文件 SHA-256：`31facfb79dcd68526fdd527cde5e451dd8ae1327f225f0f138b4d7e7aafabd73`
- 目标第 957～1199 行共 243 行，SHA-256 `be4b3ca94e5751eb1160f2f504de7b70fdcf82ddc07d1999cb93853e6507f139`
- `knowledge_commands.rs` SHA-256：`24e448222865bd22595600a817966fc7bd575793b96ad673ea880796aea22f22`
- `service/mod.rs` / crate `lib.rs` SHA-256：`77ce93c8f471f9a60309a38f43439fc40653825bd938867b8db4cd576730b119` / `0b26882097478be45ba6284c64a37bdcf1b0f8e5e8937525cf2568d63d8dd915`
- module / Tauri / workspace Cargo SHA-256：`56e92305116957bfd1e4e84edf77b4976e0a1ea987507b4b0cc89aa20cf8b90e` / `402cb2db4c1f3d032ef5c7924df6afe4f2513518faf523e088b661fabdf2b5e3` / `b9708e762fdedb6b69ee641293aaed9305bf1f0a06a3af6466627fcda7309a20`
- migration 文件哈希清单 SHA-256：`6ec2d7fd59ea651166e27828b8a7ff9e9d2fbe555724c193c19e9b007f49bf24`
- 专项 11/11；`module-exam` 210/210。

## 3. 机械迁移边界

只移动以下四个符号，顺序和主体逐字保持：

1. `validate_action`；
2. `plan_by_id`；
3. `task_scopes`；
4. `confirm_question_impact_plan`。

新子模块在主体前只允许加入编译所需 import。主体从首个 `fn validate_action` 到文件结尾必须与施工前 243 行源块同 hash。

父模块继续持有并允许子模块读取：

- `load_target`；
- `preview_question_version_impact`；
- `ConfirmQuestionImpactPlanRequest`、`QuestionImpactPlan`、`TargetIds`、`TaskScope`；
- schema/rule 常量与其他 DTO。

不得把 preview、review-case list/prepare/resolve/publish、default-upgrade、DTO 或常量一起迁移。

## 4. 行为与事务守恒

迁移后必须重新证明：

- 唯一生产 caller 仍经 `question_performance::confirm_question_impact_plan` 调用；
- 子模块恰好一个 `conn.transaction()` 和一个 `tx.commit()`；
- plan、task、outbox、audit 写入顺序和内容不变；
- same-key idempotency、preview drift、action/owner 校验不变；
- grade decision、publication、learning evidence、profile snapshot 禁止写边界不变；
- E2 late-outbox abort 测试仍通过且测试文件 hash 不变。

## 5. 机械同源证据

施工前保存：

```text
/tmp/jiaofu-r4-s2-question-performance.before.rs
/tmp/jiaofu-r4-s2-impact-plan-source-block.rs
```

施工后必须：

1. 从子模块首个 `fn validate_action` 到 EOF 计算 hash，与源块 `be4b3ca9...f139` 相同；
2. 由施工前父文件执行固定变换——删除第 957～1199 行主体及其后一个分隔空行，在 default-upgrade 声明后插入 impact-plan module 声明/重导出——生成机械预期父文件；
3. 机械预期父文件与当前父文件 `cmp=0`；
4. 测试、Tauri、service/lib、Cargo 与 migration 哈希不变。

## 6. 验证矩阵

```text
rustfmt --edition 2021 --check --config skip_children=true \
  crates/module-exam/src/service/question_performance.rs \
  crates/module-exam/src/service/question_performance/impact_plan_confirmation.rs
cargo test -p module-exam question_performance -- --nocapture
cargo test -p module-exam
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run build
git diff --check
```

新子模块与 S2 合同另做 no-index whitespace 检查。

## 7. 完成与下一步

全部通过后，S2 只标记为“第二个 use-case 机械外移完成”，下一批必须做 R4-V2 独立只读复核。不得据此宣告整个 `question_performance.rs`、整个 R4 或重构完成。

展开 porcelain 只能因 S2 合同和新子模块 175→177；HEAD/index/staged 保持。不得暂存、提交、tag、合并、推送或发布。
