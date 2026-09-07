---
title: jiaofu-suite R3-W1-E1 workbench controller audit test enablement
date: 2026-08-02
status: authorized_test_enablement_only
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_index_sha256: b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b
source_porcelain_count: 157
scope: R3-W1-E1-workbench-controller-audit-test-enablement
production_change_authorized: false
test_change_authorized: false
new_audit_authorized: true
existing_audit_change_authorized: false
controller_creation_authorized: false
release_authorized: false
---

# R3-W1-E1 终审工作台 Controller AST 审计测试前置合同

## 目标

只新增 `scripts/audit_exam_review_workbench_controllers.mjs`，把 W1-D 冻结的三台 provider/state/effect/raw setter 库存和 controller 终态变成可执行门禁。当前生产应保持功能绿，但因 controller 尚不存在且 provider/hook/UUID/确认框仍在 Tab 而非零退出。

## 冻结库存

- 20 个唯一 provider 命令，22 个调用点；规范 SHA-256：`ef7b4508e81cb5983816a19e9e1b612a2505806628bce232d602afefb899772f`。
- Objective / Dictation / Subjective 调用点：5 / 6 / 11。
- 三个 Tab 均存在；目标 controller 当前 0/3。
- 三个 Tab 当前分别有 5/7/9 个 `useState` 和各 2 个 `useEffect` 调用。
- `SubjectiveComponentEditor` 当前有 4 个 setter props。

审计器必须把 provider 库存漂移与结构目标红分开：库存错误无论阶段都阻断；controller 缺失、Tab 未变薄和语义动作未接通在 E1 是预期目标红。

## 目标门禁

1. 三个 Tab 与三个 controller 目标文件存在；
2. provider 命令、总调用点和逐工作台调用点精确匹配；
3. 三个 Tab 的 provider、`useState/useEffect`、UUID 和 `window.confirm` 为 0；
4. 每个 Tab 恰好调用一次自己的 controller；
5. provider 调用由对应 controller 独占，三 controller 不互相 import；
6. 三 controller 的公共 actions 不暴露 raw state setter；
7. Tab 与 `SubjectiveComponentEditor` 不接收/调用 raw setter，并出现对应语义动作；
8. `Exam.tsx` 仍只向三 Tab 传原 workbench/onDone/onError，不依赖 controller。

## 保护与验收

- 不改生产、现有 UI、C2/C1/H1 审计器、API 或依赖；
- C2 10/10、C1 19/19、H1 17/17/26/26、65/65、build 和 27/27 UI 继续全绿；
- 扩展 porcelain 只能 `157→159`，只新增本合同和新审计器；
- HEAD/index/staged 和 W1-D 冻结哈希不变。

通过 E1 只允许另立 W1-S1 Objective controller 合同，不证明 W1、R3、真实 provider、老师减负、集成或发布完成。
