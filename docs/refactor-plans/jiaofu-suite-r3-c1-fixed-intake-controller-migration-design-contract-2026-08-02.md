---
title: jiaofu-suite R3-C1 fixed intake controller migration design contract
date: 2026-08-02
status: design_approved_for_test_enablement_only
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_index_sha256: b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b
source_porcelain_count: 131
source_fixed_intake_sha256: 2de6b5e82f588284a126fbf84aa81c2f398d6c2689558d41642c107d668f224a
source_fixed_intake_lines: 1644
source_lifecycle_sha256: f695551aebabdca8b13a4289e538ea6d76762072f90ed92c6179a76dadec45a2
source_root_state_sha256: b905b0d11747b7f2f7dd7e129e271fd7ae35a0d5b23bcd5d6951f45798735e1b
source_domain_state_sha256: 5fdb7b5761026f3dd8e0bf5b82b8637663a78719203478c62226de1add87a602
source_direct_test_sha256: 9942359cc0ac3df950b7297f44f368c312f32c80480c505ae150f5194e20a7da
source_lifecycle_audit_sha256: 8454eb2159567d719e5560e5a5744d23fa4a47e4b5174653bc52b2e9111c7a22
source_business_command_count: 23
source_provider_callsite_count: 26
source_business_command_sha256: 375a66bfade8b24a2d2ffb180bb934494754e4d5cff25401bd04b8e7e4358ba4
scope: R3-C1-fixed-intake-controller-migration-design
production_change_authorized: false
test_enablement_authorized: true
controller_migration_authorized: false
c2_semantic_action_change_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-C1 固定上传 controller 迁移设计合同

## 1. 设计结论

H1-V2 已证明固定上传生命周期门禁完成本地自动化收口，但 `FixedIntakeTab.tsx` 仍有 1,644 行。当前大文件不是由 JSX 展示造成，而是以下职责仍集中在同一组件：

| 当前区段 | 主要职责 | 约行数 |
|---|---|---:|
| 79～265 | reducer、ref、operation registry、scope mutation、options 校准 | 187 |
| 266～590 | 文件选择、prepare、答案资料分析与三类处置 | 325 |
| 591～892 | 归组、三个 read effect、质量、重拍、材料类型 | 302 |
| 893～1048 | 普通卷分析、确认、题库旁路、题区识别 | 156 |
| 1050～1237 | 答题卡模板与学生页处理 | 188 |
| 1239～1431 | 默写模板与学生页处理 | 193 |
| 1434～1644 | 展示派生值与 JSX 组合 | 211 |

因此 C1 不允许把前 1,350 行整体复制到一个新的 1,000 行 hook 后宣称重构完成。目标是保持一个根 controller，同时把运行时、领域命令和纯派生拆成有明确依赖方向的小模块。

## 2. 目标结构

```text
FixedIntakeTab.tsx
  └── useFixedIntakeController.ts
        ├── fixedIntakeControllerRuntime.ts
        ├── fixedIntakeUploadCommands.ts
        ├── fixedIntakeAnswerSourceCommands.ts
        ├── fixedIntakeGroupingQualityCommands.ts
        ├── fixedIntakeOrdinaryCommands.ts
        ├── fixedIntakeAnswerSheetCommands.ts
        ├── fixedIntakeDictationCommands.ts
        └── fixedIntakeViewModel.ts
```

### 2.1 `FixedIntakeTab.tsx`

最终只允许：

- 调用 `useFixedIntakeController`；
- 渲染空态、上传表单、七个现有展示面板和布局壳；
- 把 controller 的 `view` 与 `actions` 传给面板。

最终禁止：

- 直接导入或调用任何 `exam*` provider；
- 直接调用 Tauri `open`、`crypto.randomUUID()`；
- 持有 `useReducer`、`useRef` 或业务 `useEffect`；
- 直接 dispatch `FixedIntakeAction`；
- 重新计算跨面板统计或材料 scope。

### 2.2 `useFixedIntakeController.ts`

只负责组合：

