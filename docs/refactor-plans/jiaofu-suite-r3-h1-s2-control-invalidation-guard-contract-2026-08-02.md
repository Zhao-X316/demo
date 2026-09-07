---
title: jiaofu-suite R3-H1-S2 control invalidation and completion guard contract
date: 2026-08-02
status: authorized_local_state_foundation_and_connection
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_index_sha256: b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b
source_state_sha256: 5fdb7b5761026f3dd8e0bf5b82b8637663a78719203478c62226de1add87a602
source_state_test_sha256: c51687ba4081eef7d36a7920bff3b0be672f701059c143182517c70cbd8f96dc
source_fixed_intake_sha256: 5a7e7cc9bff2f48a526aabcccacc98f4981211dd45245956dbdf1ec1481f3132
source_lifecycle_sha256: a3fd7b901a3d507473d3d801dd717b15c9bb2efa922de473c7c763b56723f6d5
source_lifecycle_test_sha256: a70a933043cbbbdda4312624a393fec128b960b8676f7cdfba78649d4fd099e1
source_lifecycle_ui_sha256: 12eb1e54202e5e91f50e335ce5ffab539e01324d3d36c5abdb6288c77a2a1275
scope: R3-H1-S2-root-control-atomic-invalidation-completion-guard
prepare_connection_authorized: false
page_cycle_connection_authorized: false
operation_registry_connection_authorized: false
effect_change_authorized: false
controller_migration_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-H1-S2 control、原子失效与 completion guard 合同

## 1. 目标

本批只完成固定上传生命周期状态基础：

1. 以一个纯根 reducer 组合现有 answerSource reducer 与 batch workflow reducer，消除生产中的双 reducer 写入边界；
2. 增加 `control.scopeRevision / draftRevision / activeOperations` UI 投影；
3. 增加一次 reducer action 内同时修改 context/draft、递增 revision、清空答案结构与全部处理投影的原子失效事件；
4. 增加统一 guarded completion 包装，stale completion 返回原 state，不能清除当前 scope 的 busy 或写回结果；
5. 把现有生产 dispatch 机械切到根 reducer，但本批不让页面开始发原子失效或 guarded completion action。

因此，本批完成后 S2 纯状态目标必须转绿，既有可见行为保持；旧页周期覆盖、旧 prepare 回填和同轮双调用三项浏览器风险仍应为 RED。它们只能在 H1-S3/S4 接线后转绿。

## 2. 改前事实

- `FixedIntakeTab` 当前分别持有 answerSource reducer 与 batch workflow reducer；一次输入失效需要多个 dispatch，无法在同一 state transition 清除两个域。
- batch workflow state 没有 revision 或 active operation 投影。
- `PREPARE_SUCCEEDED/FINISHED`、页周期和其他异步 completion 可直接 dispatch，不经过 identity 校验。
- `fixedIntakeLifecycle.ts` 已提供 5/5 通过的 owner-token registry 与 completion identity predicate，但没有生产消费者。
- H1 浏览器脚本仍观测 `new=0/stale=1`、旧 prepare 结果 1、同轮 provider 调用 2。

## 3. 根状态与 reducer

在独立的 `fixedIntakeRootState.ts` 新增扁平根状态 `FixedIntakeState`：

```text
answerSource
context
draft
batch
grouping
quality
ordinary
answerSheet
dictation
control
```

`control` 固定为：

```text
scopeRevision: number
draftRevision: number
activeOperations: string[]
```

初始 revision 均为 0，activeOperations 为空且每次工厂调用返回不同数组。真实 Promise、token、AbortController 不进入 state。

根 reducer 对既有 action 只做组合路由：answerSource action 只改变 answerSource，batch workflow action 只改变既有 workflow 域；现有两个纯 reducer 继续留在 `fixedIntakeState.ts`，作为不改实现、不改哈希的单域边界。生产由两个 `useReducer` 机械替换为一个根 `useReducer`，现有 dispatch 调用名可作为同一个 dispatch 的局部别名，API 参数、顺序、消息和按钮不变。根编排不得塞回已经 1117 行的领域状态文件，避免本次重构继续放大大文件。

## 4. 原子失效事件

新增 `FIXED_INTAKE_SCOPE_INVALIDATED`，mutation 仅允许：

```text
class_selected
assessment_selected
student_paths_selected
expected_pages_changed
answer_file_selected
answer_text_changed
answer_cleared
```

共同规则：

1. 有效 mutation 在一次 root reducer transition 内使 `scopeRevision + 1`；draft mutation 同时使 `draftRevision + 1`；
2. class/assessment mutation 保留完整 draft；学生路径 mutation 写新路径并立即清 `pageCycle`，但保留当前 `expectedPages`；
3. answer file/text mutation在同一 transition 内保持互斥；clear 同时清空文件与文本；
4. answerSource 重置为初始状态；batch/grouping/quality/ordinary/answerSheet/dictation 全部回到无处理投影、无 busy 的初始状态；
5. context 使用 mutation 后的新值，draft 使用上述保留/修改结果；
6. `activeOperations` 保留，因为旧 Promise 仍在飞行，只是其 completion 已因 revision 失效；后续 finally 才由 registry 释放并刷新投影；
7. class/assessment 等值、expectedPages 等值、答案等值/已清空时返回同一 state，不递增 revision。重新选择学生路径视为明确的新输入事件，即使路径文本相同也要失效旧 scope。

