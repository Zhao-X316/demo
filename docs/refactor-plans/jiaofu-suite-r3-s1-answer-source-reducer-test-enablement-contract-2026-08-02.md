---
title: jiaofu-suite R3-S1 answer source reducer test enablement contract
date: 2026-08-02
status: authorized_local_pure_model
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_fixed_intake_sha256: 6f8192950782c9476e14a64ca0a3a6ed8e53d918140a889492b02378676a3d93
source_fixed_intake_test_sha256: 1e588651b004fc54210a545accae126f704a4f10dfd915453bcd8d45c8784003
scope: R3-S1-answer-source-reducer-pure-model
production_connection_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-S1 答案资料状态纯模型执行合同

## 目标

在不接入生产页面的前提下，为 `FixedIntakeTab` 当前四个答案资料状态建立单一纯状态模型、事件 reducer、映射初始化与就绪 selector，并用直接 TypeScript 测试冻结现有转换语义。R3-S1 只证明模型可用，不替换 `useState`、setter、API 请求或老师终审路径。

## 改前事实

1. `FixedIntakeTab.tsx` 仍为 1,121 行，并独立持有 `answerSourceAnalysis`、`answerSourceBusy`、`answerSourceError`、`rubricPointMappings` 四个 state。
2. 答案分析开始会置 busy、清空旧错误；成功保存分析并根据 review 初始化评分点映射；失败只写错误，保留旧分析和映射；finally 解除 busy。
3. 一致确认、沿用旧答案和采用新版本成功后，只替换 analysis 中的 review；失败由页面级 `onError` 报告，当前不写 `answerSourceError`。
4. 相同结构的简答评分点按 `orderIndex` 自动沿用旧 `stableId`；评分点增删或重排时保留空映射等待老师选择。
5. 当前前端 selector 会拦截空映射并允许多个 `__new_rubric_point__`，但因把 `Set.add()` 当成布尔值使用，不能在前端拦截“同一旧评分点重复沿用”；后端事务仍会拒绝并回滚重复沿用。R3-S1 只如实冻结这个现状，不在重构批中偷改按钮启用语义。
6. R3-E1 浏览器保护网已通过；生产 `FixedIntakeTab.tsx` SHA-256 为 `6f819295...a3d93`，共享壳脚本 SHA-256 为 `1e588651...4003`。

## 施工快照

- 分支：`codex/t2-artifacts`
- HEAD：`6072360049b09c3155726773039da139834294fe`
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`
- 展开 porcelain：96 条；staged：0
- `package.json` SHA-256：`a3f0afa7...69b0`
- `tsconfig.json` SHA-256：`e09b1d73...f906`
- `fixedIntakeState.ts` 与对应直接测试在改前均不存在

## 唯一允许修改

1. 本合同。
2. 新增 `src/pages/exam/fixedIntakeState.ts`：仅放 answerSource slice 的纯类型、初始状态工厂、事件 reducer、映射辅助与就绪 selector。
3. 新增 `tests/frontend/fixedIntakeState.test.ts`：仅直接验证上述纯模型。
4. 完成后的验收总览、重构标准、文档索引、任务地图和修改记录。

不得修改或导入本模型到 `FixedIntakeTab.tsx`、任何展示面板、API、Rust/SQL/DTO、样式、依赖或现有测试。本批新增的生产目录文件必须保持无运行时消费者。

## 状态与事件合同

### 状态

```text
answerSource
  analysis: AnswerSourceAnalysisResult | null
  busy: boolean
  error: string
  rubricPointMappings: Record<string, string>
```

每次创建初始状态都必须得到独立映射对象，避免测试或未来 reducer 实例之间共享可变引用。

### 事件

| 事件 | 必须结果 |
|---|---|
| `ANSWER_SOURCE_RESET` | 返回全新空状态，不复用旧映射对象 |
| `ANSWER_SOURCE_STARTED` | `busy=true`、清空旧错误，保留旧分析和映射作为失败重试证据 |
| `ANSWER_SOURCE_SUCCEEDED` | 保存完整 analysis，按 review 初始化映射；busy 仍由 FINISHED 解除 |
| `ANSWER_SOURCE_FAILED` | 写入安全错误文本，保留旧分析和映射；busy 仍由 FINISHED 解除 |
| `ANSWER_SOURCE_FINISHED` | 仅置 `busy=false` |
| `ANSWER_SOURCE_RESOLUTION_SUCCEEDED` | 有 analysis 时仅替换 review；无 analysis 时 fail-closed 原样返回 |
| `RUBRIC_MAPPING_CHANGED` | 只修改指定 `assessmentItemId:orderIndex`，不得原地修改旧 state |

reason code、通知、API 参数、幂等键和 resolution 失败提示仍由现有页面持有，不进入本纯 slice。

## 必须直接验证

1. 初始状态和值对象隔离。
2. STARTED 清错、占用并保留重试证据。
3. SUCCEEDED 保存分析，相同结构按顺序预填旧评分点。
4. 评分点结构变化时使用空映射，未完成为 not ready，多个新增点评分点允许通过；同时明确冻结当前前端重复沿用仍返回 ready、由后端拒绝的已知边界。
5. FAILED 与 FINISHED 的错误、证据和 busy 语义。
6. RESOLUTION 成功只更新 review；缺少 analysis 时不制造虚假状态。
7. 单点评分映射变更不修改其他键，也不原地修改旧对象。
8. RESET 清空全部四字段并生成新的映射对象。

## 必跑验收

1. `node --test tests/frontend/fixedIntakeState.test.ts`。
2. `node --test tests/frontend/examPure.test.ts`。
3. `npx tsc --noEmit`。
4. `npm run build`。
5. 十三个 R2/R3 UI 脚本全部 `py_compile` 并逐组运行。
6. `git diff --check`。
7. `rg` 证明 `src/pages/exam/fixedIntakeState.ts` 没有生产消费者。
8. `FixedIntakeTab.tsx`、共享壳脚本、`package.json`、`tsconfig.json`、HEAD、Git index SHA-256 和 staged 保持不变；展开 porcelain 只允许因本合同、纯模型和直接测试新增 3 条。

## 停止条件

- 直接测试必须修改现有生产页面才能通过；
- reducer 引入 React、Tauri、API 命令、随机 UUID、时间或提示副作用；
- 新模型与当前页面形成双写或被任何生产模块提前导入；
- 借本批修改 reset、reason code、错误提示、按钮、请求顺序或老师门禁；
- 借纯模型重构提前修复前端重复评分点映射校验，造成与 R3-E1 基线不同的按钮启用行为；
- 非目标生产、既有测试、暂存区或 HEAD 漂移。

## 完成口径

全部验收通过后，只能记为“R3-S1 答案资料状态纯模型与直接测试通过，尚未接入生产；R3-S2 可以另立合同迁移该域四个 state”。不得写成生产 reducer/controller 已接线、生命周期风险已修复、完整 R3 已完成、真实 OCR/答案识别已验证、老师已减负或已集成发布。
