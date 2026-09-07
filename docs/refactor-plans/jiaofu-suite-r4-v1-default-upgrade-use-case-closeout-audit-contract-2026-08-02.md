# Jiaofu Suite R4-V1 首个 Rust Use-case 拆分收口审计合同（2026-08-02）

## 1. 审计性质

R4-V1 是 R4-S1 改后状态的独立只读复核。仓库内只允许新增本合同；不得修改父/子模块、测试、Tauri、service/lib、迁移、Cargo、前端或任何业务代码。

本批目标是重新证明：

1. 首个 use-case 的主体机械同源；
2. 原公开模块路径和唯一 Tauri caller 未变；
3. transaction owner、SQL、写入顺序、事件/审计和历史禁止写边界未变；
4. E1 失败测试及完整工程矩阵可从零复现；
5. 工作树只有 V1 合同这一条新增仓库路径。

## 2. S1 改后冻结快照

- 分支：`codex/t2-artifacts`
- HEAD：`6072360049b09c3155726773039da139834294fe`
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`
- 展开 porcelain：172；staged：0
- 父模块：2,330 行，SHA-256 `e7ec100aad1f7d6c1fb796e25425ccd6730ed967370f4f0c153759d2d6303af7`
- 私有子模块：480 行，SHA-256 `8b7217f235c2135858090be09e8004c1993c8d85b36dacdac69388e283cb39a2`
- 子模块 467 行主体 SHA-256：`694cbca1c3222e175dfbedc1b9a668242d2cac929d59ba044d99d4adfc12b857`
- E1 测试：1,210 行，SHA-256 `0b001785de89ae03b6dc3e65ed44fc725aec805d09d133af659a3f8c6d5fd05a`
- `knowledge_commands.rs` SHA-256：`24e448222865bd22595600a817966fc7bd575793b96ad673ea880796aea22f22`
- `service/mod.rs` / crate `lib.rs` SHA-256：`77ce93c8f471f9a60309a38f43439fc40653825bd938867b8db4cd576730b119` / `0b26882097478be45ba6284c64a37bdcf1b0f8e5e8937525cf2568d63d8dd915`
- module / Tauri / workspace Cargo SHA-256：`56e92305116957bfd1e4e84edf77b4976e0a1ea987507b4b0cc89aa20cf8b90e` / `402cb2db4c1f3d032ef5c7924df6afe4f2513518faf523e088b661fabdf2b5e3` / `b9708e762fdedb6b69ee641293aaed9305bf1f0a06a3af6466627fcda7309a20`
- migration 清单 SHA-256：`6ec2d7fd59ea651166e27828b8a7ff9e9d2fbe555724c193c19e9b007f49bf24`
- S1 验证：专项 10/10、module-exam 209/209、workspace 482/482、Tauri 74/74、两套 Clippy、Tauri check、Vite 95 modules、whitespace 全绿。

## 3. 结构与事务复核清单

- 父模块恰好一处 `mod default_upgrade` 与一处 `pub use default_upgrade::upgrade_assessment_default_from_impact`；
- 公开函数定义只存在于私有子模块；
- Tauri caller 仍只调用父模块原公开路径；
- 子模块主体 hash 与 `/tmp/jiaofu-r4-s1-default-upgrade-source-block.rs` 相同；
- 当前父模块与 `/tmp/jiaofu-r4-s1-question-performance.expected.rs` 字节一致；
- 子模块恰好一个 `conn.transaction()`、一个 `tx.commit()`；
- 子模块仍写 version、items、selection、assessment timestamp、outbox、audit；
- 子模块不出现 attempt、grade decision、publication、learning evidence、profile snapshot 的写语句；
- E1 测试仍含 selection-insert abort trigger、全量回滚断言和新 request key 重试。

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
  crates/module-exam/src/service/question_performance/default_upgrade.rs
git diff --check
```

全部冻结哈希、HEAD、index 和 staged 必须保持；展开 porcelain 只能因新增本合同 172→173。

## 5. 完成与边界

V1 全部通过后，只能把“R4 首个 Rust use-case 拆分闭环”标记完成。整个 R4 仍需重新盘点下一候选；不得把首拆方法未经审计复制到 `assessment.rs`、`subjective.rs` 或其他聚合文件。

V1 不证明老师体验改善、真实 provider、真实数据权利、独立 `.app`、R5、集成、提交、合并或发布。
