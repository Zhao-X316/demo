---
title: jiaofu-suite R3-H1-S7 quality and retake lifecycle hardening contract
date: 2026-08-02
status: authorized_local_implementation
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_index_sha256: b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b
source_fixed_intake_sha256: 0715b49721730fd1205065ec0b1eb48ce07da2b2fba0a39702873c0591e49871
source_root_state_sha256: b0442cb41e8d34c1cdb7d05fcc90913e8949766ac22f5670f30ee00bd1ae917f
source_lifecycle_sha256: eb6d88c54100c445bf3481a671999d1e8ec4fe820566f3302eab4e4ac20093d2
scope: R3-H1-S7-quality-retake-lifecycle-hardening
controller_migration_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-H1-S7 质量确认与单页重拍生命周期加固合同

## 1. 单一目标

本批只加固固定上传中的两条页面质量链：

1. `confirmGroupingQuality`：原子确认页面质量后重新读取归组证据，并在普通卷时启动既有分析；
2. `replaceRejectedPage`：选择单张重拍照片、替换拒绝页、重新读取归组证据，并在激活普通卷学生时启动既有分析。

目标是让两条链在任何 provider、文件选择器或 continuation 前都有同步 owner 和当前业务身份门禁。不得同时处理普通卷自身分析/确认、答题卡模板、默写模板、controller 外移、API/Rust/SQL/DTO 或产品行为扩展。

## 2. 冻结身份与占用键

| 流程 | operation key | primary identity | continuation identity |
|---|---|---|---|
| 页面质量确认 | `quality:<batchId>` | `scopeRevision + draftRevision + batchId` | 同 primary identity |
| 单页重拍 | `retake:<batchId>` | `scopeRevision + draftRevision + batchId + rejectedPageId` | `scopeRevision + draftRevision + batchId` |

同一批次所有重拍按钮共享一个 `retake:<batchId>` owner，因为现有状态只有一个 `retakingPageId`，不允许不同拒绝页并行覆盖该状态。`rejectedPageId` 是新的专用 completion identity：它只在该页仍存在于当前归组证据，且 `qualityResult=reject`、`matchDecision=rejected` 时成立；不得放宽 H1-S4 已冻结的“逐页处理 `pageId` 必须仍是质检通过、老师确认页”语义。

## 3. 两阶段 continuation 合同

### 3.1 质量确认

1. 先同步 claim，再发送 `QUALITY_CONFIRMATION_STARTED` 和调用质量确认 provider；claim 失败静默返回。
2. provider 成功后，只有 primary identity 当前，才接受 `QUALITY_CONFIRMATION_SUCCEEDED` 并启动同批次归组证据读取。
3. 证据读取成功后，只有 primary identity 仍当前，才接受证据并在普通卷时调用既有 `analyzeOrdinaryPages`。
4. 当前质量确认成功、随后证据读取失败时，保留既有部分成功语义：质量确认结果不回滚，显示错误并结束 busy。
5. 旧成功、旧失败、旧 finished 均静默；旧链不得读取新批次、覆盖新证据或启动普通卷分析。

### 3.2 单页重拍

1. 先同步 claim，再打开文件选择器；claim 失败不得打开第二个选择器。
2. 老师取消选择时不发送 `RETAKE_STARTED`、不调用 provider，只释放 owner。
3. 文件选择返回后必须重新验证 primary identity；目标页已不再是当前拒绝页或批次已切换时，不调用替换 provider。
4. 替换成功只在 primary identity 当前时接受 `RETAKE_SUCCEEDED`；随后归组证据读取、证据写回、普通卷分析 continuation 和 `RETAKE_FINISHED` 使用 batch continuation identity，因为成功替换后旧拒绝页按设计从当前证据消失。
5. 替换或后续证据读取失败时，仅当前 primary/batch 可以调用 `onError`；旧失败静默。
6. 无论取消、失败、成功或 stale，owner 最终都必须释放；旧 owner release 不得释放后来 owner。

## 4. 行为保持边界

- provider 参数、调用顺序、老师标记拒绝页、质量确认文案、重拍文件格式和后端事务保持不变。
- 当前链成功仍刷新归组证据；普通卷仍只在质量确认后或重拍激活学生后启动原有分析。
- 当前链失败仍保留老师已选拒绝页和既有批次数据；质量确认已落库但证据刷新失败时不伪造回滚。
- 不新增自动确认、自动发布、成绩写入或老师权威写命令。
- 不修改 H1-S4 的 `scopeKey + pageId` 质检通过页门禁；被拒页使用独立 `rejectedPageId`。

## 5. 失败先行验收

生产修改前新增根状态和独立浏览器风险测试，至少覆盖：

1. 根状态只接受当前批次的质量 completion；
2. `rejectedPageId` 只接受当前证据中的拒绝页，错误页或替换后消失的旧页静默拒绝；
3. 质量确认同轮双击只调用一次确认 provider；
4. 旧质量成功/失败不写新批次、不弹错误、不启动证据读取；
5. 质量成功后的旧证据 success/failure 不写新批次、不弹错误、不启动普通卷分析；
6. 重拍同轮双击只打开一次选择器并只调用一次替换 provider；
7. 选择器返回前批次或拒绝页失效时不调用替换 provider；
8. 旧重拍成功/失败及旧证据 continuation 不写新批次、不弹错误、不启动普通卷分析；
9. 质量与重拍共 4 个 provider 调用点通过 H1 AST 覆盖审计。

## 6. 本批允许修改

- 本合同；
- `src/pages/exam/fixedIntakeLifecycle.ts`，仅增加 `rejectedPageId` 可选身份键；
- `src/pages/exam/fixedIntakeRootState.ts`，仅派生当前拒绝页身份；
- `src/pages/exam/FixedIntakeTab.tsx` 中质量确认和单页重拍两条 handler；
- `tests/frontend/fixedIntakeState.test.ts`；
- 新增 H1-S7 独立浏览器脚本；
- 验收回执、实施标准、索引、任务地图与修改记录。

不得修改 API/Rust/SQL/DTO、领域 reducer、H1-S4 `pageId` 资格规则、共享壳既有断言、展示面板、依赖或权威写命令；不得暂存、提交、tag、合并、推送或发布。

## 7. 完成口径

本批只在 4 个调用点从审计缺口中移除、专项和既有回归全绿、23 个业务命令及受保护文件不漂移时记为 H1-S7 本地通过。即使通过，H1 仍未完成；静态总覆盖预期只从 11/17、13/26 提升为 13/17、17/26，下一批只能进入 H1-S8 普通卷分析与确认链。
