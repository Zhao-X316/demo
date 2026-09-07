---
title: jiaofu-suite R3-H1-S6 material and grouping lifecycle hardening contract
date: 2026-08-02
status: authorized_local_implementation
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_index_sha256: b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b
source_fixed_intake_sha256: da2945bc8003befd728b3dadb371f4da118e2a7a2f38aeaed9ae31c73de0d191
source_root_state_sha256: b0442cb41e8d34c1cdb7d05fcc90913e8949766ac22f5670f30ee00bd1ae917f
scope: R3-H1-S6-material-grouping-lifecycle-hardening
controller_migration_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-H1-S6 材料类型与学生归组生命周期加固合同

## 1. 单一目标

本批只加固固定上传中的两个老师确认入口：

1. `confirmMaterialType`；
2. `confirmGrouping`。

目标是让两个入口都在 provider 前同步取得 owner，并让 success、failure、finished 及归组成功后的三类材料局部 reset 只对发起时的当前业务身份生效。不得同时处理质量确认、重拍、普通卷、答题卡模板或默写模板。

## 2. 冻结身份与占用键

| 流程 | operation key | completion identity |
|---|---|---|
| 材料类型确认 | `material:<batchId>` | `scopeRevision + draftRevision + batchId` |
| 学生归组确认 | `grouping:<batchId>` | `scopeRevision + draftRevision + batchId` |

材料类型的三个选择必须共享同一个 key；同一事件轮点击“普通试卷 / 答题卡 / 默写”中的任意两个，最多只有一个 provider 调用。归组确认同轮重入也最多调用一次 provider。

## 3. 行为保持矩阵

- 取得 owner 后才发送 `MATERIAL_CONFIRMATION_STARTED` 或 `GROUPING_CONFIRMATION_STARTED`；未取得 owner 时静默返回。
- 当前材料确认成功仍更新材料类型、老师确认状态、归组路线和下一步；失败保留原批次与老师草稿。
- 当前归组确认成功仍更新学生起止学号和确认状态，并执行既有普通卷/答题卡/默写局部 reset；失败保留第一位学生和缺交名单。
- 旧 scope、旧 draft 或旧 batch 的 success 不得写入新批次；旧 failure 不得调用 `onError`；旧 finished 不得解除新批次的 busy。
- 旧归组 success 不得触发三类材料局部 reset，也不得借后续 effect 读取新批次归组证据。
- 无论 completion 是否 stale，原 owner 都必须在 provider settle 后释放；旧 owner release 不得释放后来 owner。
- provider 参数、老师确认按钮、材料路线、学生顺序/缺交语义、后端事务与 AI/老师权限边界保持不变。

## 4. 失败先行验收

新增根状态和独立浏览器风险测试，至少覆盖：

1. 材料与归组 completion 只接受当前 batch，旧 success/finished 不可写；
2. 材料三选一同轮交叉点击只调用 provider 一次；
3. 归组确认同轮双击只调用 provider 一次；
4. 切换作业并创建新批次后，旧材料成功不能替换新批次材料状态；
5. 切换作业后，旧材料失败不显示错误；
6. 切换作业后，旧归组成功不能确认新批次或启动新证据链；
7. 切换作业后，旧归组失败不显示错误；
8. 两个调用点均通过 H1 AST 覆盖审计。

## 5. 本批允许修改

- 本合同；
- `src/pages/exam/FixedIntakeTab.tsx`；
- `tests/frontend/fixedIntakeState.test.ts`；
- 新增 H1-S6 独立浏览器脚本；
- 验收回执、实施标准、README/索引、任务地图与修改记录。

不得修改 API/Rust/SQL/DTO、领域 reducer、根状态、生命周期 helper、共享壳既有断言、面板展示、依赖或权威写命令；不得暂存、提交、tag、合并、推送或发布。

## 6. 完成口径

本批只在两个调用点从审计缺口中移除、专项和既有回归全绿、命令清单及受保护文件不漂移时记为 H1-S6 本地通过。即使通过，H1 仍未完成；静态总覆盖预期只从 9/17、11/26 提升为 11/17、13/26，下一批只能进入 H1-S7 质量确认与单页重拍。
