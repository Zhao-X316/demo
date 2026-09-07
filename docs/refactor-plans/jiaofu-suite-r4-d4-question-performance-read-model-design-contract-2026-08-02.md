# Jiaofu Suite R4-D4 题目表现 Read-model 内聚外移设计合同（2026-08-02）

## 1. 本批性质

R4-D4 只读盘点 `question_performance.rs` 在三个已闭环写 use-case 之外的剩余职责。仓库内只允许新增本合同；不得修改生产代码、测试、Tauri、service/lib、迁移、Cargo、前端或业务数据。

本批结论是：继续拆分，但只选择一个结构因素——把连续的题目表现/版本影响 read-model 查询块内聚到私有子模块。`resolve_question_impact_review_case` 与 `publish_question_impact_review_case` 涉及成绩 revision、发布快照和学习证据切换，继续留在父模块，不进入本轮机械外移。

## 2. 冻结输入

- 分支：`codex/t2-artifacts`
- HEAD：`6072360049b09c3155726773039da139834294fe`
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`
- staged：0；展开 porcelain：183
- 父模块：1,815 行，SHA-256 `73b6dfc2711e21f3332982aeae0f887e126e452b2e9e5f7a094dfc6e962a4b11`
- 测试模块：1,383 行，SHA-256 `5b56980d80ccd712f15b1bbcb4fd52cb764a844d4829b15b3e3f8effab72a6c0`
- `impact_plan_confirmation.rs`：SHA-256 `453a10bad5444510588d26f05d3c8f3392a4314e8b18f027b2010b68292a5b99`
- `review_case_preparation.rs`：SHA-256 `d9b2e990cfdb3efdffb273289777db3015813783857422aa020117cbae1fa88d`
- `knowledge_commands.rs`：SHA-256 `24e448222865bd22595600a817966fc7bd575793b96ad673ea880796aea22f22`
- `service/mod.rs` / crate `lib.rs`：`77ce93c8f471f9a60309a38f43439fc40653825bd938867b8db4cd576730b119` / `0b26882097478be45ba6284c64a37bdcf1b0f8e5e8937525cf2568d63d8dd915`
- module / Tauri / workspace Cargo：`56e92305116957bfd1e4e84edf77b4976e0a1ea987507b4b0cc89aa20cf8b90e` / `402cb2db4c1f3d032ef5c7924df6afe4f2513518faf523e088b661fabdf2b5e3` / `b9708e762fdedb6b69ee641293aaed9305bf1f0a06a3af6466627fcda7309a20`
- module-exam migration 清单：`6ec2d7fd59ea651166e27828b8a7ff9e9d2fbe555724c193c19e9b007f49bf24`

## 3. 候选连续源块

冻结父模块第 559～1179 行，共 621 行：

```text
list_question_performance
load_target
preview_question_version_impact
list_question_impact_review_cases
```

- `/tmp/jiaofu-r4-d4-question-performance-read-model-source-block.rs`：621 行，SHA-256 `644019f522e2f88b96f8c4ac38fe76322c94905ab45f70d7e5b606b01aec87a8`
- 删除该连续块后的机械预期父文件：1,194 行，SHA-256 `a79d5bf1c24a800572ab8e638257ce086abcc6ac470619a2e236cf23dfd46172`
- 源块中没有 `INSERT`、`UPDATE`、`DELETE`、DDL、`transaction()` 或 `commit()`；三个公开入口均只读。

## 4. 调用与接口边界

- `list_question_performance`：唯一生产 caller 为 `src-tauri/src/knowledge_commands.rs`；测试直接调用父模块路径。
- `preview_question_version_impact`：Tauri caller 与 `impact_plan_confirmation` sibling use-case 共用；测试直接调用父模块路径。
- `list_question_impact_review_cases`：Tauri caller 与 `review_case_preparation` sibling use-case 共用；测试直接调用父模块路径。
- `load_target`：仅由 impact preview 与 `impact_plan_confirmation` 使用，是 read-model 连续块内唯一需要向父级保持 crate-private 可见性的 helper。

S4 如实施，只能新增私有 `read_model` 子模块，并由父模块保持原有三个公开函数路径；`load_target` 只允许通过父级 `pub(super)` 重导出供既有 sibling 使用，不得变为 crate 对外公开 API。Tauri、测试和 sibling caller 均不得改调用路径。

## 5. 依赖与所有权

Read-model 子模块允许从父模块复用：

```text
required
canonical_rate
accessible_question_clause
load_target_components
TargetIds
各 read-model DTO
schema/rule version 常量
```

DTO 与共享查询 helper 继续由父模块所有；本轮不为降低行数移动 DTO，不提升任何 helper 可见性，不修改 SQL、排序、hash payload、boundary note、错误文本或时间语义。

## 6. 明确不选的高风险职责

- `resolve_question_impact_review_case` 自己持有唯一剩余父模块 transaction，事务内创建新 grade decision、resolution、outbox 和 audit，并保护既有发布及学习证据不提前切换。
- `publish_question_impact_review_case` 委托 `assessment::publish_attempt` 切换 publication 与 learning evidence，并在返回后验证目标 decision 已进入正式发布快照。
- 两者共享 review scope、历史 revision 与发布一致性约束，不适合与只读查询一并机械移动。

因此 D4 不把“父文件继续变小”当作充分理由；resolve/publish 保持原位，后续是否再拆必须重新独立设计和补足事务失败证据。

## 7. E4 测试先行要求

生产外移前必须先增加一条 read-model 特性测试，至少证明：

1. 依次调用三个公开查询时 SQLite `total_changes` 不增加；
2. grade decision、publication、learning evidence、profile link、impact plan/task、review case/resolution、outbox、audit 计数不变；
3. owner 访问边界不因 read-model 聚合而放宽；
4. 列表仍只消费当前有效发布的老师确认评分，preview/case 输出仍保留既有历史边界；
5. 测试只建立证据，不修改生产代码。

E4 通过后才能进入 S4。S4 必须用源块 hash 和机械预期父文件做字节级同源验证；禁止顺手改名、整理 SQL、调整 import、修改 DTO 或重写错误信息。

## 8. 最低验证

```text
cargo test -p module-exam question_performance -- --nocapture
git diff --check
```

全部冻结生产/测试哈希、HEAD、index 和 staged 必须保持；展开 porcelain 只能因本合同 183→184。D4 合同另做 no-index whitespace 检查。

## 9. 完成与边界

D4 完成只代表“选择并冻结 read-model 外移候选”。它不代表生产代码已拆分，也不证明老师减负、真实 provider、真实数据权利、独立 `.app`、R5、集成、提交、合并或发布。
