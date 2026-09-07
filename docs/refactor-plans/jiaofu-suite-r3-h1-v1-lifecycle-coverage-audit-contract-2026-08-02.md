---
title: jiaofu-suite R3-H1-V1 lifecycle coverage audit contract
date: 2026-08-02
status: authorized_local_verification
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_index_sha256: b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b
source_fixed_intake_sha256: f6e44fc4c7c8425c82c7293157c270fc9546739c394f8be1849ea5b3e65488e3
source_lifecycle_sha256: eb6d88c54100c445bf3481a671999d1e8ec4fe820566f3302eab4e4ac20093d2
source_root_state_sha256: 8a07e1ad513f654df9a01ce156f4d142acf7bac4b8583197d4f237c336abd6c8
scope: R3-H1-V1-lifecycle-coverage-audit
production_change_authorized: false
test_assertion_change_authorized: false
controller_migration_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-H1-V1 生命周期覆盖审计合同

## 1. 目标

本批只验证 R3-H1 是否满足总设计合同冻结的完整生命周期要求，不预设通过。验证对象不是已有七个绿色场景，而是 `FixedIntakeTab.tsx` 中全部固定上传 provider 调用：

1. 同一操作键必须在 provider 前同步 claim；
2. 所有 success、failure、finished、提示和直接 continuation 必须受当前 scope/业务身份保护；
3. React StrictMode、同轮重入和旧 Promise 晚到不能制造第二条调用链或写入新 scope；
4. 17 个冻结操作族中任一操作族存在未保护调用点，H1-V1 都必须判为未通过，C1/C2 不得开工。

本批不修改生产代码、既有断言、API、Rust/SQL/DTO、依赖、样式或老师业务语义。若审计发现缺口，只记录可复现事实并拆出后续 H1-S5～S8；不得在验证批顺手修复。

## 2. 冻结审计全集

当前页面有 23 个唯一业务命令、26 个 provider 调用点。按调用链归属冻结为 17 个操作族：

| 操作族 | 同步占用键前缀 | 调用点数 |
|---|---|---:|
| 页周期推断 | `page-cycle:` | 1 |
| 批次准备 | `prepare:` | 1 |
| 答案资料分析 | `answer-source:` | 1 |
| 答案资料三类确认 | `answer-resolution:` | 3 |
| 材料类型确认 | `material:` | 1 |
| 学生归组确认 | `grouping:` | 1 |
| 归组证据读取 effect | `grouping-evidence:` | 1 |
| 质量确认及证据刷新 | `quality:` | 2 |
| 单页重拍及证据刷新 | `retake:` | 2 |
| 普通卷逐页分析 | `ordinary-analyze:` | 1 |
| 普通卷结构确认、题库旁路与题区识别 | `ordinary-confirm:` | 3 |
| 答题卡模板状态读取 effect | `answer-sheet-template-status:` | 1 |
| 答题卡模板分析、确认与状态刷新 | `answer-sheet-template:` | 3 |
| 答题卡学生页处理 | `answer-sheet-page:` | 1 |
| 默写模板状态读取 effect | `dictation-template-status:` | 1 |
| 默写模板分析与确认 | `dictation-template:` | 2 |
| 默写学生页处理 | `dictation-page:` | 1 |

质量确认、重拍和答题卡模板确认内部的读取属于发起该 continuation 的老师操作，不另算第二个 effect 操作族；同一个 owner 必须保护整条调用链。普通卷确认同理，题库同步失败继续不得阻断题区识别。

## 3. 可重复审计器

新增 `scripts/audit_exam_fixed_intake_lifecycle.mjs`，使用当前 TypeScript AST：

1. 枚举 `FixedIntakeTab.tsx` 中全部 `exam*` provider 调用；
2. 校验唯一业务命令集合仍为 23 个、调用点仍为 26 个，防止静态清单漏掉新增入口；
3. 按所属 handler/effect 映射到上述 17 个操作族；
4. 在所属函数边界内同时寻找正确的 `claimOperation` 键和 current-completion 门禁；
5. 输出每个操作族的调用点、同步 owner、stale guard 和状态；
6. 只有 17/17 操作族、26/26 调用点全部具备两类保护才退出 0；否则列出精确文件行、provider、owner 和缺失项后退出 1。

审计器只证明静态接线完整性。即使未来转绿，仍须用直接测试和浏览器风险场景证明同步双击、旧 scope 和 StrictMode 的实际行为。

## 4. 验收判定

### 4.1 H1-V1 通过

必须同时满足：

- 静态审计 17/17、26/26；
- 生命周期、根状态和纯函数直接测试全部通过；
- H1 浏览器原目标 3/3、新目标 4/4 通过且老师权威写命令为 0；
- 固定上传共享壳、全部既有批改台浏览器回归与学习洞察回归通过；
- TypeScript、Vite build、Python 脚本编译和 `git diff --check` 通过；
- 23 个业务命令清单及 hash、受保护 API/后端/依赖、HEAD/index/staged 不漂移。

### 4.2 H1-V1 未通过

只要静态审计或任何运行门禁失败：

- H1 保持进行中；
- 记录“已完成整体验证并发现缺口”，不得写成 H1 已关闭；
- C1/C2、R4～R5继续冻结；
- 后续按单一因素拆分为 H1-S5～S8，完成后另做 H1-V2，不得放宽审计器阈值或删除调用点以制造绿色。

## 5. 唯一允许修改

1. 本合同；
2. 新增只读审计器 `scripts/audit_exam_fixed_intake_lifecycle.mjs`；
3. H1-V1 验收总览、重构实施标准、文档索引、任务地图与修改记录。

不得修改 `FixedIntakeTab.tsx`、生命周期/根状态/领域 reducer、既有测试和浏览器脚本、共享壳、面板、API、后端、依赖或业务文案；不得暂存、提交、推送、tag、合并或发布。

## 6. 完成口径

本批完成口径必须跟随实际审计结果。如果存在未保护操作，只能记录为：“H1-V1 已完成完整覆盖审计，H1 尚未通过，缺口已定位并拆入后续加固批次。”自动化绿灯、行数变化或既有七个场景通过都不能覆盖该结论。
