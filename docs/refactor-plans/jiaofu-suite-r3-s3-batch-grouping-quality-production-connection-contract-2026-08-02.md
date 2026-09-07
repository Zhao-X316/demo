---
title: jiaofu-suite R3-S3 batch grouping quality reducer and production connection contract
date: 2026-08-02
status: authorized_local_behavior_preserving_refactor
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_fixed_intake_sha256: 7fb2e00ed6afaedfe49f886dcddac66dcf32a8d7838bbbb41497100518c5df46
source_state_sha256: cc1204c3dbe6915ebfea597d47c831882414e2d884fa517193196f48617c6d3d
source_state_test_sha256: 0ae7de549091644c8d30092ea50d48971fca544ebd30c4edb4bcdb9be957ae9b
source_shared_shell_sha256: dcff9b4dc01f5a158129a40992f89fa972703f4abbe1d9e54b105a2c0d00e4a7
scope: R3-S3-batch-grouping-quality-production-connection
behavior_change_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-S3 批次、归组与质量状态 reducer 及生产接线合同

## 目标

把 `FixedIntakeTab` 中 batch、grouping、quality 三个相互联动的 UI 投影合并为一个嵌套纯 reducer，并在直接测试通过后机械接入生产。请求函数、API 参数、幂等键、调用顺序、失败保留、可见文案、按钮门禁和老师确认步骤保持不变。

本批不是生命周期纠偏。当前 `resetRequest()`、选择新学生文件、expected pages 变化和异步 finally 的差异必须如实保留，原子 reset、同步防双击、StrictMode 和 stale completion 仍留给 R3-H1。

## 改前状态域

| slice | 当前字段 | 初始值 |
|---|---|---|
| `batch` | `requestKey`、`busy`、`result`、`confirmingType` | `"" / false / null / false` |
| `grouping` | `startNo`、`absentStudentNos`、`confirming`、`evidence`、`loadingEvidence` | `"" / [] / false / [] / false` |
| `quality` | `rejectedPageIds`、`confirming`、`retakingPageId` | `[] / false / null` |

共 12 个字段。它们都是当前 WebView 的瞬时投影，不是后端批次、页面归属或质量事实。

## 当前行为矩阵

1. `resetRequest()` 与选择新学生文件都会清空 `requestKey/result/startNo/absent/evidence/rejected`，但不会清除 `busy/confirmingType/confirmingGrouping/loadingEvidence/confirmingQuality/retakingPageId`；本批必须保持这一不完整 reset，不能借 reducer 原子化暗改。
2. expected pages 变化只清空 `requestKey/result`，不清归组或质量草稿。
3. prepare 开始写入或复用 `requestKey` 并置 busy；失败保留 key 和所有输入；finally 只解除 busy。
4. prepare 成功保存 result、按 `groupingFirstStudentNo → roster[0] → ""` 初始化起始学号、清空缺交并清除 requestKey；不会额外清理旧 evidence/rejected。
5. 答案资料 reason 替换只修改当前 result 的答案类 reason code；没有 result 时 no-op。
6. 材料类型确认开始/结束只控制 `confirmingType`；成功合并确认结果并显式清除 `materialTypeNeedsConfirmation`。
7. 老师修改第一份学生时同时清空缺交选择；缺交 toggle 使用非变异数组更新。
8. 归组确认成功合并 result，并清空 grouping evidence 与拒绝页草稿；普通卷、答题卡、默写投影的清理仍由原请求函数中各自现有 setter 执行。
9. 归组证据读取开始只置 loading；成功替换 evidence；失败保留旧 evidence；finally 只解除 loading。
10. quality 已完成时拒绝页 toggle no-op；未完成时只切换指定 page ID。
11. quality 确认成功先合并 result，再单独读取并替换 evidence；第二步失败时前一步 result 仍保留。
12. 重拍在老师选好文件后才写 `retakingPageId`；成功只更新 result 的 `mappedGroupCount/rejectedGroupCount/nextAction`，随后单独读取 evidence；失败保留旧 result/evidence/rejected；finally 清空 retaking。

## 纯状态合同

新增 `FixedIntakeBatchWorkflowState`，只包含嵌套 `batch/grouping/quality`，并提供：

- `createInitialFixedIntakeBatchWorkflowState()`；
- `fixedIntakeBatchWorkflowReducer(state, action)`；
- 明确的语义 action，不接收 React `SetStateAction`，不调用 API、时间、UUID 或提示。

必须覆盖的事件：

| 事件 | 结果 |
|---|---|
| `FIXED_INTAKE_SESSION_RESET` | 只清当前 reset 的 6 个字段，保留六个占用字段 |
| `PREPARE_INPUT_INVALIDATED` | 只清 `requestKey/result` |
| `PREPARE_STARTED/SUCCEEDED/FINISHED` | 保持 key、busy、result、startNo、absent 当前语义 |
| `ANSWER_SOURCE_REASON_REPLACED` | 只替换答案类 reason code |
| `MATERIAL_CONFIRMATION_STARTED/SUCCEEDED/FINISHED` | 保持材料类型确认语义 |
| `GROUPING_START_CHANGED` | 写 startNo 并清 absent |
| `ABSENT_STUDENT_TOGGLED` | 只切换指定学号 |
| `GROUPING_CONFIRMATION_STARTED/SUCCEEDED/FINISHED` | 合并 result，成功时清 evidence/rejected |
| `GROUPING_EVIDENCE_STARTED/SUCCEEDED/FINISHED` | 保持旧证据失败保留语义 |
| `QUALITY_REJECTION_TOGGLED` | 已终审 no-op，否则切换 page ID |
| `QUALITY_CONFIRMATION_STARTED/SUCCEEDED/FINISHED` | 合并 result，保留拒绝草稿 |
| `RETAKE_STARTED/SUCCEEDED/FINISHED` | 每次只占用一页，成功仅合并三个现有字段 |

