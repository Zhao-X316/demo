---
title: jiaofu-suite R3-S4 ordinary state reducer and production connection contract
date: 2026-08-02
status: authorized_local_behavior_preserving_refactor
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_fixed_intake_sha256: d70c3aff9ddd85757eacfe8fe0af9db3cc87e2e55ff9fff7643c2c3d7f7ada28
source_state_sha256: 7df5c79911cc6b6edae2caade3ab9c4a40b06bae81943f6764ba87d5e1715110
source_state_test_sha256: bef2fd0a0ee8b79289b5eb54a866acdf2eb2ee027ba288fcdb5829ae52fc1dbd
source_shared_shell_sha256: dcff9b4dc01f5a158129a40992f89fa972703f4abbe1d9e54b105a2c0d00e4a7
source_ordinary_sync_sha256: 4006d90bee68240f781b55606082b14ad507365a48a2155c495f554ee5fab2aa
scope: R3-S4-ordinary-state-production-connection
behavior_change_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-S4 普通卷状态 reducer 与生产接线合同

## 目标

把 `FixedIntakeTab` 中普通卷逐页分析和老师结构确认的四个瞬时 UI 投影并入现有 workflow reducer，并在直接测试通过后机械接入生产。普通卷分析、结构确认、题库旁路、逐题区识别的请求函数继续原地；命令参数、幂等键、相对顺序、逐页部分成功、失败提示、按钮和老师确认步骤保持不变。

本批不迁移答题卡、默写、context/draft，也不实施 R3-H1。当前 reset 差异、同 tick 防双击、StrictMode 和 stale completion 必须继续保留为后续行为变更边界。

## 改前状态域

| 字段 | 初始值 | 含义 |
|---|---|---|
| `ordinaryPaperRuns` | `{}` | `pageId → OrdinaryPaperRunResult` 的机器分析投影 |
| `analyzingPageIds` | `[]` | 当前逐页分析占用 |
| `ordinaryConfirmations` | `{}` | `pageId → OrdinaryStructureConfirmationResult` 的老师结构确认投影 |
| `confirmingOrdinaryPageIds` | `[]` | 当前确认/题库旁路/题区识别占用 |

四字段均为当前 WebView 投影，不替代后端 AI run、结构 confirmation、题库或题区事实。

## 当前行为矩阵

1. `resetRequest()` 只清 `runs/analyzingPageIds`，保留 `confirmations/confirmingPageIds`；这是已知不完整 reset，本批必须保持。
2. 选择新学生照片会清四字段；归组确认成功也会清四字段。
3. 普通卷分析只选择质量通过、老师确认归属且当前没有 run 的页面；主动重试只选择当前 run 为 `failed` 的页面。
4. 分析开始把本轮所有 page ID 去重追加到 `analyzingPageIds`；每页成功或失败后只移除该页。
5. API 正常返回的 `failed` run 仍写入 `runs`；API 抛错时不写 run，只收集错误提示，其余页继续。
6. 首次分析 key 固定为 `ordinary-paper:<pageId>:structure:v1`；主动重试才使用新 UUID。
7. 批量结构确认只选择 `succeeded + output.ready + 尚无 confirmation` 的 run；开始时以本轮全部 ready page ID 替换确认占用列表。
8. 每页结构确认成功后立即写 confirmation；随后题库同步失败只追加错误，不撤销 confirmation，也不阻断该页题区识别。
9. 一个题区识别失败只记失败，其他题区和其他页继续；每页 finally 只移除自己的确认占用。
10. 页面分析、结构确认、题库同步和题区识别的错误/成功提示仍留在请求函数，reducer 不生成文案。

## 纯状态合同

扩展 `FixedIntakeBatchWorkflowState`，新增：

```text
ordinary
  runs
  analyzingPageIds
  confirmations
  confirmingPageIds
```

必须覆盖以下语义 action：

| 事件 | 状态结果 |
|---|---|
| `ORDINARY_PARTIAL_RESET` | 只清 runs/analyzing，保留 confirmations/confirming |
| `ORDINARY_FULL_RESET` | 清四字段 |
| `ORDINARY_ANALYSIS_STARTED` | 去重追加本轮 page IDs；空集合 no-op |
| `ORDINARY_ANALYSIS_SUCCEEDED` | 只写指定 page run |
| `ORDINARY_ANALYSIS_FINISHED` | 只移除指定 page ID；不存在时 no-op |
| `ORDINARY_CONFIRMATION_STARTED` | 以本轮 page IDs 替换 confirmingPageIds |
| `ORDINARY_CONFIRMATION_SUCCEEDED` | 只写指定 page confirmation |
| `ORDINARY_CONFIRMATION_FINISHED` | 只移除指定 page ID；不存在时 no-op |

