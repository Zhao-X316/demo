# Jiaofu Suite R4 Rust 生产 use-case 拆分设计合同（2026-08-02）

## 1. 合同地位

本文是 R3-A 收口后的 R4 设计入口。R4 的目标不是按行数切 Rust 文件，而是在公开模块路径、Tauri command、DTO、SQL、事务边界和老师权威均保持不变的前提下，把一个有完整事务所有权、可独立注入失败并能由既有调用者继续使用的生产 use-case 从聚合文件中机械外移。

本合同通过只允许开始 R4-E1 失败注入测试前置；不自动授权生产代码外移、SQL/迁移/DTO 修改、跨模块抽象、集成或发布。

## 2. 冻结基线

- 分支：`codex/t2-artifacts`
- HEAD：`6072360049b09c3155726773039da139834294fe`
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`
- 展开 porcelain：168 条；staged：0
- `question_performance.rs`：2,795 行，SHA-256 `4252cc5ea7fb64e3e835f9b7982d2046772d18cc94f7ea491f6a15d940e4e054`
- `question_performance_tests.rs`：SHA-256 `88b107f2c861819c9220938832bd30ba334cbb50e1f4827941c3e4906d9f4616`
- `knowledge_commands.rs`：SHA-256 `24e448222865bd22595600a817966fc7bd575793b96ad673ea880796aea22f22`
- `service/mod.rs`：SHA-256 `77ce93c8f471f9a60309a38f43439fc40653825bd938867b8db4cd576730b119`
- `module-exam/src/lib.rs`：SHA-256 `0b26882097478be45ba6284c64a37bdcf1b0f8e5e8937525cf2568d63d8dd915`
- module / Tauri / workspace Cargo 清单哈希：`56e92305...8b90e` / `402cb2db...5e3` / `b9708e76...a20`
- module-exam migrations 清单哈希：`6ec2d7fd59ea651166e27828b8a7ff9e9d2fbe555724c193c19e9b007f49bf24`
- `question_performance` 八个公开函数名清单哈希：`e4d677377b9612a54d42ff153847fad1ed8947ffd1d1d395e2641fb52052420e`
- 对应八个 Tauri command 名清单哈希：`53d9bd8430cbbfd4d4aeb7dbc39ce1c365ca29f8f84594d8cbf31b94213101b1`
- 改前专项：`cargo test -p module-exam question_performance -- --nocapture` 为 9/9 通过，另有 199 项被过滤。

现有脏工作树全部视为用户既有改动；R4 不得暂存、提交、覆盖、清理或借机吸收其他文件。

## 3. 大文件候选与首拆选择

只读库存显示：

| 聚合文件 | 行数 | 公开函数 | 当前耦合判断 | 本轮结论 |
|---|---:|---:|---|---|
| `subjective.rs` | 3,218 | 14 | OCR、短答分析、逐项终审、答案/rubric 晋级和工作台读模型共同存在，5 个本地事务且有 Tauri/runtime 多调用者 | 暂缓 |
| `assessment.rs` | 2,919 | 11 | 作业、attempt、评分、发布和 evidence 激活同时存在，内部 transaction helper 被错题本、图谱等跨 crate 调用 | 暂缓 |
| `question_performance.rs` | 2,795 | 8 | 表现读模型、影响计划、复核 case、再发布和未来默认版本升级并列；底部未来默认升级是单一公开入口和单一事务 | **首拆** |
| `answer_source.rs` | 2,693 | 7 | 答案冲突、采用、版本与链接携带混合，跨 K1/M2 边界 | 暂缓 |
| `papers.rs` | 2,121 | 10 | 批次、质量、匹配、配准、题区与追溯被多个 materializer 复用 | 暂缓 |
| `objective.rs` | 2,102 | 7 | OMR 观察、工作台、单条/批量终审同文件，直接承载当前老师工作台 | 暂缓 |
| `dictation_pipeline.rs` | 2,099 | 13 | 模板、物化、OCR、终审和工作台流水线耦合，直接承载当前老师工作台 | 暂缓 |

因此 R4 首个生产拆分对象固定为：

```text
module_exam::service::question_performance::upgrade_assessment_default_from_impact
```

选择原因不是它所在文件最长，而是该 use-case 具备以下边界：

1. 一个 Tauri command、一个公开 service 入口、一个事务 owner；
2. 只负责从当前默认已确认作业版本克隆未来版本并切换未来上传默认；
3. 明确不改历史 attempt、grade、publication 或 learning evidence；
4. 私有 scope/source/hash/readback helper 均集中在文件底部，可完整外移；
5. 既有两项行为测试已覆盖成功、幂等、历史不变和 stale/unreviewed 拒绝；只缺事务中段故障回滚证据。

## 4. 调用图

```text
React/K1 题目影响页
  -> Tauri command k1_question_impact_upgrade_assessment
       [只持 AppState DB mutex，不开启事务]
  -> question_performance::upgrade_assessment_default_from_impact
       -> 请求字段/当前老师/幂等 request_key 校验
       -> 读取 impact plan + source assessment version + owner
       -> current_assessment_default
       -> 读取 source items 与目标 answer/rubric/link
       -> conn.transaction()                    <- 唯一事务 owner
            -> 插入 draft assessment version
            -> 克隆 assessment items，并只替换目标题的版本引用
            -> 计算 item_set_hash 并确认新版本
            -> 插入 default version selection
            -> 更新 assessment.updated_at
            -> 同事务写 outbox
            -> 同事务写 audit
          -> commit
       -> assessment_default_upgrade_by_id 只读返回 DTO
  -> AssessmentDefaultUpgrade 经原 Tauri command 返回前端
