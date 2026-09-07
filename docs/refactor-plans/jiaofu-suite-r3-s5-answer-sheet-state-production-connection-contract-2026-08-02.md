---
title: jiaofu-suite R3-S5 answer-sheet state reducer and production connection contract
date: 2026-08-02
status: authorized_local_behavior_preserving_refactor
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_fixed_intake_sha256: a1a8adc0c82736cbb2c3b5abb6984c00dd3451710532355d681047c1eceaf87f
source_state_sha256: 41f0e6f30f8ee2467b0d75627488399fdbcc0f0cef43ffdfaa5fa69c0e342527
source_state_test_sha256: cefb0110270ba261f2016278adac8273949c1d9c883ea8e6b3ce0be1e1f30e78
source_answer_sheet_panel_sha256: 8b294bbb732cbb04c5a48c93277f228b5068a68de759cb17f3884b6db14f4bd5
source_shared_shell_sha256: dcff9b4dc01f5a158129a40992f89fa972703f4abbe1d9e54b105a2c0d00e4a7
scope: R3-S5-answer-sheet-state-production-connection
behavior_change_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-S5 答题卡状态 reducer 与生产接线合同

## 目标

把 `FixedIntakeTab` 中答题卡模板集与逐页处理的七个瞬时 UI 投影并入现有 workflow reducer，并在直接测试通过后机械接入生产。模板状态查询、老师上传并确认模板、模板集 ready 后串行处理页面的请求函数继续原地；命令参数、幂等键、相对顺序、逐页部分成功、失败提示和老师确认步骤保持不变。

本批不迁移默写、context/draft 或请求 controller，也不实施 R3-H1。当前 reset 不清模板 busy、effect 取消只阻止 UI 回写、同 tick 防双击、StrictMode 和 stale completion 均必须继续保留为后续行为变更边界。

## 改前状态域

| 字段 | 初始值 | 含义 |
|---|---|---|
| `answerSheetTemplateStatus` | `null` | 当前答题卡模板集状态投影 |
| `answerSheetTemplateStatusLoaded` | `false` | 模板状态查询是否已结束 |
| `answerSheetTemplateRun` | `null` | 老师上传模板后的机器分析 run 投影 |
| `answerSheetTemplateBusy` | `false` | 模板分析或确认占用 |
| `answerSheetPageResults` | `{}` | `pageId → AnswerSheetPageProcessingResult` 的逐页处理投影 |
| `answerSheetPageFailures` | `{}` | `pageId → safe error message` 的逐页失败投影 |
| `processingAnswerSheetPageIds` | `[]` | 当前逐页处理占用 |

七字段均为当前 WebView 投影，不替代后端模板、AI run、OMR/OCR、主观题转写或评分事实。

## 当前行为矩阵

1. `resetRequest()`、选择新学生照片和归组确认成功均清 status、loaded、run、results、failures、processing IDs，但保留 `templateBusy`；这是现有不完整 reset，本批必须保持。
2. 模板状态 effect 只在答题卡、质量复核完成且有参考页时运行；先把 loaded 设为 false。成功写 status 与 loaded=true，若模板集 ready 则触发逐页处理；失败只写 loaded=true 并提示错误。
3. effect cleanup 只用本地 `cancelled` 阻止晚到状态回写，不取消后端请求，也不新增跨 scope request token。
4. 模板分析只有在老师选择文件后才进入 busy；开始时清旧 run。API 正常返回的 failed run 仍保存，API 抛错时 run 保持 null；finally 释放 busy。
5. 模板确认先调用确认 API，再读取参考页模板状态；两次都成功后才写新 status 并清 run。模板集 ready 时随后处理页面；任一步抛错均保留旧 status/run，finally 释放 busy。
6. 普通逐页处理只选择质量通过且老师确认归属的页面；首次处理选择既无 result 也无 failure 的页，主动重试只选择有 failure 的页。
7. 处理开始把本轮 page ID 去重追加到 processing；页面始终串行处理。
8. 单页成功写 result，并只删除该页旧 failure；单页失败写 failure 但不删除既有 result。每页 finally 只移除自己的 processing ID。
9. 单页失败不阻断后续页面；循环结束统一汇总错误，不产生成功 toast，也不让 reducer 生成文案。
10. 老师仍须在面板中上传并确认模板；本批不自动确认模板、不改变批改与发布边界。

## 纯状态合同

扩展 `FixedIntakeBatchWorkflowState`，新增：

```text
answerSheet
  templateStatus
  templateStatusLoaded
  templateRun
  templateBusy
  pageResults
  pageFailures
  processingPageIds
```

必须覆盖以下语义 action：

| 事件 | 状态结果 |
|---|---|
| `ANSWER_SHEET_RESET` | 清 status/loaded/run/results/failures/processing，保留 busy；已满足时 no-op |
| `ANSWER_SHEET_STATUS_LOAD_STARTED` | loaded=false，保留旧 status；已为 false 时 no-op |
| `ANSWER_SHEET_STATUS_LOAD_SUCCEEDED` | 写 status 且 loaded=true |
| `ANSWER_SHEET_STATUS_LOAD_FAILED` | loaded=true，保留旧 status；已为 true 时 no-op |
| `ANSWER_SHEET_TEMPLATE_ANALYSIS_STARTED` | busy=true 且清旧 run |
| `ANSWER_SHEET_TEMPLATE_ANALYSIS_SUCCEEDED` | 写返回 run，包括 failed run |
| `ANSWER_SHEET_TEMPLATE_CONFIRMATION_STARTED` | busy=true，保留 run |
| `ANSWER_SHEET_TEMPLATE_CONFIRMATION_SUCCEEDED` | 写刷新后的 status 并清 run |
| `ANSWER_SHEET_TEMPLATE_OPERATION_FINISHED` | busy=false；已为 false 时 no-op |
| `ANSWER_SHEET_PAGES_STARTED` | 去重追加本轮 page IDs；空集合或无新增时 no-op |
| `ANSWER_SHEET_PAGE_SUCCEEDED` | 写指定 page result，并只删除该页 failure |
| `ANSWER_SHEET_PAGE_FAILED` | 写指定 page failure，保留所有 result |
| `ANSWER_SHEET_PAGE_FINISHED` | 只移除指定 page ID；不存在时 no-op |

