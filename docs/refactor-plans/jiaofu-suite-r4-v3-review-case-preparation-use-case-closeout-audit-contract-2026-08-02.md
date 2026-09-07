# Jiaofu Suite R4-V3 待处理 Case 准备 Use-case 拆分收口审计合同（2026-08-02）

## 1. 审计性质

R4-V3 是 R4-S3 改后状态的独立只读复核。仓库内只允许新增本合同；不得修改父/子模块、测试、Tauri、service/lib、迁移、Cargo、前端或任何业务代码。

本批目标是重新证明：

1. 第三个 use-case 的 274 行主体机械同源；
2. 原公开模块路径和唯一 Tauri caller 未变；
3. transaction owner、SQL、case/outbox/audit 写入顺序和历史禁止写边界未变；
4. E3 late-outbox 失败测试及完整工程矩阵可重复通过；
5. 工作树只因 V3 合同增加一条仓库路径。

## 2. S3 改后冻结快照

- 分支：`codex/t2-artifacts`
- HEAD：`6072360049b09c3155726773039da139834294fe`
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`
- 展开 porcelain：182；staged：0
- 父模块：1,815 行，SHA-256 `73b6dfc2711e21f3332982aeae0f887e126e452b2e9e5f7a094dfc6e962a4b11`
- 私有子模块：287 行，SHA-256 `d9b2e990cfdb3efdffb273289777db3015813783857422aa020117cbae1fa88d`
- 子模块 274 行主体 SHA-256：`5ac167e1616c1804ab23d800857b00672ddab9ce4b240f3ecdc73d6010f1d83d`
- E3 测试：1,383 行，SHA-256 `5b56980d80ccd712f15b1bbcb4fd52cb764a844d4829b15b3e3f8effab72a6c0`
- `knowledge_commands.rs` SHA-256：`24e448222865bd22595600a817966fc7bd575793b96ad673ea880796aea22f22`
- `service/mod.rs` / crate `lib.rs` SHA-256：`77ce93c8f471f9a60309a38f43439fc40653825bd938867b8db4cd576730b119` / `0b26882097478be45ba6284c64a37bdcf1b0f8e5e8937525cf2568d63d8dd915`
- module / Tauri / workspace Cargo SHA-256：`56e92305116957bfd1e4e84edf77b4976e0a1ea987507b4b0cc89aa20cf8b90e` / `402cb2db4c1f3d032ef5c7924df6afe4f2513518faf523e088b661fabdf2b5e3` / `b9708e762fdedb6b69ee641293aaed9305bf1f0a06a3af6466627fcda7309a20`
- module-exam migration 清单 SHA-256：`6ec2d7fd59ea651166e27828b8a7ff9e9d2fbe555724c193c19e9b007f49bf24`
- S3 验证：专项 12/12、module-exam 211/211、workspace 484/484、Tauri 74/74、两套 Clippy、Tauri check、Vite 95 modules、rustfmt 与 whitespace 全绿。

## 3. 结构与事务复核清单

- 父模块恰好一处 `mod review_case_preparation` 与一处 `pub use review_case_preparation::prepare_question_impact_review_cases`；
- 公开函数定义只存在于私有子模块；
- 唯一生产 caller 仍只调用父模块原公开路径；
- 子模块主体 hash 与 `/tmp/jiaofu-r4-s3-review-case-preparation-source-block.rs` 相同；
- 当前父模块与 `/tmp/jiaofu-r4-s3-question-performance.expected.rs` 字节一致；
- 子模块恰好一个 `conn.transaction()`、一个 `tx.commit()`；
- 子模块仍只冻结 review case 并追加 outbox/audit；
- 子模块不出现 grade decision、publication、learning evidence 或 profile snapshot/link 的写语句；
- E3 测试仍含指定 review-cases outbox abort trigger、八类计数回滚、相同 plan 请求重试和历史事实不变断言。

## 4. 独立验证矩阵

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
  crates/module-exam/src/service/question_performance/review_case_preparation.rs
git diff --check
```

全部冻结哈希、HEAD、index 和 staged 必须保持；展开 porcelain 只能因新增本合同 182→183。V3 合同另做 no-index whitespace 检查。

## 5. 完成与边界

V3 全部通过后，只能把“R4 第三个 Rust use-case 拆分闭环”标记完成。整个 R4 的下一动作必须重新做剩余职责与风险盘点；不得因父文件仍较大就自动继续拆分。

V3 不证明老师体验改善、真实 provider、真实数据权利、独立 `.app`、R5、集成、提交、合并或发布。
