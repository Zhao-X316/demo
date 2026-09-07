---
title: jiaofu-suite R3-H1-S8 ordinary lifecycle hardening contract
date: 2026-08-02
status: authorized_local_implementation
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_index_sha256: b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b
source_fixed_intake_sha256: 48ced2111dea8a1d0a54ba462805df62b2b4ca2b694509344eff37f9c054d96a
source_root_state_sha256: 6ade5dd3911a5313ee63c759ddde7d258102f882eb0f39aab5bfa3f7b1a76726
source_lifecycle_sha256: b64571e52bbf5be204a0598c3d373354594a96f0f5d80919ae897e249f31f923
source_state_sha256: 5fdb7b5761026f3dd8e0bf5b82b8637663a78719203478c62226de1add87a602
source_state_test_sha256: 1c0279f1f67285e6ac2e0d04fdd3ad562e9407f2b6b8665fa51dc5e957159078
scope: R3-H1-S8-ordinary-lifecycle-hardening
controller_migration_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-H1-S8 普通卷分析与确认生命周期加固合同

## 1. 单一目标

本批只加固固定上传普通卷的两条异步链：

1. `analyzeOrdinaryPages`：逐页版面分析，保留首次固定幂等键、主动重试 UUID、逐页部分成功与失败汇总；
2. `confirmReadyOrdinaryPages`：逐页老师结构确认后，按原顺序执行私有题库旁路和客观题区识别。

目标是让 4 个 provider 调用点在同轮重入、切换班级/作业/上传草稿、页面失效或分析 run 被替换时，不再把旧 success/failure/finished/continuation 写入当前批次。不得同时处理答题卡模板、默写模板、controller 外移、API/Rust/SQL/DTO 或产品行为扩展。

## 2. 冻结身份与占用键

| 流程 | operation key | batch identity | page identity |
|---|---|---|---|
| 普通卷分析（首次与重试共享） | `ordinary-analyze:<batchId>` | `scopeRevision + draftRevision + batchId` | batch identity + `pageId` |
| 普通卷结构确认/题库/题区识别 | `ordinary-confirm:<batchId>` | `scopeRevision + draftRevision + batchId` | batch identity + `pageId + ordinaryRunId` |

首次分析和主动重试共享同一个批次 owner，避免 UI state 尚未提交时两个入口交叉启动。`pageId` 继续沿用 H1-S4 的严格资格：当前归组证据中仍为 `qualityResult=pass` 且 `matchDecision=teacher_confirmed`。新增 `ordinaryRunId` 只从当前 `ordinary.runs[pageId].ai_run_id` 派生；不得复用答案资料的 `runId`，也不得只因 page 相同就接受旧分析版本的确认链。

## 3. 普通卷分析合同

1. 先取得当前 batch identity 并同步 claim，再发送 `ORDINARY_ANALYSIS_STARTED`；claim 失败在任何 provider 前静默返回。
2. 本轮 pages 的筛选、顺序、首次/重试幂等键和逐页部分成功语义保持不变。
3. 每个 provider 前重新验证 page identity；批次或页面已失效时立即停止旧循环，不再调用后续页面。
4. provider success、failure、finished 均按 page identity 处理；旧 success 不写 run，旧 failure 不进入汇总，旧 finished 不清当前占用。
5. 当前页 API 抛错仍只记录该页并继续下一当前页；最终错误提示只允许当前 batch identity 触发。
6. 无论成功、失败或 stale，owner 最终必须释放；旧 owner release 不得释放后来的 owner。

## 4. 普通卷确认、题库旁路与题区识别合同

1. 先冻结 ready runs 及其 `pageId + ordinaryRunId`，同步 claim，再发送 `ORDINARY_CONFIRMATION_STARTED`；同轮第二次进入不得调用 provider。
2. 每页结构确认 provider 前后均验证 page/run identity。旧确认 success 不写 confirmation，也不启动题库同步。
3. 当前结构确认 success 仍立即写 confirmation；题库同步失败仍只加入 failures，绝不能撤销 confirmation或阻断该页题区识别。
4. 题库同步与每个题区识别 provider 前后均验证同一 page/run identity；一旦 stale，停止该旧页及后续旧页，不弹旧错误、不计入结果、不触发后续 provider。
5. 当前题区失败仍只记录该区，其他当前题区和当前页继续；逐页 finished 与最终 `onError/onDone` 只允许相应 current identity/batch 触发。
6. provider 参数、调用顺序 `confirm → sync → recognize`、计数、老师提示和题库私有候选语义全部保持不变。

## 5. 失败先行验收

生产修改前至少建立以下证据：

1. 根状态只接受当前 eligible page 的普通卷分析 completion；
2. 普通卷确认 completion 必须同时匹配当前 page 与当前 `ordinaryRunId`；同页旧 run 被替换后必须拒绝；
3. 主动重试同轮双击只产生一个分析 provider 请求；
4. 旧分析 success/failure 不写新 scope、不弹错误，也不继续调用旧列表后续页面；
5. 结构确认同轮双击只产生一个确认 provider 请求；
6. 旧确认 success/failure 不写 confirmation、不弹错误、不启动 sync/recognize 或后续页面；
7. 旧题库同步 success/failure 不启动题区识别或后续页面，failure 静默；
8. 旧题区识别 success/failure 不继续后续旧页面，failure 静默；
9. 既有普通卷题库旁路脚本继续证明 current 链严格保持 `confirm → sync → recognize`，且 sync failure 不阻断 recognize；
10. 普通卷 4 个 provider 调用点通过 H1 AST 覆盖审计。

## 6. 本批允许修改

- 本合同；
- `src/pages/exam/fixedIntakeLifecycle.ts`，仅增加 `ordinaryRunId` 可选身份键；
- `src/pages/exam/fixedIntakeRootState.ts`，仅派生当前 eligible page 的普通卷 run identity；
- `src/pages/exam/FixedIntakeTab.tsx` 中 `analyzeOrdinaryPages` 与 `confirmReadyOrdinaryPages`；
- `tests/frontend/fixedIntakeState.test.ts`；
- 新增 H1-S8 独立浏览器脚本；
- 验收回执、实施标准、索引、任务地图与修改记录。

不得修改领域 reducer、普通卷进度面板、既有普通卷题库旁路脚本、共享壳、API/Rust/SQL/DTO、答题卡/默写代码、依赖或老师权威写命令；不得暂存、提交、tag、合并、推送或发布。

## 7. 完成口径

本批只在 4 个普通卷调用点从审计缺口中移除、失败先行专项与全部既有回归全绿、23 个业务命令及受保护文件不漂移时记为 H1-S8 本地通过。即使通过，H1 仍未完成；静态总覆盖预期只从 13/17、17/26 提升为 15/17、21/26，剩余答题卡模板 3 点与默写模板 2 点，下一批只能进入 H1-S9 答题卡模板链。
