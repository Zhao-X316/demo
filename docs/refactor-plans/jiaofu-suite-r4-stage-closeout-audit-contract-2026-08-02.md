# Jiaofu Suite R4 Rust Use-case 阶段收口审计合同（2026-08-02）

## 1. 审计性质

R4-A 是 R4 整体阶段的只读收口审计。仓库内只允许新增本合同；不得修改 Rust 生产、测试、Tauri、迁移、Cargo、前端或业务数据。

本批必须一次性回答：

1. R4 的 16 份 D/E/S/V 合同是否与 16 份验收回执配对；
2. 四个私有子模块是否各自有清晰职责，原公开路径是否保持；
3. 四个直接 transaction owner 是否仍与 default-upgrade、impact-plan confirmation、review-case preparation、resolve 一一对应；
4. 三类写 use-case 的 late-failure 保护和 read-model 的零写入/owner 保护是否仍可重复；
5. 父模块剩余 resolve/publish 高风险职责是否应继续原位保留；
6. R5 “统一任务壳”的真实老师成对证据前置是否已满足。

## 2. 改前冻结快照

- 分支：`codex/t2-artifacts`
- HEAD：`6072360049b09c3155726773039da139834294fe`
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`
- staged：0；展开 porcelain：188
- 父模块：1,198 行，SHA-256 `c453791b465c3754a8a4cb115e91ae5764ceb3a6775a651a8219b64d67674941`
- `default_upgrade.rs`：480 行，SHA-256 `8b7217f235c2135858090be09e8004c1993c8d85b36dacdac69388e283cb39a2`
- `impact_plan_confirmation.rs`：256 行，SHA-256 `453a10bad5444510588d26f05d3c8f3392a4314e8b18f027b2010b68292a5b99`
- `review_case_preparation.rs`：287 行，SHA-256 `d9b2e990cfdb3efdffb273289777db3015813783857422aa020117cbae1fa88d`
- `read_model.rs`：634 行，SHA-256 `4656e8b09b31488ac46a4080c73eb70c618fdec71e6ad337c1b573d578944476`
- R4 测试：1,472 行，SHA-256 `95371e13b2bd2fc9771bc8fa6c66074c5c864f0962dbf33750a2f27821677ccb`
- `knowledge_commands.rs`：SHA-256 `24e448222865bd22595600a817966fc7bd575793b96ad673ea880796aea22f22`
- service/lib：`77ce93c8f471f9a60309a38f43439fc40653825bd938867b8db4cd576730b119` / `0b26882097478be45ba6284c64a37bdcf1b0f8e5e8937525cf2568d63d8dd915`
- module / Tauri / workspace Cargo：`56e92305116957bfd1e4e84edf77b4976e0a1ea987507b4b0cc89aa20cf8b90e` / `402cb2db4c1f3d032ef5c7924df6afe4f2513518faf523e088b661fabdf2b5e3` / `b9708e762fdedb6b69ee641293aaed9305bf1f0a06a3af6466627fcda7309a20`
- module-exam migration 清单：`6ec2d7fd59ea651166e27828b8a7ff9e9d2fbe555724c193c19e9b007f49bf24`

## 3. 必须完成的结构复核

### 3.1 合同与回执

- `docs/refactor-plans/` 中 R4 前置 D/E/S/V 合同必须恰好 16 份；
- `笔记/教辅系统/验收/` 中对应 `00-R4-*` 回执必须恰好 16 份；
- 四个拆分均必须同时存在 design、failure/query test、extraction 和 independent closeout 证据。

### 3.2 公开路径和事务

- 父模块必须仅声明并重导出四个私有子模块入口，不增加 crate 对外 API；
- Tauri 的 8 个 `question_performance::*` 调用路径必须保持；
- 直接 `conn.transaction()` 总数必须为 4：三个外移写 use-case 各 1，父模块 resolve 路径 1，read-model 为 0；
- `resolve_question_impact_review_case` 和 `publish_question_impact_review_case` 必须继续定义在父模块，不因行数目标强拆。

### 3.3 特性保护

- `future_default_upgrade_failure_rolls_back_version_items_selection_outbox_and_audit`；
- `impact_plan_outbox_failure_rolls_back_plan_tasks_audit_and_history`；
- `review_case_outbox_failure_rolls_back_case_audit_and_history`；
- `read_models_are_query_only_and_preserve_owner_and_history_boundaries`。

上述四条必须全部保留并在专项 13/13 中通过。

## 4. 完整验证矩阵

```text
cargo test -p module-exam question_performance -- --nocapture
cargo test -p module-exam
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run build
rustfmt --edition 2021 --check --config skip_children=true \
  crates/module-exam/src/service/question_performance.rs \
  crates/module-exam/src/service/question_performance/default_upgrade.rs \
  crates/module-exam/src/service/question_performance/impact_plan_confirmation.rs \
  crates/module-exam/src/service/question_performance/review_case_preparation.rs \
  crates/module-exam/src/service/question_performance/read_model.rs
git diff --check
```

全部冻结哈希、HEAD、index 和 staged 必须保持；展开 porcelain 只能因新增本合同 188→189。本合同另做 no-index whitespace 检查。

## 5. R5 放行门禁

R4-A 可以在自动化全绿后把 R4 标记为“Rust use-case 结构重构完成（本地自动化，未集成、未发布）”，但不得自动开工 R5。

R5 必须先有可核验的真实老师成对试点证据：同一老师、同一批材料、同一工作量的人工基线和辅助复核，且数据权利/provider/成本/稳定性证据当前有效。合成测试、合成 `.app`、数据合同或空白脚手架不等于该证据。

若 R4-A 未发现上述真实成对证据，结论必须是“R4 本地自动化收口，R5 暂不放行”，不得用推测或合成记录补全。

## 6. 完成边界

R4-A 不是异模型审查，不证明真实老师减负、真实 provider/数据权利、独立 `.app`、R5、集成、提交、合并或发布。
