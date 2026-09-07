# Jiaofu Suite R4-S1 未来默认版本 use-case 机械外移合同（2026-08-02）

## 1. 唯一施工目标

R4-S1 只把 `question_performance::upgrade_assessment_default_from_impact` 及其专属私有结构和查询 helper 从聚合父文件机械外移到私有子模块；原公开 Rust 路径、Tauri caller、DTO、常量、SQL、事务 owner、错误文案、事件/审计内容和测试语义必须保持。

本批只允许修改：

- 本合同；
- `crates/module-exam/src/service/question_performance.rs`；
- 新增 `crates/module-exam/src/service/question_performance/default_upgrade.rs`。

`question_performance_tests.rs`、Tauri、service/lib 入口、SQL migration、Cargo、前端和其他 Rust use-case 本批全部冻结。

## 2. 施工前基线

- 分支：`codex/t2-artifacts`
- HEAD：`6072360049b09c3155726773039da139834294fe`
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`
- 展开 porcelain：170；staged：0
- 父文件：2,795 行，SHA-256 `4252cc5ea7fb64e3e835f9b7982d2046772d18cc94f7ea491f6a15d940e4e054`
- E1 测试文件：1,210 行，SHA-256 `0b001785de89ae03b6dc3e65ed44fc725aec805d09d133af659a3f8c6d5fd05a`
- 待外移源块：原父文件 2325～2791 行，共 467 行，SHA-256 `694cbca1c3222e175dfbedc1b9a668242d2cac929d59ba044d99d4adfc12b857`
- `knowledge_commands.rs` SHA-256：`24e448222865bd22595600a817966fc7bd575793b96ad673ea880796aea22f22`
- `service/mod.rs` SHA-256：`77ce93c8f471f9a60309a38f43439fc40653825bd938867b8db4cd576730b119`
- `module-exam/src/lib.rs` SHA-256：`0b26882097478be45ba6284c64a37bdcf1b0f8e5e8937525cf2568d63d8dd915`
- module / Tauri / workspace Cargo SHA-256：`56e92305116957bfd1e4e84edf77b4976e0a1ea987507b4b0cc89aa20cf8b90e` / `402cb2db4c1f3d032ef5c7924df6afe4f2513518faf523e088b661fabdf2b5e3` / `b9708e762fdedb6b69ee641293aaed9305bf1f0a06a3af6466627fcda7309a20`
- module-exam migration 清单 SHA-256：`6ec2d7fd59ea651166e27828b8a7ff9e9d2fbe555724c193c19e9b007f49bf24`
- R4-E1 专项：10/10；`module-exam`：209/209；Tauri check 通过。

施工前父文件和 467 行源块已分别复制到 `/tmp/jiaofu-r4-s1-question-performance.before.rs` 与 `/tmp/jiaofu-r4-s1-default-upgrade-source-block.rs`。临时快照只用于机械同源核对，不进入仓库。

## 3. 精确移动集合

只移动以下六项：

1. `DefaultUpgradeItemHashInput`；
2. `DefaultUpgradeSourceItem`；
3. `AssessmentDefaultUpgradeScope`；
4. `current_assessment_default`；
5. `assessment_default_upgrade_by_id`；
6. `upgrade_assessment_default_from_impact`。

父模块只新增：

```rust
mod default_upgrade;
pub use default_upgrade::upgrade_assessment_default_from_impact;
```

子模块允许增加编译所需的显式 `use`，但从第一个 `#[derive(Serialize)]` 到文件结尾的 467 行必须与施工前源块逐字节一致。不得重排 SQL、改变量名、调整错误消息、拆 helper、改变可见性或重写事务。

## 4. 必须保持的不变量

| 边界 | 不变量 |
|---|---|
| 公开路径 | `module_exam::service::question_performance::upgrade_assessment_default_from_impact` 继续可用 |
| Tauri | `k1_question_impact_upgrade_assessment` 名称、签名、mutex、参数和错误映射不变 |
| DTO/serde | `UpgradeAssessmentDefaultRequest`、`AssessmentDefaultUpgrade` 留在父模块，字段和 camelCase 不变 |
| SQL/事务 | 全部 SQL 字符串、一个 `conn.transaction()`、一个 commit、写入顺序和回滚点不变 |
| 事件/审计 | event type、aggregate、payload、note、幂等键不变 |
| 历史边界 | 不写 attempt、grade、publication、learning evidence、profile snapshot |
| 测试 | E1 测试文件哈希不变，专项仍 10/10，模块仍 209/209 |
| 依赖/迁移 | Cargo 与 migration 清单哈希不变 |

## 5. 验收顺序

施工后先做机械同源和定向验证，再从零执行完整矩阵：

```text
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

另须核对：

- 子模块主体的 SHA-256 与 467 行施工前源块一致；
- 父文件只删除该源块并增加私有模块声明/公开重导出；
- 测试、Tauri、service/lib、Cargo、migration 哈希不变；
- HEAD、Git index 不变，staged=0；
- 展开 porcelain 只增加 S1 合同和新子模块两个路径，即 170→172。

任何编译、测试、Clippy、构建、同源或工作树门禁失败，都停止在 S1 修复；不得改测试、SQL、API 或扩大范围来绕过。

## 6. 完成语义

S1 通过只说明一个完整事务 use-case 已在行为保持前提下完成私有模块外移。它不说明 `question_performance.rs` 的其他职责已经拆完，也不授权批量拆其他 Rust 文件、R5、真实 provider/老师试点、集成、提交或发布。

S1 全绿后还必须执行 R4-V1 独立只读复核，重新核对调用图、事务 owner、公开边界、机械同源、完整矩阵和工作树，才完成 R4 首个 use-case 拆分闭环。