所有数组更新必须不可变；两个初始状态对象不得共享数组；no-op 必须返回原 state 引用。

## 唯一允许修改

1. 本合同。
2. `src/pages/exam/fixedIntakeState.ts`：增加上述三 slice 类型、初始工厂、action 与纯 reducer。
3. `tests/frontend/fixedIntakeState.test.ts`：先增加三 slice 的直接测试，再实现生产模型。
4. `src/pages/exam/FixedIntakeTab.tsx`：删除上述 12 个独立 state/setter，接入一个 reducer；请求函数仍原地、命令顺序与参数不变。
5. `src/pages/exam/FixedIntakeUploadForm.tsx`：只把 `setRequestKey + setResult` 收窄为 `invalidatePreparedBatch()` 语义动作。
6. `src/pages/exam/FixedIntakeGroupingPanel.tsx`：只把 `setGroupingStartNo + setAbsentStudentNos([])` 收窄为 `changeGroupingStart(studentNo)` 语义动作。
7. 完成后的验收总览、重构标准、文档索引、任务地图和修改记录。

不得修改共享壳脚本、质量面板、API、Rust/SQL/DTO、样式、依赖、其他业务域 state 或其他生产组件。

## 生产接线映射

- 页面从 reducer 解构回原有局部名称，减少 JSX 扩散。
- `resetRequest()` 和选择新学生文件均 dispatch `FIXED_INTAKE_SESSION_RESET`，其他域原 setter 保持原顺序。
- expected pages 变化通过 `invalidatePreparedBatch()` dispatch `PREPARE_INPUT_INVALIDATED`。
- prepare、答案 reason、材料确认、归组确认/证据、质量确认、重拍与两个老师草稿动作逐一替换为对应事件。
- 归组确认成功后对 ordinary/answerSheet/dictation 的清理仍使用现有 setter；不得提前迁移 S4～S6。
- 请求函数、effect 和面板继续留在现文件；不得移动到 controller。

## 改前基线与保护

- 分支/HEAD：`codex/t2-artifacts@6072360049b09c3155726773039da139834294fe`。
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`；staged=0。
- 展开 porcelain：101 条。
- `FixedIntakeTab.tsx`：`7fb2e00e...df46`。
- `fixedIntakeState.ts`：`cc1204c3...d3d`。
- `fixedIntakeState.test.ts`：`0ae7de54...ae9b`。
- 共享壳脚本：`dcff9b4d...e4a7`。
- Grouping panel：`58d2256f...5897`；Quality panel：`818a3c96...21e`；Upload form：`0197b572...a0ac`。
- 23 个业务命令清单 SHA-256：`375a66bfade8b24a2d2ffb180bb934494754e4d5cff25401bd04b8e7e4358ba4`。
- baseline：direct 10/10、共享壳浏览器特征 PASS。

## 必跑验收

1. 测试先行：新增直接测试在生产模型不存在时稳定失败，再实现转绿。
2. `node --test tests/frontend/fixedIntakeState.test.ts`。
3. `node --test tests/frontend/examPure.test.ts`。
4. `npx tsc --noEmit`。
5. `npm run build`。
6. 十三个 UI 脚本全部 `py_compile` 并逐组运行，固定上传共享壳必须覆盖 prepare 重试、上下文/input reset、归组/质量/重拍、第二批三材料和 reload fail-closed。
7. 23 个业务命令名称清单与基线逐字一致；权威写命令黑名单仍为 0。
8. 静态审计证明 12 个旧 `useState` 为 0，workflow reducer 生产消费者恰为 1，两个面板 raw setter 为 0。
9. `git diff --check`、目标空白扫描、完整 porcelain、HEAD、Git index、staged 和非目标哈希保护。

## 停止条件

- 需要修改 API、Rust/SQL/DTO、命令参数/顺序、幂等键、提示、按钮或老师确认步骤；
- reset 自动清除了改前未清的 busy/confirming/loading/retaking 字段；
- quality/retake 的第二次 evidence 读取失败变成整笔前端回滚或 fail-fast；
- 请求函数被搬入新 hook，或普通卷/答题卡/默写状态被顺带迁移；
- 新增依赖、第二状态源或任一直接/浏览器/保护测试失败；
- 暂存区、HEAD、Git index 或非目标生产文件漂移。

## 完成口径

全部验收通过后，只能记为“R3-S3 batch/grouping/quality 三域 reducer 已受测并机械接入生产，现有 reset、请求和失败保留语义不变”。不得写成生命周期安全、controller、完整固定上传重构、真实 OCR/AI、老师减负、独立 `.app`、集成或发布已经完成。