- 一个根 reducer/runtime；
- options 校准；
- 六组领域命令；
- 三个只读 effect；
- 纯 view model；
- 对外稳定的 `view/actions`。

它不得内联全部领域请求实现，不得出现 JSX，不得成为第二事实源。目标不超过 350 行；超过时必须说明新增职责，不能用格式压缩规避。

### 2.3 领域命令模块

领域命令模块是普通 TypeScript factory，不是互相嵌套的 React hook。每个模块只接收明确 runtime、当前投影和必要 continuation callback，返回当前同签名 action：

| 模块 | 操作族 |
|---|---|
| `fixedIntakeUploadCommands.ts` | 页周期、prepare、答案文件选择 |
| `fixedIntakeAnswerSourceCommands.ts` | 答案分析、确认一致、沿用当前、另存新版本、rubric 映射 |
| `fixedIntakeGroupingQualityCommands.ts` | 材料确认、归组、质量、重拍、归组证据读取 |
| `fixedIntakeOrdinaryCommands.ts` | 普通卷分析、结构确认、题库旁路、题区识别 |
| `fixedIntakeAnswerSheetCommands.ts` | 模板读取/分析/确认、学生页处理 |
| `fixedIntakeDictationCommands.ts` | 模板读取/分析/确认、学生页处理 |

单个命令模块目标不超过 400 行；不得把普通卷、答题卡和默写合并成一个参数化“万能材料处理器”。三类材料的 provider、幂等键、失败文案、部分成功和老师确认顺序继续独立。

### 2.4 运行时与 view model

`fixedIntakeControllerRuntime.ts` 只持有根 state/ref、mounted、operation registry、同步 claim、scope mutation、guarded dispatch 等共享基础，不含领域 provider。

`fixedIntakeViewModel.ts` 只能做纯派生：options、route label、归组候选、普通卷/答题卡/默写计数。它不得读取 ref、调用 provider、dispatch 或生成随机值。

## 3. 不可变业务合同

C1 全程必须保持：

1. 23 个唯一业务命令、26 个 provider 调用点及规范 hash 不变；
2. H1 的 17 个 operation owner 键前缀、claim 时机、completion identity 和 stale continuation guard 不变；
3. API 参数、调用次数、串行/并行顺序和幂等键文本不变；
4. prepare 失败复用 request key，成功或输入失效才清除；
5. 普通卷确认后题库同步失败仍不得阻断题区识别；
6. 答题卡/默写按页保留成功与失败，模板未 ready/active 不处理学生页；
7. 原始 OCR、评分、老师终审、发布和 learning evidence 语义不变；
8. 老师确认按钮、错误文案、成功文案、空态和现有面板 props 行为不变；
9. 完整刷新仍 fail-closed 回上传起点，不引入 localStorage 或前端事实副本；
10. `Exam.tsx`、API、Rust、SQL、DTO、依赖和样式不改。

## 4. 依赖方向与循环规避

允许的依赖方向：

```text
Tab → root controller → command factories → runtime/root state/API
                    └→ pure view model → root state/API types
```

命令 factory 之间不得互相 import。跨域 continuation 由根 controller 按以下方向注入：

- upload `submit` 可接收 `analyzeAnswerSource` callback；
- grouping/quality 可接收 `analyzeOrdinaryPages`、`processAnswerSheetPages`、`processDictationPages`；
- 答题卡模板确认只调用本模块返回的学生页处理动作；
- 默写模板确认只调用本模块返回的学生页处理动作；
- read effect 由根 controller 发起并调用对应命令模块的 continuation。

禁止 command A import command B，禁止通过模块级可变变量解决顺序，禁止引入 React context 作为临时总线。

## 5. 对外接口冻结

根 hook 最终返回：

```ts
type FixedIntakeController = {
  hasOptions: boolean;
  view: FixedIntakeViewModel;
  actions: FixedIntakeActions;
};
```

C1 为行为保持批，`actions` 暂时保留现有调用名与签名，例如 `setClassId`、`setAssessmentVersionId`、`setExpectedPages`。把这些名字改成 `selectClass`、`changeExpectedPages` 等语义动作属于 C2，不得在 C1 混做。

