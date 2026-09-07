---
title: jiaofu-suite R3-C1-E1 controller boundary audit test enablement contract
date: 2026-08-02
status: authorized_test_enablement
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_index_sha256: b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b
source_porcelain_count: 132
source_fixed_intake_sha256: 2de6b5e82f588284a126fbf84aa81c2f398d6c2689558d41642c107d668f224a
source_fixed_intake_lines: 1644
source_lifecycle_audit_sha256: 8454eb2159567d719e5560e5a5744d23fa4a47e4b5174653bc52b2e9111c7a22
source_business_command_count: 23
source_provider_callsite_count: 26
source_business_command_sha256: 375a66bfade8b24a2d2ffb180bb934494754e4d5cff25401bd04b8e7e4358ba4
scope: R3-C1-E1-controller-boundary-audit-test-enablement
production_change_authorized: false
test_change_authorized: true
controller_migration_authorized: false
c2_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-C1-E1 Controller 边界审计测试前置合同

## 1. 唯一目标

本批只新增 `scripts/audit_exam_fixed_intake_controller_boundary.mjs`，把 C1 终态结构转成可重复 AST 门禁。当前生产基线预期非零退出，因为 26 个 provider、Tauri dialog、UUID、reducer/ref/effect 和直接 dispatch 仍在 `FixedIntakeTab.tsx`，且目标 controller 文件尚不存在。

该非零是 C1 迁移目标红，不是现有功能回归失败。H1 生命周期审计、直接测试、TypeScript、build 和全部 27 组 UI 必须继续为绿。

## 2. 扫描范围

审计器固定扫描：

- `src/pages/exam/FixedIntakeTab.tsx`；
- `src/pages/exam/useFixedIntakeController.ts`；
- `src/pages/exam/fixedIntakeControllerRuntime.ts`；
- `src/pages/exam/fixedIntakeUploadCommands.ts`；
- `src/pages/exam/fixedIntakeAnswerSourceCommands.ts`；
- `src/pages/exam/fixedIntakeGroupingQualityCommands.ts`；
- `src/pages/exam/fixedIntakeOrdinaryCommands.ts`；
- `src/pages/exam/fixedIntakeAnswerSheetCommands.ts`；
- `src/pages/exam/fixedIntakeDictationCommands.ts`；
- `src/pages/exam/fixedIntakeViewModel.ts`。

尚未创建的目标文件必须显示为 `missing`，不能静默跳过后仍判通过。

## 3. 永久库存门禁

无论 C1 迁移到哪个阶段，下列清单都必须立即满足，否则属于真实回归：

1. 扫描范围内唯一 `exam*` 业务命令精确为 23 个；
2. provider 调用点精确为 26 个；
3. 排序命令清单加末尾换行后的 SHA-256 精确为 `375a66bf...8ba4`；
4. 未登记的 provider 不得藏在扫描范围外的新 fixed-intake controller 文件中；
5. H1 原审计器继续独立输出 17/17、26/26。

C1 边界审计不得复制 H1 的 owner/guard 判定后取代 H1；两者并行，一个验证生命周期，一个验证结构所有权。

## 4. C1 终态门禁

最终全部满足才退出 0：

- `FixedIntakeTab.tsx` 不超过 350 行；
- Tab 中 provider、Tauri `open`、`crypto.randomUUID`、`useReducer`、`useRef`、业务 `useEffect`、直接 `dispatch*` 调用全部为 0；
- Tab 恰好调用一次 `useFixedIntakeController`；
- 十个目标文件全部存在；
- 根 hook 不超过 350 行，provider 调用为 0；
- runtime 不超过 300 行且 provider 调用为 0；
- view model 不超过 300 行且 provider/React hook/dispatch 为 0；
- 六个命令文件各不超过 400 行；
- provider 只存在于六个命令文件；
- Tauri dialog 只允许在 upload、answer-sheet、dictation 命令文件；
- 任一命令文件不得 import 另一个 `*Commands` 文件；
- 根 hook 必须成为 runtime、六组命令和 view model 的唯一组合入口。

## 5. 输出合同

审计器必须打印：

- 每个扫描文件的存在状态、行数、provider 数；
- 23 个命令、26 个调用点和规范 hash；
- Tab 残留 provider/dialog/UUID/hooks/dispatch/root-hook-call 数量；
- 目标文件存在数、行数阈值、命令模块互相依赖和 provider 所有权；
- `summary=checks X/Y`；
- 每个失败项的精确描述，必要时含文件和行号。

库存漂移与边界未完成都退出 1，但必须分别标记为 `inventory_error` 与 `boundary_gap`，避免把迁移目标红误认成命令丢失。

## 6. 唯一允许修改

1. 本合同；
2. 新增 `scripts/audit_exam_fixed_intake_controller_boundary.mjs`；
3. 仓库外 C1-E1 验收总览、重构标准、文档索引、任务地图和修改记录。

不得修改 `src/`、既有测试、H1 审计器、package、API、Rust/SQL/DTO、面板 props、文案或样式；不得暂存、提交、tag、合并、推送或发布。

## 7. 验收

必须取得：

1. `node --check scripts/audit_exam_fixed_intake_controller_boundary.mjs` 通过；
2. 新审计器当前非零，并精确报告 26 个 provider 仍在 Tab、目标文件缺失；
3. 新审计器库存仍为 23 个命令、26 个调用点、原 hash，无 `inventory_error`；
4. H1 审计器继续 17/17、26/26、退出 0；
5. 直接测试 63/63、TypeScript、Vite build、Python 编译、全部 27 组 UI 与 `git diff --check` 通过；
6. 生产/测试/H1 审计器哈希、HEAD/index/staged 不漂移；
7. 展开 porcelain 132→134，只新增本合同和新审计器。

本批通过口径：

> C1-E1 测试前置通过；结构目标按预期为红，现有功能和 H1 门禁仍为绿。下一批只放行 C1-S1 runtime、options 校准与纯 view model 提取。
