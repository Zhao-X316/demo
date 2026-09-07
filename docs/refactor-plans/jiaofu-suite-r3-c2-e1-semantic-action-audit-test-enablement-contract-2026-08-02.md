---
title: jiaofu-suite R3-C2-E1 semantic action audit test enablement contract
date: 2026-08-02
status: authorized_test_enablement_only
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_index_sha256: b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b
source_porcelain_count: 152
source_fixed_intake_sha256: 551aa65cc4157b86778e51a7edd5c0434ffcbc12a005e422e25bfa3df21b22be
source_upload_form_sha256: 8a6c070d490556c81f51ad36bc3e0ef7038cc1d675785a3b60d3a447d24384b9
source_root_controller_sha256: 86855098e5869f7574b5f2518625a0d638aa4d858b8d9f43fb09dac3286c82d2
source_upload_commands_sha256: 92bca583a5b0cca48abda162bbe24524283dabe9be694e6faae0b27d5bd0e01c
source_controller_audit_sha256: b7f2d20cd08dfd5c3128da7af77235fdcbf0ecd14620368497b1fc5f5efba389
source_lifecycle_audit_sha256: 3e9d64964c3c5b1cc91c45b5d921c58534f8f4de892129c83ac830ad3c58b2e7
scope: R3-C2-E1-semantic-action-audit-test-enablement
production_change_authorized: false
test_change_authorized: false
new_audit_authorized: true
existing_audit_change_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-C2-E1 语义动作 AST 审计测试前置合同

## 1. 唯一目标

只新增 `scripts/audit_exam_fixed_intake_semantic_actions.mjs`，把 C2-D 冻结的 raw setter 库存和语义动作终态变成可执行门禁。当前生产应因 raw setter 仍存在而非零退出；这属于预期目标红，不是功能回归失败。

## 2. 必须报告

审计器扫描 `FixedIntakeTab.tsx`、`FixedIntakeUploadForm.tsx`、`useFixedIntakeController.ts` 和 `fixedIntakeUploadCommands.ts`，至少输出：

1. 四个目标文件是否存在；
2. 七个既有 reducer 事件 token 是否齐全；
3. 六个 raw setter 在各文件的出现位置；
4. upload form 语义 props 完整性；
5. Tab 语义 action 使用完整性；
6. root 公共 actions 是否暴露 raw setter、是否包含五个对外语义动作；
7. `selectStudentFiles/selectAnswerFile` 是否仅作为 root→upload factory 内部依赖且未进入公共 actions。

## 3. 预期红灯

当前应满足库存/事件检查，但因以下事实失败：

- upload form 与 Tab 仍有四类 raw setter；
- root 与 upload factory 仍有六类 raw setter 名；
- 五个对外语义动作尚未完整出现；
- 两个内部路径动作尚未改名且 raw 版本仍在公共 actions。

审计器若当前全绿，说明检测遗漏；若事件库存失败，说明出现非 C2 的行为漂移，应停止而不是改断言。

## 4. 保护与验收

- 不改生产、测试、C1/H1 审计器、API 或依赖；
- C1 仍为 19/19，H1 仍为 17/17/26/26；
- 65/65、TypeScript、build、Python、27/27 UI 与 whitespace 保持绿；
- 展开 porcelain 只能 `152→154`，只新增本合同和新审计器；
- HEAD、index、staged 和全部受保护哈希不变。

通过 E1 只允许另立 C2-S1 生产合同，不证明 C2 完成、集成或发布。
