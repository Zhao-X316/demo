# Jiaofu Suite R4-E4 题目表现 Read-model 零写入特性测试合同（2026-08-02）

## 1. 本批唯一代码变化

R4-E4 只允许在：

```text
crates/module-exam/src/service/question_performance_tests.rs
```

新增一个同时覆盖三个公开 read-model 查询的零写入/访问边界特性测试。不得修改生产代码、已有测试、Tauri、service/lib、迁移、Cargo 或前端。

## 2. 改前冻结基线

- 分支：`codex/t2-artifacts`
- HEAD：`6072360049b09c3155726773039da139834294fe`
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`
- staged：0；展开 porcelain：184
- 父模块：1,815 行，SHA-256 `73b6dfc2711e21f3332982aeae0f887e126e452b2e9e5f7a094dfc6e962a4b11`
- 测试模块：1,383 行，SHA-256 `5b56980d80ccd712f15b1bbcb4fd52cb764a844d4829b15b3e3f8effab72a6c0`
- D4 连续 read-model 源块：621 行，SHA-256 `644019f522e2f88b96f8c4ac38fe76322c94905ab45f70d7e5b606b01aec87a8`
- `knowledge_commands.rs`、service/lib、三份 Cargo 与 module-exam migration 清单保持 D4 冻结值；
- 专项 12/12；module-exam 211/211。

## 3. 测试必须锁定的事实

新增 `read_models_are_query_only_and_preserve_owner_and_history_boundaries`，必须：

1. 先建立一条已有 impact plan/task/review case 的 fixture，使三个查询均返回非空结果；
2. 在查询前冻结 SQLite `total_changes()`；
3. 冻结 grade decision、publication、learning evidence、profile link、impact plan/task、review case/resolution、outbox、audit 十类计数；
4. 依次调用 `list_question_performance`、`preview_question_version_impact`、`list_question_impact_review_cases` 并核对当前有效发布/影响范围/case 基本输出；
5. 用其他 owner 调用，证明私有题目不进入表现列表、preview 和 plan case 查询被拒绝；
6. 全部查询后 `total_changes()` 与十类计数仍完全等于调用前。

测试不得建立生产测试开关，不得放宽 SQL/owner 条件，不得把查询失败转换为写入记录，也不得修改既有 fixture 或测试语义。

## 4. 验证矩阵

```text
cargo test -p module-exam question_performance -- --nocapture
cargo test -p module-exam
cargo check --manifest-path src-tauri/Cargo.toml
git diff --check
```

还要证明：

- 新增测试区不制造新的 rustfmt 差异；
- 父模块、Tauri、service/lib、迁移与 Cargo 哈希不变；
- HEAD/index/staged 保持；
- 展开 porcelain 只因 E4 合同 184→185；测试文件是 D4 已存在的未跟踪路径内修改，不另增路径计数。

## 5. 完成与下一步

全部通过后，E4 只标记为“read-model 查询/访问边界已由特性测试锁定”。下一步才允许另立 R4-S4 机械外移合同；不得在 E4 修改生产函数。

E4 不证明 S4、整个 R4、R5、老师减负、真实 provider、独立 `.app`、集成、提交、合并或发布。