现有八个展示面板的 props 在 C1 不改名、不改 required/optional、不改变回调签名。根 hook 的内部接口可以分批演进，但每批必须由类型检查和全量 UI 证明面板契约未漂移。

## 6. C1 分批施工路线

| 批次 | 唯一因素 | 生产代码 |
|---|---|---|
| C1-E1 | 新增 controller 边界审计与目标红测试 | 不改 |
| C1-S1 | 提取 runtime、options 校准与纯 view model | 不移动 provider 调用 |
| C1-S2 | 提取 upload/prepare 与答案资料命令 | 只迁对应 5 个操作族 |
| C1-S3 | 提取材料、归组、质量、重拍与证据 effect | 只迁对应 5 个操作族 |
| C1-S4 | 提取普通卷命令 | 只迁普通卷 2 个操作族 |
| C1-S5 | 提取答题卡模板/effect/学生页命令 | 只迁答题卡 3 个操作族 |
| C1-S6 | 提取默写模板/effect/学生页命令，收口根 hook 与薄 Tab | 只迁默写 3 个操作族 |
| C1-V1 | 只读边界、依赖图、哈希与全量回归终审 | 不改生产 |

每批必须另立施工合同，冻结改前哈希和精确调用点；不得把 S1～S6 合成一次大剪切。

## 7. C1-E1 测试前置要求

下一批只允许新增独立 controller 边界审计器和对应合同，不修改生产代码。审计器必须：

1. 从 AST 枚举 `FixedIntakeTab.tsx` 和全部 `fixedIntake*Controller/Commands` 文件；
2. 当前基线应明确失败，因为 26 个 provider 调用仍在 Tab；
3. 最终要求 Tab 中 `exam*`、Tauri `open`、`crypto.randomUUID`、`useReducer/useRef/useEffect` 和直接 dispatch 均为 0；
4. 最终要求跨 controller 文件仍精确为 23 个命令、26 个调用点，规范 hash 不变；
5. 检查命令模块之间没有互相 import，根 controller 是唯一组合入口；
6. 输出每项边界的精确文件与行号，非零退出不能被写成整体验收失败；
7. 不修改 H1 生命周期审计器，H1 17/17、26/26 必须继续单独运行并保持绿色。

## 8. 每批验收矩阵

每个 C1-S 批次至少执行：

- C1 边界审计：按迁移矩阵允许部分完成，最终 C1-V1 必须全绿；
- H1 生命周期 AST 审计：始终 `17/17`、`26/26`；
- 业务命令 23 个及规范 hash；
- 直接测试 63/63；
- `npx tsc --noEmit` 与 `npm run build`；
- `python3 -m py_compile scripts/test_*_ui.py`；
- 当前全部 27 组 UI 回归；
- `git diff --check`；
- HEAD、Git index、staged 与既有脏工作树保护。

若某批需要为模块可测性增加纯测试，必须先建立失败先行用例；不得修改已有断言以适配行为变化。

## 9. 完成标准

C1 只有同时满足以下条件才可关闭：

- `FixedIntakeTab.tsx` 只保留 hook 调用、空态和 JSX 组合；
- Tab 中 provider、Tauri dialog、随机 UUID、reducer/ref/effect、直接 dispatch 全部为 0；
- 根 hook、运行时、六组命令与 view model 职责符合第二节，依赖图无环；
- 没有单个新的巨型 hook/command 文件吞并原 1,350 行编排；
- 23 个命令、26 个调用点、17/17 生命周期门禁、参数/顺序/文案均不漂移；
- 63/63、TypeScript、Vite 和 27/27 组 UI 全部通过；
- C1-V1 独立只读终审通过。

完成后只允许另立 C2 语义动作合同。C1 不证明真实 provider、老师减负、刷新恢复、独立 `.app`、集成或发布。

## 10. 本批授权边界

本设计合同只授权下一批 C1-E1 测试前置。当前不得创建 controller 生产文件、移动 provider 调用、改面板 props、修改测试断言或开始 C2。不得暂存、提交、tag、合并、推送或发布。
