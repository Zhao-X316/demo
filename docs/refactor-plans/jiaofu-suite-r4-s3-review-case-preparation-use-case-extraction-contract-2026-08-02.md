# Jiaofu Suite R4-S3 待处理 Case 准备 Use-case 机械外移合同（2026-08-02）

## 1. 本批唯一生产变化

R4-S3 只把 R4-D3 冻结、R4-E3 已补失败保护的 274 行 `prepare_question_impact_review_cases` 函数从父模块机械外移到：

```text
crates/module-exam/src/service/question_performance/review_case_preparation.rs
```

父模块只新增私有 module 声明与：

```text
pub use review_case_preparation::prepare_question_impact_review_cases;
```

不得改变公开路径、函数签名、SQL、事务、校验顺序、错误文案、snapshot/payload、event/audit、DTO、Tauri caller 或测试内容。

## 2. 改前冻结基线

- 分支：`codex/t2-artifacts`
- HEAD：`6072360049b09c3155726773039da139834294fe`
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`
- staged：0；展开 porcelain：180
- 父模块：2,088 行，SHA-256 `8eb07a1856590b85552ff37550f0e35ab56c47021fd4668884d569093dab25c5`
- E3 测试文件：1,383 行，SHA-256 `5b56980d80ccd712f15b1bbcb4fd52cb764a844d4829b15b3e3f8effab72a6c0`
- 目标第 1811～2084 行共 274 行，SHA-256 `5ac167e1616c1804ab23d800857b00672ddab9ce4b240f3ecdc73d6010f1d83d`
- `knowledge_commands.rs`、service/lib、三份 Cargo 与 module-exam migration 清单保持 E3 冻结哈希；
- 专项 12/12；module-exam 211/211。

## 3. 机械迁移边界

只移动 `prepare_question_impact_review_cases` 函数。新子模块在主体前只允许加入编译所需 import；主体从首个 `pub fn prepare_question_impact_review_cases` 到 EOF 必须与施工前 274 行源块同 hash。

父模块继续持有并允许子模块读取：

- `ReviewTaskSnapshot`；
- `required`、`list_question_impact_review_cases`；
- `PrepareQuestionImpactReviewCasesRequest/Result`；
- schema/rule 常量与其他 DTO。

不得把 `ReviewTaskSnapshot`、list、resolve、publish、DTO、常量、测试或其他 Rust 文件一起迁移，不得扩大这些父级私有项的可见性。

## 4. 行为与事务守恒

迁移后必须重新证明：

- 唯一生产 caller 仍经 `question_performance::prepare_question_impact_review_cases` 调用；
- 子模块恰好一个 `conn.transaction()` 和一个 `tx.commit()`；
- case、outbox、audit 写入顺序和内容不变；
- same-plan idempotency、task-count/scope drift、case immutable 语义不变；
- grade decision、publication、learning evidence、profile snapshot/link 禁止写边界不变；
- E3 late-outbox abort 测试仍通过且测试文件 hash 不变。

## 5. 机械同源证据

施工前已保存：

```text
/tmp/jiaofu-r4-s3-question-performance.before.rs
/tmp/jiaofu-r4-s3-review-case-preparation-source-block.rs
```

施工后必须：

1. 从子模块首个公开函数到 EOF 计算 hash，与源块 `5ac167e1...d83d` 相同；
2. 从施工前父文件删除第 1811～2084 行函数及其后一个分隔空行，在现有 impact-plan module 声明后插入 prepare module 声明/重导出，生成机械预期父文件；
3. 机械预期父文件与当前父文件 `cmp=0`；
4. 测试、Tauri、service/lib、Cargo 与 migration 哈希不变。

## 6. 验证矩阵

```text
rustfmt --edition 2021 --check --config skip_children=true \
  crates/module-exam/src/service/question_performance.rs \
  crates/module-exam/src/service/question_performance/review_case_preparation.rs
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

新子模块与 S3 合同另做 no-index whitespace 检查。

## 7. 完成与下一步

全部通过后，S3 只标记为“第三个 use-case 机械外移完成”，下一批必须做 R4-V3 独立只读复核。不得据此宣告整个 `question_performance.rs`、整个 R4 或重构完成。

展开 porcelain 只能因 S3 合同和新子模块 180→182；HEAD/index/staged 保持。不得暂存、提交、tag、合并、推送或发布。