本批只实现并直接测试该事件，不把现有 class/assessment/文件/答案 UI handler 改为发该事件，避免与 H1-S3 的页周期/prepare completion 接线拆开后形成半安全行为。

## 5. active operation UI 投影

新增 `FIXED_INTAKE_ACTIVE_OPERATIONS_CHANGED`：

- 复制传入 key 数组，不持有 registry 内部集合；
- 内容逐项相同则返回同一 state；
- 只改变 control.activeOperations，不改变 revision 或业务域；
- 本批没有 registry 订阅生产接线，因此正常 UI 不出现新文案或按钮变化。

## 6. guarded completion

新增 `FIXED_INTAKE_COMPLETION_RECEIVED`：

```text
identity: FixedIntakeCompletionIdentity
completion: FixedIntakeCompletionAction
```

根 reducer 用当前 state 构造：

```text
scopeRevision
draftRevision
batchId = 当前 batch.result?.batchId
```

然后调用 H1-S1 的 `isFixedIntakeCompletionCurrent`：

- 任一 expected 身份不匹配，原样返回当前 root state；
- 匹配时只把内部 completion 路由给原有单域 reducer；
- stale success/failure/finished 均不能写结果、提示投影或解除新 scope busy；
- pageId/runId/scopeKey 只有在 root state 具备对应当前事实后才允许调用者携带；S2 不伪造这些身份，后续按材料 slice 另补业务门禁。

内部 completion 类型只能取已冻结的异步成功、失败、finished 与页周期完成 action，不能包装 started 或老师输入 action。

本批只实现并直接测试 guard；生产异步调用仍用既有直接 dispatch，H1 浏览器风险不得意外转绿。

## 7. 红绿测试

先修改 `tests/frontend/fixedIntakeState.test.ts` 新增六项根状态目标：

1. 根初始状态组合两个既有域并提供独立 control 数组；
2. 根 reducer 路由既有 answerSource/workflow action 时只改对应域；
3. context 等值无操作，真实 context 失效只递增 scope、保留 draft、清空全部处理投影；
4. 学生路径与答案 mutation 递增 draft、保持页数/互斥并原子清空全部处理投影；
5. activeOperations 复制输入且不改变 revision；
6. stale completion 不写回也不解除当前 busy，current completion 正常复用既有 reducer 语义。

测试必须先在根状态/事件未实现时稳定红，再实现转绿。既有 35 项测试不得删除、放宽或改写为新政策；新增后目标为 41/41。

## 8. 唯一允许修改

1. 本合同；
2. `tests/frontend/fixedIntakeState.test.ts`；
3. 新增 `src/pages/exam/fixedIntakeRootState.ts`，仅承载根状态、control、原子失效与 completion guard；
4. `src/pages/exam/FixedIntakeTab.tsx`，仅把两个 reducer 机械组合为一个根 reducer 与 dispatch；
5. H1-S2 验收总览、重构标准、文档索引、任务地图与修改记录。

不得修改 `fixedIntakeState.ts`、`fixedIntakeLifecycle.ts`、其 5 项测试、浏览器 H1 脚本、共享 mock、上传表单、API、三材料面板、Rust/SQL/DTO、依赖、样式或业务文案。

## 9. 必跑验收

1. 新测试改前稳定红，改后 `fixedIntakeState.test.ts` 41/41；
2. `fixedIntakeLifecycle.test.ts` 5/5、`examPure.test.ts` 6/6；
3. `npx tsc --noEmit`、`npm run build`；
4. H1 浏览器脚本仍三项 RED，证明 prepare/page-cycle/registry 未偷接；
5. 十三个既有 UI 回归与学习洞察额外保护全部为绿；
6. `git diff --check`；既有领域 reducer、helper、生命周期测试/脚本、共享壳、API、依赖哈希不变；
7. HEAD、Git index 与 staged 保持不变，展开 porcelain 111→113，只新增本合同和 `fixedIntakeRootState.ts` 两条路径。

## 10. 停止条件与完成口径

若实现需要修改 API、请求顺序、effect、页周期/prepare handler、operation registry、业务文案、老师门禁、后端或依赖，立即停止并另立 H1-S3/S4 合同。

全部验收后只能记录为：“H1-S2 单一根状态、control slice、原子失效事件和 completion guard 已实现并生产组合现有 reducer；现有调用尚未使用 lifecycle action，三项浏览器风险仍存在。”不得写成 H1、StrictMode、真实 AI/OCR、老师减负、controller、集成或发布已完成。
