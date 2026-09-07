---
title: jiaofu-suite R3-S2 answer source production connection contract
date: 2026-08-02
status: authorized_local_mechanical_connection
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_fixed_intake_sha256: 6f8192950782c9476e14a64ca0a3a6ed8e53d918140a889492b02378676a3d93
source_answer_source_panel_sha256: 48b87d7c011b456d9b908e84030cc41fe54516bc28b8428fb8b30eae0fa13f1d
source_answer_source_state_sha256: 9af3d2557ff39f9762bb449bb428339a5f331a593d0acab5b6a7f27ef7449a90
source_answer_source_test_sha256: e6451a87f0a5faa3a23746fc69d691ccaaee3d25b440cbef1601e04340a65ae9
scope: R3-S2-answer-source-production-connection
behavior_change_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-S2 答案资料状态生产接线执行合同

## 目标

把 R3-S1 已受测的 answerSource 纯状态模型机械接入 `FixedIntakeTab`，以一个 reducer 替代页面内四个互相关联的 `useState`，并把答案评分点下拉框的原始 setter 改为语义事件。API、参数、幂等键、reason code、消息、调用顺序、老师终审门禁和可见行为必须保持不变。

## 改前事实

1. `FixedIntakeTab.tsx` 通过四个独立 state 持有 `answerSourceAnalysis`、`answerSourceBusy`、`answerSourceError`、`rubricPointMappings`，同时重复实现了 R3-S1 已提取的映射初始化和就绪判断。
2. 答案分析开始时会进入 busy 并清空旧错误；分析成功写完整 analysis 和映射；失败写 `answerSourceError`；finally 解除 busy。
3. 一致确认、沿用当前答案、采用新版本三种老师处理动作开始时只进入 busy，不会清空既有 `answerSourceError`；成功只替换现有 analysis 的 review；失败继续仅通过页面 `onError` 报告；finally 解除 busy。
4. `FixedIntakeAnswerSourcePanel` 当前接收 `setRubricPointMappings` 原始 React setter，仅用于一个下拉框的单点映射变化。
5. 当前前端重复旧评分点映射校验存在已知缺口；Rust 服务会拒绝并回滚。R3-S2 只做机械接线，继续冻结现状，另由 `R3-V1` 建立行为修正合同。
6. 改前共享壳浏览器特征测试通过；这只证明受控模拟场景中的现有 UI 行为，不证明真实 OCR、AI、老师减负或发布可用。

## 施工快照