数组和映射更新必须不可变；两个初始状态对象不得共享映射或数组引用；no-op 必须返回原 state 引用。reducer 不处理错误文案、请求触发、模板 ready 派生或逐页筛选。

## 唯一允许修改

1. 本合同。
2. `src/pages/exam/fixedIntakeState.ts`：增加 answerSheet slice、action 和纯 reducer 分支。
3. `tests/frontend/fixedIntakeState.test.ts`：先增加答题卡状态直接测试，再实现模型。
4. `src/pages/exam/FixedIntakeTab.tsx`：删除七个 answer-sheet `useState`，从现有 reducer 解构同名投影并把 setter 机械替换为 dispatch。
5. 完成后的验收总览、重构标准、文档索引、任务地图和修改记录。

不得修改 `FixedIntakeAnswerSheetProgressPanel`、共享壳脚本、普通卷题库同步脚本、API、Rust/SQL/DTO、样式、依赖、默写/context/draft state 或其他生产组件。

## 生产接线映射

- `resetRequest()`、选择新学生文件和归组确认成功均 dispatch `ANSWER_SHEET_RESET`。
- 模板状态 effect 的开始、成功、失败分别 dispatch status load started/succeeded/failed；effect dependency、取消标记、ready 后处理调用和错误提示原样保留。
- 模板分析在文件选择成功后 dispatch analysis started；API 返回后 dispatch analysis succeeded；finally dispatch operation finished。
- 模板确认开始 dispatch confirmation started；确认与刷新均成功后 dispatch confirmation succeeded；ready 后处理调用和 finally 语义保持原样。
- `processAnswerSheetPages` 的批次开始、单页成功、单页失败和单页 finally 分别 dispatch pages started/page succeeded/page failed/page finished。
- 页面筛选、循环顺序、错误数组、`onError`、API 调用和 key 生成原样保留。
- 面板继续接收派生计数与现有语义 handler；不改 props、文案或按钮。

## 改前基线与保护

- 分支/HEAD：`codex/t2-artifacts@6072360049b09c3155726773039da139834294fe`。
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`；staged=0。
- 展开 porcelain：103 条。
- `FixedIntakeTab.tsx`：1,076 行，SHA-256 `a1a8adc0...f87f`。
- `fixedIntakeState.ts`：617 行，SHA-256 `41f0e6f3...2527`。
- `fixedIntakeState.test.ts`：936 行，SHA-256 `cefb0110...e78`。
- 答题卡面板：SHA-256 `8b294bbb...bd5`。
- 共享壳脚本：SHA-256 `dcff9b4d...e4a7`；普通卷题库同步脚本：`4006d90b...2aa`。
- `src/api/exam.ts` / `package.json` / `tsconfig.json`：`1a796762...164` / `a3f0afa7...69b0` / `e09b1d73...f906`。
- 23 个业务命令清单 SHA-256：`375a66bfade8b24a2d2ffb180bb934494754e4d5cff25401bd04b8e7e4358ba4`。
- baseline：状态直接测试 23/23、共享壳 PASS。

## 必跑验收

1. 测试先行：新增测试在 answerSheet slice/action 尚不存在时稳定失败，再实现转绿。
2. `node --test tests/frontend/fixedIntakeState.test.ts` 与 `node --test tests/frontend/examPure.test.ts`。
3. `npx tsc --noEmit`、`npm run build`。
4. 十三个 UI 脚本全部 `py_compile` 并逐组运行；共享壳普通卷二次批次、答题卡模板门禁和普通卷题库旁路必须 PASS。
5. 23 个业务命令名称清单与基线逐字一致；权威写入黑名单仍为 0。
6. 静态审计证明七个 answer-sheet `useState` 和对应 raw setter 为 0，workflow reducer 生产消费者仍恰为 1。
7. 答题卡面板、共享壳与普通卷题库同步脚本、API/Rust/DTO、默写字段和依赖配置哈希不变。
8. `git diff --check`、目标空白扫描、完整 porcelain、HEAD、Git index 和 staged 保护。

## 停止条件

- 需要修改答题卡 API 参数/顺序、幂等键、提示、按钮、老师模板确认步骤或逐页串行语义；
- reset 清除了 busy，模板确认未刷新状态就清 run，或单页失败改成全批 fail-fast；
- 成功未删除同页旧 failure，失败删除了既有 result，或 retry/首次处理筛选发生变化；
- 答题卡面板、默写、context/draft、request function/controller 被顺带迁移；
- 新增依赖、第二状态源或任一直接/浏览器/保护测试失败；
- 暂存区、HEAD、Git index 或非目标文件漂移。

## 完成口径

全部验收通过后，只能记为“R3-S5 答题卡七字段 UI 投影 reducer 已受测并机械接入生产，现有 reset、模板确认、串行请求和逐页部分成功语义不变”。不得写成答题卡业务已重写、生命周期安全、controller、真实 OCR/OMR、老师减负、独立 `.app`、集成或发布已经完成。
