---
title: jiaofu-suite R3-H1-S9 answer-sheet template lifecycle hardening contract
date: 2026-08-02
status: authorized_local_implementation
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_fixed_intake_sha256: 38e1a0dae18e8257553a295b1d569e70052a882afc615e8410a19b11354d8039
source_lifecycle_sha256: b9e7ce0ae2ea4b2e9cbb7bfa6cabef4654843461ad57517699698207334d64e5
source_root_state_sha256: 695495b58c85488c7299704fc87131cc6ea7f5d82107342bacbb68378a42f977
source_state_sha256: 5fdb7b5761026f3dd8e0bf5b82b8637663a78719203478c62226de1add87a602
source_state_test_sha256: f5e9e8a5b5821748c8c757ddc208f5574ce57c12d125875dbcd629415f1ce9f9
source_shared_shell_sha256: dcff9b4dc01f5a158129a40992f89fa972703f4abbe1d9e54b105a2c0d00e4a7
source_audit_sha256: 8454eb2159567d719e5560e5a5744d23fa4a47e4b5174653bc52b2e9111c7a22
scope: R3-H1-S9-answer-sheet-template-lifecycle-hardening
behavior_change_authorized: lifecycle_safety_only
production_authorized: false
controller_migration_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-H1-S9 答题卡模板生命周期加固合同

## 1. 本批目标

本批只加固答题卡模板人工链的三个 provider 调用点：

1. `pickAndAnalyzeAnswerSheetTemplate` 调用 `examAnswerSheetAnalyzeTemplate`；
2. `confirmAnswerSheetTemplate` 调用 `examAnswerSheetConfirmTemplate`；
3. 同一确认链调用 `examAnswerSheetTemplateStatus` 刷新整套模板状态。

H1-S4 已保护自动模板状态读取 effect 和答题卡学生页处理，本批只补人工模板链。完成后 AST 预期从 15/17 操作族、21/26 调用点提升为 16/17、24/26；H1 仍未完成，下一批只能进入 H1-S10 默写模板链。

## 2. 改前事实

当前人工模板链仅依赖 `templateBusy` 的下一次 React 渲染：

- 同一事件轮次重复点击可能打开两个文件选择器或发出两个分析/确认请求；
- 文件选择器、模板分析、模板确认或确认后状态刷新晚到时，没有 current scope/page/run 门禁；
- 旧确认链可继续刷新状态，甚至在新作业上下文启动旧学生页处理；
- success/failure/finally 可写入新 scope，清除新操作的 busy 或展示旧错误。

这只证明前端异步生命周期尚未加固，不证明正式成绩、老师终审或发布数据已经被污染。

## 3. 操作 owner

### 3.1 模板分析

选择空白卡前同步 claim：

```text
answer-sheet-template:<scopeKey>:analysis
```

同一 scope 在文件选择器或分析 provider 未结束前只能存在一个分析 owner。claim 失败时不得打开第二个文件选择器、不得发送 started、不得调用 provider。

### 3.2 模板确认

确认前同步 claim：

```text
answer-sheet-template:<scopeKey>:run:<answerSheetTemplateRunId>
```

同一分析 run 的重复确认只允许一个 provider 链。分析与确认使用不同后缀，便于审计，但 UI 的 `templateBusy` 继续保持原有单操作投影。

## 4. completion 身份

### 4.1 基础身份

人工模板操作发起时捕获：

```text
scopeRevision
draftRevision
scopeKey = answer_sheet:<batchId>:<referencePageId>
pageId = 当前缺失模板页对应的已确认学生页
```

文件选择器返回、分析 success/failure/finished、确认 success/failure、状态刷新 success/failure 只有在该基础身份仍 current 时才可继续。

### 4.2 模板 run 身份

确认链额外捕获：

```text
answerSheetTemplateRunId = 当前 templateRun.ai_run_id
```

根状态从 `state.answerSheet.templateRun?.ai_run_id` 派生当前 run。确认 provider 和状态刷新前后都必须校验该 run；旧 run 不得覆盖新 run 或触发学生页处理。

`ANSWER_SHEET_TEMPLATE_CONFIRMATION_SUCCEEDED` 按既有 reducer 语义清空当前 run，因此成功后的 `finished` 只用基础身份收口 busy；进入 success 前必须先通过完整 run 身份门禁。scope/page 已失效时，基础 finished 同样被拒绝，不能清理新 scope 的 busy。

