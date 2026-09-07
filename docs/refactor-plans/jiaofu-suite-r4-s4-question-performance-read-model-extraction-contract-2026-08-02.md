# Jiaofu Suite R4-S4 题目表现 Read-model 机械外移合同（2026-08-02）

## 1. 本批唯一结构变化

R4-S4 只允许把 D4 冻结、E4 测试锁定的连续 read-model 源块外移到：

```text
crates/module-exam/src/service/question_performance/read_model.rs
```

父模块只新增私有 module、三个公开函数重导出和一个私有 `load_target` 绑定。不得修改 SQL、排序、hash payload、错误文本、DTO、Tauri/sibling caller、测试内容、迁移、Cargo、前端或业务数据。

## 2. 施工前冻结

- 分支：`codex/t2-artifacts`
- HEAD：`6072360049b09c3155726773039da139834294fe`
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`
- staged：0；展开 porcelain：185
- 父模块：1,815 行，SHA-256 `73b6dfc2711e21f3332982aeae0f887e126e452b2e9e5f7a094dfc6e962a4b11`
- E4 测试：1,472 行，SHA-256 `95371e13b2bd2fc9771bc8fa6c66074c5c864f0962dbf33750a2f27821677ccb`
- 连续源块：父模块第 559～1179 行，共 621 行，SHA-256 `644019f522e2f88b96f8c4ac38fe76322c94905ab45f70d7e5b606b01aec87a8`
- `knowledge_commands.rs`、service/lib、三份 Cargo 与 migration 清单保持 D4/E4 冻结值；
- 专项 13/13；module-exam 212/212。

## 3. 允许的机械操作

1. 新增 `mod read_model`；
2. 从父模块原路径公开重导出：
   - `list_question_performance`
   - `preview_question_version_impact`
   - `list_question_impact_review_cases`
3. 父模块私有绑定 `read_model::load_target`，供既有 `impact_plan_confirmation` sibling 继续通过 `super::load_target` 使用；
4. 子模块增加编译所需 import；
5. `load_target` 定义只增加 `pub(super)` 可见性，使父模块能建立私有绑定。该可见性仅到父级，不得升级成 crate/public API；
6. 删除父模块原 621 行连续块。

除 `load_target` 的 `pub(super)` 标记外，子模块业务主体归一化后必须与 D4 源块字节一致。

## 4. 机械预期

- 预期父模块：1,198 行，SHA-256 `c453791b465c3754a8a4cb115e91ae5764ceb3a6775a651a8219b64d67674941`；
- 预期私有子模块：634 行，SHA-256 `4656e8b09b31488ac46a4080c73eb70c618fdec71e6ad337c1b573d578944476`；
- 子模块去掉 13 行 import/header、把 `pub(super) fn load_target` 归一化为 `fn load_target` 后，621 行主体 SHA-256 必须恢复 `644019f522e2f88b96f8c4ac38fe76322c94905ab45f70d7e5b606b01aec87a8`；
- 当前父模块必须与 `/tmp/jiaofu-r4-s4-question-performance.expected.rs` 字节一致；
- 当前子模块必须与 `/tmp/jiaofu-r4-s4-read_model.expected.rs` 字节一致。

## 5. 行为和接口不变量

- 三个 Tauri command 继续调用 `question_performance::*` 原路径；
- `impact_plan_confirmation` 与 `review_case_preparation` sibling caller 不改；
- 三个公开函数定义只存在于私有 read-model 子模块；
- read-model 子模块没有写 SQL、DDL、transaction 或 commit；
- E4 查询零写入/owner 特性测试保持原内容并继续通过；
- resolve/publish、父模块唯一剩余 transaction、grade/publication/evidence/profile 历史语义完全不动。

## 6. 验证矩阵

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

全部保护哈希、HEAD、index 和 staged 必须保持；展开 porcelain 只能因 S4 合同和新子模块 185→187。S4 合同、父/子模块和 E4 测试差异另做 no-index whitespace 检查。

## 7. 完成与下一步

验证全部通过后，S4 只标记为“read-model 已按冻结边界机械外移”。下一步只能做 R4-V4 独立收口审计；不得直接进入 R5 或继续拆 resolve/publish。

S4 不证明整个 R4、R5、老师减负、真实 provider、独立 `.app`、集成、提交、合并或发布。