- 分支：`codex/t2-artifacts`
- HEAD：`6072360049b09c3155726773039da139834294fe`
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`
- 展开 porcelain：99 条；staged：0
- `FixedIntakeTab.tsx` SHA-256：`6f819295...a3d93`
- `FixedIntakeAnswerSourcePanel.tsx` SHA-256：`48b87d7c...3f1d`
- `fixedIntakeState.ts` SHA-256：`9af3d255...9a90`
- `fixedIntakeState.test.ts` SHA-256：`e6451a87...ae9`
- 共享壳脚本 SHA-256：`1e588651...4003`
- `package.json` SHA-256：`a3f0afa7...69b0`
- `tsconfig.json` SHA-256：`e09b1d73...f906`

## 唯一允许修改

1. 本合同。
2. `src/pages/exam/fixedIntakeState.ts`：只增加受测的 `ANSWER_SOURCE_RESOLUTION_STARTED` 事件，以准确保持老师处理动作“只置 busy、不清错”的现状。
3. `tests/frontend/fixedIntakeState.test.ts`：增加上述事件的直接特征测试。
4. `src/pages/exam/FixedIntakeTab.tsx`：删除本地重复 helper 和四个 answerSource state，机械替换为 reducer state/dispatch。
5. `src/pages/exam/FixedIntakeAnswerSourcePanel.tsx`：只把评分点 raw setter 改为单点语义 callback。
6. 完成后的验收总览、重构标准、文档索引、任务地图和修改记录。

不得修改 API、Rust/SQL/DTO、样式、共享壳脚本、依赖、其他业务域 state 或其他生产面板。

## 接线映射

| 原页面动作 | reducer 事件 | 必须保持的语义 |
|---|---|---|
| 重置请求、重新选择学生照片 | `ANSWER_SOURCE_RESET` | 四字段一起回到全新初始状态 |
| 开始分析答案资料 | `ANSWER_SOURCE_STARTED` | busy=true、清空旧错误、保留旧分析和映射 |
| 分析成功 | `ANSWER_SOURCE_SUCCEEDED` | 保存完整 analysis 并初始化映射 |
| 分析异常 | `ANSWER_SOURCE_FAILED` | 写错误，保留旧证据 |
| 分析 finally | `ANSWER_SOURCE_FINISHED` | 仅解除 busy |
| 三种老师处理动作开始 | `ANSWER_SOURCE_RESOLUTION_STARTED` | 仅 busy=true，不清空旧错误或证据 |
| 三种老师处理成功 | `ANSWER_SOURCE_RESOLUTION_SUCCEEDED` | 仅替换现有 analysis.review |
| 三种老师处理 finally | `ANSWER_SOURCE_FINISHED` | 仅解除 busy |
| 评分点下拉变化 | `RUBRIC_MAPPING_CHANGED` | 只改指定题目和序号映射 |

页面继续用原有局部名称解构 reducer state，避免扩散 JSX 改动。面板 callback 必须传 `assessmentItemId`、`orderIndex` 和选择值，不得获得整个映射对象的写权限。

## 明确不改

1. 不修复 `Set.add()` 误作布尔值导致的前端重复映射校验缺口。
2. 不增加请求序列号、AbortController 或 stale response 抑制，因此不得宣称并发响应风险已解决。
3. 不改变答案资料分析与三种 resolution 的错误展示路径。
4. 不改变 `replaceAnswerSourceReason` 的条件、时机和 reason code。
5. 不改变 `crypto.randomUUID()`、结构版本键、API 参数或 API 调用顺序。
6. 不改变任何按钮可见性、禁用条件、文案、老师确认步骤或评分点版本语义。
7. 不迁移普通试卷、答题卡、默写、归组、质量确认等其他状态域。

## 必跑验收

1. `node --test tests/frontend/fixedIntakeState.test.ts`，预期 9/9。
2. `node --test tests/frontend/examPure.test.ts`，预期 6/6。
3. `npx tsc --noEmit`。
4. `npm run build`。
5. 十三个 R2/R3 UI 脚本全部 `py_compile` 并逐组运行。
6. `git diff --check`。
7. 静态审计证明页面不再持有四个 answerSource setter，`fixedIntakeState` 恰有一个生产消费者。
8. 改前/改后业务命令集合、共享壳脚本、`package.json`、`tsconfig.json`、HEAD、Git index 和 staged 保持不变。
9. 展开 porcelain 只允许由本合同新增 1 条；其余目标文件均已在当前用户工作树中存在。

## 停止条件

- reducer 接线要求修改 API、reason code、提示文案、老师门禁或请求顺序；
- resolution 开始事件清除了旧错误，或分析开始事件不再清错；
- 面板仍可直接替换整个 rubric 映射对象；
- 借接线修复重复映射校验、增加并发控制或迁移其他状态域；
- 任一直接测试、TypeScript、构建、浏览器特征或静态保护项失败；
- 非目标生产文件、既有测试、暂存区、HEAD 或 index 漂移。

## 完成口径

全部验收通过后，只能记为“R3-S2 answerSource reducer 已机械接入固定上传生产页面，受控特征与保护项通过”。不得写成完整 `FixedIntakeTab` 已重构、重复评分点校验已修复、并发响应安全已证明、真实 OCR/AI 已验证、老师减负已证明、已提交、已集成或已发布。