## 5. 行为不变量

本批不得改变：

1. 只有当前答题卡材料、归组/质量已确认且存在合格目标页时才能建立模板；
2. 文件类型仍为 JPG/JPEG/PNG/WebP；取消选择不发送 started、不调用 provider；
3. 模板分析幂等键仍为 `answer-sheet-template:<assessmentVersionId>:page:<pageNo>:<UUID>`；
4. failed run 仍保存并展示安全错误，ready run 才显示老师确认按钮；
5. 模板确认顺序仍为“确认当前页 → 刷新整套模板状态 → 若整套 ready 则处理当前合格学生页”；
6. 多页模板未 ready 时只更新状态并进入下一缺失页，不提前处理学生页；
7. 学生页仍走 H1-S4 的 `answer-sheet-page:<pageId>` owner 与 scope/page guard；
8. 不自动评分、不自动终审、不自动发布，不修改后端事务、DTO 或老师权威。

## 6. 失败先行测试

### 6.1 根状态直接测试

新增一个用例验证：

- 错目标页的分析 completion 被拒绝；
- 当前目标页的分析 run 可写入；
- 当前页旧 `answerSheetTemplateRunId` 的确认 success 被拒绝；
- 当前页当前 run 的确认 success 被接受。

改生产前预期因根 identity 尚不解析答题卡模板 run 而红；不得在测试内放假实现。

### 6.2 独立浏览器测试

新增 H1-S9 专项脚本，至少覆盖十个场景：

1. 分析同轮重复点击只打开一次选择器并调用一次 provider；
2. 旧文件选择器返回后不启动分析；
3. 旧分析 success 静默；
4. 旧分析 failure 静默；
5. 确认同轮重复点击只调用一次 provider；
6. 旧确认 success 不刷新状态；
7. 旧确认 failure 不弹旧错误；
8. 旧状态刷新 success 不启动学生页处理；
9. 旧状态刷新 failure 不弹旧错误；
10. 当前确认链仍按确认→刷新→学生页处理执行一次。

脚本不得触发成绩接受、人工改分、批量确认或发布命令。

## 7. 唯一允许修改

1. 本合同；
2. `src/pages/exam/fixedIntakeLifecycle.ts`；
3. `src/pages/exam/fixedIntakeRootState.ts`；
4. `src/pages/exam/FixedIntakeTab.tsx` 中两条答题卡模板人工 handler；
5. `tests/frontend/fixedIntakeState.test.ts`；
6. 新增独立 H1-S9 浏览器脚本；
7. 本批验收总览、实施标准、任务地图、文档/修改记录索引和未审查队列。

不得修改 `fixedIntakeState.ts`、API、Rust/SQL/DTO、共享壳、答题卡展示面板、H1-S4 read effect/学生页处理、既有 UI 脚本、依赖或样式；不得暂存、提交、推送、tag、合并或发布。

## 8. 必跑门禁

1. 失败先行：直接测试新增目标必须先红，专项浏览器十场景必须在生产接线前报告 RED；
2. `node --test tests/frontend/examPure.test.ts tests/frontend/fixedIntakeLifecycle.test.ts tests/frontend/fixedIntakeState.test.ts`；
3. `npx tsc --noEmit`、`npm run build`；
4. H1-S9 专项、H1-S4 read-effect/逐页专项、固定上传共享壳和全部既有 UI 回归；
5. `python3 -m py_compile scripts/test_*_ui.py`；
6. `node scripts/audit_exam_fixed_intake_lifecycle.mjs` 预期仍因仅剩默写模板 2 点退出 1，但摘要必须为 16/17、24/26；
7. `git diff --check`、23 个业务命令及规范 hash、受保护文件 hash、HEAD/index/staged/porcelain 复核；
8. 验收后停止本地 preview 服务。

## 9. 完成口径

只有三个答题卡模板调用点从审计缺口移除、失败先行目标转绿、当前多页/整套 ready 行为和全部既有回归保持绿、工作树保护成立时，才能记录“H1-S9 本地通过”。

即使通过，H1 仍未完成；下一批只放行 H1-S10 默写模板分析/确认，不提前进入 H1-V2、C1/C2、controller 外移、真实 provider 或发布。