```

调用者和公开边界：

- Tauri 只从 `src-tauri/src/knowledge_commands.rs` 的 `k1_question_impact_upgrade_assessment` 调用该函数；
- 生产路径继续使用 `module_exam::service::question_performance::upgrade_assessment_default_from_impact`；
- `UpgradeAssessmentDefaultRequest`、`AssessmentDefaultUpgrade`、规则版本常量和 serde 字段继续留在父模块公开边界；
- 新子模块必须是父模块私有实现，父模块用 `pub use` 保持原公开路径。

## 5. 事务所有权与写入不变量

| 阶段 | 读/写对象 | 所有权与不变量 |
|---|---|---|
| 事务前校验 | impact plan、source/default assessment version、source items、request-key selection | 只读；stale、跨老师、来源未确认、来源不在冻结范围、待升级题不唯一均在写前拒绝 |
| 事务内版本创建 | `exam_assessment_versions_v2` | 先 draft，item hash 完成后才 confirmed；失败不得残留 draft/confirmed 新版本 |
| 事务内题目克隆 | `exam_assessment_items_v2` | 原顺序/分值/呈现快照不变；仅目标 question 切换 answer/rubric/link；失败不得残留部分 items |
| 事务内默认选择 | `exam_assessment_default_version_selections_v2` | request key 幂等；selection 与新版本必须同成同败 |
| 事务内聚合更新时间 | `exam_assessments_v2.updated_at` | 与 selection 同事务；失败恢复原值 |
| 事务内事件 | outbox + audit | 与业务写同成同败；不得出现只有版本没有事件或只有事件没有版本 |
| 明确禁止写 | attempts、grade decisions、publications、learning evidence、profile snapshots | 成功和失败均保持数量、引用和当前有效状态不变 |

事务继续由 public use-case 自己开启和提交；Tauri command、新子模块外调用者、repo helper 均不得新增外层事务或拆成多次提交。

## 6. 公开接口影响表

| 边界 | R4-S1 允许结果 | 禁止 |
|---|---|---|
| Rust 模块路径 | 原路径继续可用；父模块 `pub use` 重导出原函数 | 改名、改参数、改返回值、让调用者改 import |
| DTO/serde | `UpgradeAssessmentDefaultRequest` 与 `AssessmentDefaultUpgrade` 字段/命名/hash 不变 | 移字段、改 camelCase、改布尔语义 |
| Tauri command | 八个相关 command 名和签名不变 | 改 command、mutex、错误映射或 IPC 名 |
| SQL/迁移 | 查询和写入 SQL 逐字机械迁移；迁移清单 hash 不变 | 新表、新列、新索引、SQL 重写 |
| 事务 | 仍为一个 `conn.transaction()` 与一个 commit | 拆事务、网络调用、跨模块写入 |
| Cargo | workspace/module/Tauri 依赖不变 | 新 crate、新 feature、新依赖 |
| 业务语义 | 只升级未来默认版本；历史全部不变 | 自动重评、自动再发布、自动改 evidence |

## 7. 失败注入缺口与 R4-E1

现有两项相关测试覆盖：

- 成功克隆、目标版本替换、默认切换、历史不变和同 request 幂等；
- stale 默认版本和不在冻结影响范围内的来源拒绝。

现有测试没有在事务已经插入新 version/items 后制造错误，因此尚不能证明中段失败会同时回滚 version、items、selection、assessment timestamp、outbox 和 audit。

R4-E1 只允许修改 `question_performance_tests.rs`，新增一项：

```text
future_default_upgrade_failure_rolls_back_version_items_selection_outbox_and_audit
```

测试步骤固定为：

1. 用现有 fixture 生成 `future_only` impact plan；
2. 记录 assessment version/item/selection、outbox、audit、attempt、grade、publication、learning evidence 数量和当前默认版本；
3. 创建仅用于测试的 `BEFORE INSERT ON exam_assessment_default_version_selections_v2` abort trigger；该位置发生在新 version/items 已写、版本已 confirmed 之后；
4. 调用原公开函数并断言错误；
5. 删除 trigger；
6. 断言全部记录数、默认版本、历史四类事实和业务事件均与调用前完全一致；
7. 再以新 request key 调用一次并成功，证明失败没有污染后续合法重试。

R4-E1 不改生产、SQL migration、Tauri、DTO 或依赖。目标专项由 9/9 增为 10/10 后，才允许 R4-S1。

## 8. R4-S1 机械拆分合同预案

R4-S1 只允许：

1. 新增私有实现文件 `crates/module-exam/src/service/question_performance/default_upgrade.rs`；
2. 将当前 `question_performance.rs` 第 2325～2792 行的三项私有结构、两个私有 read helper 和一个公开 use-case 原样移入；
3. 父模块新增私有 `mod default_upgrade;` 与 `pub use default_upgrade::upgrade_assessment_default_from_impact;`；
4. 只做编译所需的 import 可见性调整和 canonical rustfmt；
5. 不修改测试内容、公开类型/常量、SQL 字符串、错误文案、事件名、审计 note、调用者或事务边界。

机械同源必须用施工前快照证明，不能只凭“测试通过”推断。R4-S1 施工前要在 E1 已完成状态重新记录父文件、测试、Tauri、迁移和 Cargo 哈希，并逐段比对外移源块。

## 9. 分批路线

| 批次 | 唯一因素 | 生产代码 |
|---|---|---|
| R4-D | 本设计、调用图、事务表、接口影响和首拆选择 | 不改 |
| R4-E1 | 未来默认升级事务中段失败注入特征测试 | 不改 |
| R4-S1 | 单一 use-case 私有子模块机械外移与原路径重导出 | 只改父模块与新增子模块 |
| R4-V1 | 独立只读复核调用图、事务、公开接口、测试和工作树 | 不改 |

R4-V1 通过只代表首个 Rust use-case 拆分闭环，不代表整个 R4 阶段完成。后续必须重新盘点候选，不能自动把 `assessment.rs`、`subjective.rs` 或其他大文件按同样方法批量搬迁。

## 10. 工程门禁

R4-E1 至 V1 最低必须运行：

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

E1 允许先跑专项和 module-exam；S1/V1 必须从零跑完整矩阵。任一失败都停止在当前批，不改 SQL、降低断言或扩大允许范围。

## 11. 完成与停止条件

R4-D 完成必须同时满足：

- 首拆对象以调用图和事务边界选出，而非按行数；
- 唯一事务 owner、全部写表、明确禁止写对象和失败注入点已冻结；
- 原 Rust 路径、Tauri command、DTO、SQL/迁移和 Cargo 影响均为零；
- 下一批只允许 E1 测试，生产继续冻结；
- HEAD、Git index、staged 和既有生产/测试/Tauri/迁移/Cargo 哈希保持不变。

R4-D 不证明生产拆分已实现，不证明老师体验改善，也不证明真实 provider、数据、独立 `.app`、集成、发布或商业化。
