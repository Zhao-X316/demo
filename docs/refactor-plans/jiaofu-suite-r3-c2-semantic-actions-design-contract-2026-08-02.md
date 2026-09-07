---
title: jiaofu-suite R3-C2 fixed intake semantic actions design contract
date: 2026-08-02
status: design_approved_for_test_enablement_only
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_index_sha256: b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b
source_porcelain_count: 151
source_fixed_intake_sha256: 551aa65cc4157b86778e51a7edd5c0434ffcbc12a005e422e25bfa3df21b22be
source_upload_form_sha256: 8a6c070d490556c81f51ad36bc3e0ef7038cc1d675785a3b60d3a447d24384b9
source_root_controller_sha256: 86855098e5869f7574b5f2518625a0d638aa4d858b8d9f43fb09dac3286c82d2
source_upload_commands_sha256: 92bca583a5b0cca48abda162bbe24524283dabe9be694e6faae0b27d5bd0e01c
source_controller_direct_test_sha256: 71a750d00a1f5c9a20524fbf03f02f90e7cadc098827cb19c3fc77869056b1fb
source_controller_audit_sha256: b7f2d20cd08dfd5c3128da7af77235fdcbf0ecd14620368497b1fc5f5efba389
source_lifecycle_audit_sha256: 3e9d64964c3c5b1cc91c45b5d921c58534f8f4de892129c83ac830ad3c58b2e7
source_exam_api_sha256: 1a796762c34c9079430423965937faead89a16c2618930e64603bd262c49c164
source_package_sha256: a3f0afa700aaeaa813f93dd6c56f6b9e8b3452e0be48e19fc2b8883af99d69b0
source_lock_sha256: 8f627a00b69b6e231659534859ea475e241192396491c5c8659d87cb39aaadf4
source_panel_raw_setter_prop_count: 4
source_raw_setter_symbol_count: 6
scope: R3-C2-fixed-intake-semantic-actions-design
production_change_authorized: false
test_enablement_authorized: true
semantic_action_migration_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-C2 固定上传语义动作设计合同

## 1. 设计结论

C1-V1 已证明 controller 分域和生命周期边界完成本地自动化收口。C2 不再重排请求或状态，只收窄“展示组件如何请求状态变化”的接口，使展示层表达老师意图而不是暴露状态字段写法。

只读库存显示，剩余 raw setter 只位于上传入口：

| 边界 | 当前 raw setter | 当前语义 |
|---|---|---|
| `FixedIntakeUploadForm` props | `setClassId` | 老师选择班级 |
| 同上 | `setAssessmentVersionId` | 老师选择作业版本 |
| 同上 | `setExpectedPages` | 老师修改每人页数草稿 |
| 同上 | `setAnswerText` | 老师修改粘贴答案草稿 |
| 根 controller 内部 | `setStudentPaths` | 文件选择器完成后接纳学生文件 |
| 根 controller 内部 | `setAnswerPath` | 文件选择器完成后接纳答案文件 |

其他固定上传面板已使用 `changeRubricPointMapping`、`changeGroupingStart`、`toggleAbsentStudent`、`toggleRejectedPage` 等语义动作，不为统一命名再次修改。

## 2. 终态接口

展示层和 controller 公共 `actions` 使用：

```text
selectClass
selectAssessment
changeExpectedPages
changeAnswerText
clearAnswerSource
pickStudentPapers
pickAnswer
```

文件选择器的已选路径只在 root controller 与 upload factory 之间使用：

```text
selectStudentFiles
selectAnswerFile
```

二者不得继续从公共 `actions` 暴露，因为展示表单只应发出“选择文件”命令，不能绕过 dialog、白名单、排序、页周期和生命周期门禁直接写路径。

## 3. 行为保持边界

C2-S1 只能机械改名和收窄导出，必须保持：

1. reducer 事件类型和 payload 不变；
2. class/assessment options 校准行为不变；
3. 文件 dialog、扩展名白名单、自然顺序、页周期推断和 completion identity 不变；
4. 答案文件与粘贴文本互斥、清除答案和 session 失效范围不变；
5. 23 个业务命令、26 个 provider 调用点、H1 owner/identity、参数、顺序、幂等键和文案不变；
6. 所有按钮、label、禁用条件和老师确认门禁不变；
7. API、Rust、SQL、DTO、样式和依赖不改。

语义动作不是新的业务事实。它们仍只 dispatch 既有 root reducer 事件，后端继续持有批次、识别、终审和发布权威。

## 4. AST 门禁

C2-E1 新增独立审计器，当前基线必须因上述 raw setter 而目标红，且同时证明库存可测。最终至少要求：

1. `FixedIntakeUploadForm` props、参数解构和 JSX 调用中 raw `set*` 为 0；
2. `FixedIntakeTab` 对 upload form 的 raw `set*` 传递为 0；
3. root controller 公共 actions 不暴露 `setClassId/setAssessmentVersionId/setStudentPaths/setAnswerPath/setAnswerText/setExpectedPages/clearAnswer`；
4. root controller 与 upload factory 不再出现这六个 raw setter 名；
5. 七个目标语义动作存在于正确所有者且无重复别名；
6. `setStudentPaths/setAnswerPath` 的替代动作只作内部依赖，不进入公共 actions；
7. C1 19/19、H1 17/17/26/26 与 23/26/规范 hash 继续独立为绿。

审计器不得把当前目标红写成功能失败，也不得修改既有 C1/H1 审计器。

## 5. 分批施工

| 批次 | 唯一因素 | 生产代码 |
|---|---|---|
| C2-D | 本设计合同与只读库存 | 不改 |
| C2-E1 | 新增语义动作 AST 审计器 | 不改 |
| C2-S1 | 上传表单、Tab、root controller 与 upload factory 机械改名并收窄公共 actions | 只改四个文件 |
| C2-V1 | 独立只读冻结哈希、边界与全量回归 | 不改 |

不需要按四个表单字段再拆四批；它们共用同一个 props/controller 接口且不含 provider 行为。不得把三个终审工作台、R4～R5 或产品功能混入 C2。

## 6. 验收矩阵

每个生产批至少运行：

- C2 语义动作审计；
- C1 controller 边界审计 `19/19`；
- H1 生命周期审计 `17/17`、`26/26`；
- 全部前端直接测试；
- TypeScript 与 Vite build；
- Python UI 脚本编译和当前 27 组 UI 回归；
- `git diff --check`；
- 目标哈希、HEAD、Git index、staged 和既有脏工作树保护。

## 7. 当前授权

本合同只授权下一批 C2-E1 新增独立 AST 审计器。当前不授权修改四个生产文件，不授权改测试断言、provider、API、依赖、集成或发布。不得暂存、提交、tag、合并、推送或发布。