数组和映射更新必须不可变；两个初始状态对象不得共享引用；no-op 必须返回原 state 引用。reducer 不处理错误文案、题库计数或题区计数。

## 唯一允许修改

1. 本合同。
2. `src/pages/exam/fixedIntakeState.ts`：增加 ordinary slice、action 和纯 reducer 分支。
3. `tests/frontend/fixedIntakeState.test.ts`：先增加普通卷状态直接测试，再实现模型。
4. `src/pages/exam/FixedIntakeTab.tsx`：删除四个 ordinary `useState`，从现有 reducer 解构同名投影并把 setter 机械替换为 dispatch。
5. 完成后的验收总览、重构标准、文档索引、任务地图和修改记录。

不得修改 `FixedIntakeOrdinaryProgressPanel`、共享壳脚本、普通卷题库同步脚本、API、Rust/SQL/DTO、样式、依赖、答题卡/默写/context/draft state 或其他生产组件。

## 生产接线映射

- `resetRequest()` dispatch `ORDINARY_PARTIAL_RESET`；选择新学生文件和归组确认成功 dispatch `ORDINARY_FULL_RESET`。
- `analyzeOrdinaryPages` 的开始、单页成功和单页 finally 分别 dispatch analysis started/succeeded/finished。
- `confirmReadyOrdinaryPages` 的开始、单页结构确认成功和单页 finally 分别 dispatch confirmation started/succeeded/finished。
- 页面筛选、循环顺序、错误数组、题库计数、题区计数、`onError/onDone`、API 调用和 key 生成原样保留。
- 面板继续接收派生计数与现有语义 handler；不改 props、文案或按钮。

## 改前基线与保护

- 分支/HEAD：`codex/t2-artifacts@6072360049b09c3155726773039da139834294fe`。
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`；staged=0。
- 展开 porcelain：102 条。
- `FixedIntakeTab.tsx`：1,072 行，SHA-256 `d70c3aff...ada28`。
- `fixedIntakeState.ts`：486 行，SHA-256 `7df5c799...5110`。
- `fixedIntakeState.test.ts`：759 行，SHA-256 `bef2fd0a...1dbd`。
- 普通卷面板：SHA-256 `e22d400a...e1e9`。
- 共享壳脚本：SHA-256 `dcff9b4d...e4a7`；普通卷题库同步脚本：`4006d90b...b2aa`。
- `package.json` / `tsconfig.json`：`a3f0afa7...69b0` / `e09b1d73...f906`。
- 23 个业务命令清单 SHA-256：`375a66bfade8b24a2d2ffb180bb934494754e4d5cff25401bd04b8e7e4358ba4`。
- baseline：状态直接测试 19/19、共享壳 PASS、普通卷题库旁路 PASS。

## 必跑验收

1. 测试先行：新增测试在 ordinary slice/action 尚不存在时稳定失败，再实现转绿。
2. `node --test tests/frontend/fixedIntakeState.test.ts` 与 `node --test tests/frontend/examPure.test.ts`。
3. `npx tsc --noEmit`、`npm run build`。
4. 十三个 UI 脚本全部 `py_compile` 并逐组运行；共享壳普通卷二次批次与普通卷题库旁路必须 PASS。
5. 23 个业务命令名称清单与基线逐字一致；权威写入黑名单仍为 0。
6. 静态审计证明四个 ordinary `useState` 和对应 raw setter 为 0，workflow reducer 生产消费者仍恰为 1。
7. 普通卷面板、两份浏览器脚本、API/Rust/DTO、答题卡/默写字段和依赖配置哈希不变。
8. `git diff --check`、目标空白扫描、完整 porcelain、HEAD、Git index 和 staged 保护。

## 停止条件

- 需要修改普通卷 API 参数/顺序、幂等键、提示、按钮、老师确认步骤或题库旁路语义；
- partial reset 清除了确认结果，或 full reset 遗留旧确认；
- 单页错误改成全批 fail-fast，题库同步失败阻断题区识别，或确认失败撤销其他页；
- 答题卡、默写、context/draft、request function/controller 被顺带迁移；
- 新增依赖、第二状态源或任一直接/浏览器/保护测试失败；
- 暂存区、HEAD、Git index 或非目标文件漂移。

## 完成口径

全部验收通过后，只能记为“R3-S4 普通卷四字段 UI 投影 reducer 已受测并机械接入生产，现有 reset、请求顺序和逐页部分成功语义不变”。不得写成普通卷业务已重写、生命周期安全、controller、真实 OCR/OMR、老师减负、独立 `.app`、集成或发布已经完成。
