---
title: jiaofu-suite R3-H1-S5 answer source lifecycle hardening contract
date: 2026-08-02
status: authorized_local_implementation
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_index_sha256: b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b
source_fixed_intake_sha256: f6e44fc4c7c8425c82c7293157c270fc9546739c394f8be1849ea5b3e65488e3
source_root_state_sha256: 8a07e1ad513f654df9a01ce156f4d142acf7bac4b8583197d4f237c336abd6c8
scope: R3-H1-S5-answer-source-lifecycle-hardening
controller_migration_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-H1-S5 答案资料生命周期加固合同

## 1. 单一目标

本批只加固固定上传中的四个答案资料 provider 入口：

1. `analyzeAnswerSource`；
2. `confirmMatchingAnswerSource`；
3. `keepCurrentBoundAnswers`；
4. `adoptAnswerSourceAsNewVersion`。

目标是让每条调用链在 provider 前同步取得 owner，并让 success、failure、finished、答案原因码和 `onError` 只对发起时的业务身份生效。不得同时处理材料类型、归组、质量、重拍、普通卷、答题卡模板或默写模板。

## 2. 冻结身份与占用键

| 流程 | operation key | completion identity |
|---|---|---|
| 首次答案分析 | `answer-source:<batchId>:initial` | `scopeRevision + draftRevision + batchId` |
| 老师重试分析 | `answer-source:<batchId>:retry` | `scopeRevision + draftRevision + batchId` |
| 三类老师处置 | `answer-resolution:<batchId>:<sourceAiRunId>` | `scopeRevision + draftRevision + batchId + runId` |

三类处置必须共享同一个 key；同轮点击“确认一致 / 沿用当前 / 另存新版本”中的任意两个，最多只有一个 provider 调用。既有 provider 幂等键保持不变：首次仍为 `answer-source:<batchId>:structure:v3`，重试仍生成新的 `answer-source:<batchId>:retry:<uuid>`。

## 3. 行为保持矩阵

- 取得 owner 后才发送 `ANSWER_SOURCE_STARTED` 或 `ANSWER_SOURCE_RESOLUTION_STARTED`；未取得 owner 时静默返回。
- 分析成功仍保存完整 analysis、初始化 rubric 映射，并按 run/review 更新答案类 reason code。
- 分析失败仍保留旧证据与映射；只有当前身份可写安全错误和调用 `onError`。
- 三类处置成功仍只替换 review 并清除答案类 reason code；失败不清旧证据。
- current completion 的 `FINISHED` 仍解除 busy；stale completion 的 `FINISHED` 不得解除新 scope 的 busy。
- 无论 completion 是否 stale，原 owner 都必须在 provider settle 后释放；旧 owner release 不得释放后来 owner。
- 老师 rubric 映射校验仍在 provider 前执行，现有提示、参数、按钮、路由、AI/老师权限边界不变。

## 4. 失败先行验收

新增根状态和独立浏览器风险测试，至少覆盖：

1. 当前批次的分析 success/failure/finished/reason 可写，旧 batch 不可写；
2. 三类处置只接受当前 `sourceAiRunId`，旧 run 的 review/reason/finished 不可写；
3. 分析重试同轮双击只调用 provider 一次；
4. 三类处置同轮交叉点击只调用 provider 一次；
5. 切换作业后旧分析成功不回填；
6. 切换作业后旧处置失败不显示错误；
7. 四个调用点均通过 H1 AST 覆盖审计。

## 5. 本批允许修改

- 本合同；
- `src/pages/exam/FixedIntakeTab.tsx`；
- `src/pages/exam/fixedIntakeRootState.ts`；
- `tests/frontend/fixedIntakeState.test.ts`；
- 新增 H1-S5 独立浏览器脚本；
- 验收回执、实施标准、README/索引、任务地图与修改记录。

不得修改 API/Rust/SQL/DTO、领域 reducer、共享壳既有断言、面板展示、依赖或权威写命令；不得暂存、提交、tag、合并、推送或发布。

## 6. 完成口径

本批只在四个调用点从审计缺口中移除、专项和既有回归全绿、命令清单及受保护文件不漂移时记为 H1-S5 本地通过。即使通过，H1 仍未完成；静态总覆盖预期只从 7/17、7/26 提升为 9/17、11/26，下一批只能进入 H1-S6。
