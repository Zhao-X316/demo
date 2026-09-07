# Jiaofu Suite R4-D3 剩余 Rust Use-case 只读重盘合同（2026-08-02）

## 1. 本批性质

R4-D3 只重新盘点两个已闭环 use-case 外的剩余 Rust 业务边界。仓库内只允许新增本合同；不得修改生产代码、测试、Tauri、service/lib、迁移、Cargo、前端或业务数据。

本批不以文件行数、目录整齐或“继续拆分”为目标，而要回答：剩余代码中是否仍存在一个单一业务职责、专属 helper、明确 transaction owner、唯一或稳定 caller、可先补失败测试且不扩大公开接口的候选。

## 2. 冻结输入

- 分支：`codex/t2-artifacts`
- HEAD：`6072360049b09c3155726773039da139834294fe`
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`
- staged：0；展开 porcelain：178
- `question_performance.rs`：2,088 行，SHA-256 `8eb07a1856590b85552ff37550f0e35ab56c47021fd4668884d569093dab25c5`
- `impact_plan_confirmation.rs`：256 行，SHA-256 `453a10bad5444510588d26f05d3c8f3392a4314e8b18f027b2010b68292a5b99`
- `default_upgrade.rs`：480 行，SHA-256 `8b7217f235c2135858090be09e8004c1993c8d85b36dacdac69388e283cb39a2`
- `question_performance_tests.rs`：1,290 行，SHA-256 `31facfb79dcd68526fdd527cde5e451dd8ae1327f225f0f138b4d7e7aafabd73`
- `knowledge_commands.rs` SHA-256：`24e448222865bd22595600a817966fc7bd575793b96ad673ea880796aea22f22`
- module / Tauri / workspace Cargo 与 module-exam migrations 哈希保持 V2 冻结值。

## 3. 必查维度

对剩余公开函数逐项记录：

1. caller 数量和调用层；
2. 是否持有 transaction，以及事务内写表；
3. 私有 helper 是否专属于该入口；
4. 与相邻 DTO、查询、preview、prepare/resolve/publish 的耦合；
5. 当前测试覆盖的成功、幂等、漂移与失败点；
6. 是否写 grade、publication、learning evidence、profile 等历史事实；
7. 若外移，能否只保留私有 module + 原路径重导出，不扩大接口；
8. 可否先增加一个发生在最后关键写入附近的事务失败测试。

## 4. 决策规则

只允许两种结论：

- **选择一个候选**：冻结连续源块、符号清单、caller、transaction、禁止写边界和缺失失败测试；下一批只能先做 E3 测试；
- **停止 R4 继续拆分**：若剩余边界均共享 helper、跨多个历史事实或需要扩大接口，必须给出逐候选证据并把 R4 标记为“当前安全边界收口”，不得为降行数强拆。

无论哪种结论，本批都不得修改生产或测试。R5、真实老师/provider、独立 `.app`、集成和发布继续冻结。

## 5. 最低验证

```text
cargo test -p module-exam question_performance -- --nocapture
git diff --check
```

全部冻结哈希、HEAD、index 和 staged 必须保持；展开 porcelain 只能因本合同 178→179。D3 合同另做 no-index whitespace 检查。
