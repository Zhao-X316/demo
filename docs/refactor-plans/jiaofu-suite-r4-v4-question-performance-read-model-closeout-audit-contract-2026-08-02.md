# Jiaofu Suite R4-V4 题目表现 Read-model 拆分收口审计合同（2026-08-02）

## 1. 审计性质

R4-V4 是 S4 改后状态的独立只读复核。仓库内只允许新增本合同；不得修改父/子模块、测试、Tauri、service/lib、迁移、Cargo、前端或业务数据。

本批重新证明：

1. 621 行 read-model 主体除 `load_target` 父级可见性外机械同源；
2. 三个公开 Tauri 路径和两个 sibling caller 保持；
3. 子模块零写入、零事务，resolve/publish 与父模块唯一剩余 transaction 未动；
4. E4 `total_changes`、十类计数和 owner 边界特性测试可重复通过；
5. 完整 workspace/Tauri/Clippy/build 矩阵可重复通过。

## 2. S4 改后冻结快照

- 分支：`codex/t2-artifacts`
- HEAD：`6072360049b09c3155726773039da139834294fe`
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`
- staged：0；展开 porcelain：187
- 父模块：1,198 行，SHA-256 `c453791b465c3754a8a4cb115e91ae5764ceb3a6775a651a8219b64d67674941`
- 私有 read-model：634 行，SHA-256 `4656e8b09b31488ac46a4080c73eb70c618fdec71e6ad337c1b573d578944476`
- 归一化 621 行主体：SHA-256 `644019f522e2f88b96f8c4ac38fe76322c94905ab45f70d7e5b606b01aec87a8`
- E4 测试：1,472 行，SHA-256 `95371e13b2bd2fc9771bc8fa6c66074c5c864f0962dbf33750a2f27821677ccb`
- `knowledge_commands.rs`：SHA-256 `24e448222865bd22595600a817966fc7bd575793b96ad673ea880796aea22f22`
- `service/mod.rs` / crate `lib.rs`：`77ce93c8f471f9a60309a38f43439fc40653825bd938867b8db4cd576730b119` / `0b26882097478be45ba6284c64a37bdcf1b0f8e5e8937525cf2568d63d8dd915`
- module / Tauri / workspace Cargo：`56e92305116957bfd1e4e84edf77b4976e0a1ea987507b4b0cc89aa20cf8b90e` / `402cb2db4c1f3d032ef5c7924df6afe4f2513518faf523e088b661fabdf2b5e3` / `b9708e762fdedb6b69ee641293aaed9305bf1f0a06a3af6466627fcda7309a20`
- module-exam migration 清单：`6ec2d7fd59ea651166e27828b8a7ff9e9d2fbe555724c193c19e9b007f49bf24`
- S4 验证：专项 13/13、module-exam 212/212、workspace 485/485、Tauri 74/74、两套 Clippy/check、Vite 95 modules、rustfmt/whitespace 全绿。

## 3. 独立复核清单

- 父模块恰好一处 `mod read_model`、一个私有 `load_target` 绑定和三个公开查询重导出；
- 三个公开函数定义只存在于 read-model 子模块；
- `load_target` 只为 `pub(super)`，未成为 crate/public API；
- 父模块与 `/tmp/jiaofu-r4-s4-question-performance.expected.rs` 字节一致；
- 子模块与 `/tmp/jiaofu-r4-s4-read_model.expected.rs` 字节一致；
- 子模块归一化主体 hash 等于 D4 源块 hash；
- Tauri 与 sibling caller 路径不变；
- 子模块写 SQL、DDL、transaction、commit 扫描为 0；
- E4 测试仍含 `total_changes()`、十类计数和 other-owner 拒绝断言；
- 父模块 resolve/publish 唯一剩余 transaction/发布切换路径保持原位。

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
  crates/module-exam/src/service/question_performance/read_model.rs
git diff --check
```

全部冻结哈希、HEAD、index 和 staged 必须保持；展开 porcelain 只能因新增本合同 187→188。V4 合同另做 no-index whitespace 检查。

## 5. 完成与边界

V4 全部通过后，只能关闭“R4 第四个 read-model 拆分闭环”。下一步应先做 R4 整体阶段收口审计，复核四个子模块、父模块剩余高风险职责与 R5 前置证据；不得从 V4 直接开工 R5。

V4 不证明整个 R4、R5、老师减负、真实 provider、真实数据权利、独立 `.app`、集成、提交、合并或发布。
