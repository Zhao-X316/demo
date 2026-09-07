# Jiaofu Suite R4-E3 待处理 Case 准备事务失败测试合同（2026-08-02）

## 1. 本批唯一代码变化

R4-E3 只允许在：

```text
crates/module-exam/src/service/question_performance_tests.rs
```

新增一个 `prepare_question_impact_review_cases` 的 late-outbox 事务失败测试。不得修改生产代码、已有测试、Tauri、service/lib、迁移、Cargo 或前端。

## 2. 改前冻结基线

- 分支：`codex/t2-artifacts`
- HEAD：`6072360049b09c3155726773039da139834294fe`
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`
- staged：0；展开 porcelain：179
- 父模块 SHA-256：`8eb07a1856590b85552ff37550f0e35ab56c47021fd4668884d569093dab25c5`
- 测试文件：1,290 行，SHA-256 `31facfb79dcd68526fdd527cde5e451dd8ae1327f225f0f138b4d7e7aafabd73`
- 候选函数第 1811～2084 行共 274 行，SHA-256 `5ac167e1616c1804ab23d800857b00672ddab9ce4b240f3ecdc73d6010f1d83d`
- `knowledge_commands.rs`、service/lib、三份 Cargo 与 module-exam migration 清单保持 D3 冻结哈希；
- 专项 11/11；module-exam 210/210。

## 3. 测试必须锁定的事实

新测试必须：

1. 建立一个含一条 `recalculate_unpublished` task 的 impact plan；
2. 在 prepare 前冻结 review case、impact task、outbox、audit、grade decision、publication、learning evidence、profile evidence link 八类计数；
3. 创建只拦截 event type `k1.question_version_impact.review_cases_prepared` 的 `BEFORE INSERT ON outbox_events` abort trigger；
4. 调用 prepare 并断言失败；此时 case insert 已发生在事务内；
5. 删除 trigger 后证明八类计数全部恢复调用前；
6. 使用相同 plan、task count 和老师 payload 成功重试；
7. 重试只新增一条 case、一条 outbox、一条 audit；impact task 与四类历史事实不变。

测试不得依赖生产测试开关，不得放宽数据库 trigger，不得改写旧测试夹具语义。

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
- 展开 porcelain 只因 E3 合同 179→180。

## 5. 完成与下一步

全部通过后，E3 只标记为“第三候选失败边界已锁定”。下一步才允许另立 R4-S3 机械外移合同；不得直接修改生产函数。

E3 不证明 S3、整个 R4、R5、老师减负、真实 provider、独立 `.app`、集成、提交、合并或发布。
