---
title: jiaofu-suite R3-C1-S5 answer sheet command extraction contract
date: 2026-08-02
status: authorized_for_implementation
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_index_sha256: b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b
source_porcelain_count: 146
source_fixed_intake_sha256: 0279e7d41741d022e481e2f16303d92382a74b9743dc332ebd50a8e43b956f1b
source_root_controller_sha256: 0433c49ff69f4f958e483a685e681e09e8f3660e44bcaf049bdf538b3acbf8ce
source_ordinary_commands_sha256: 764f8170d2bb888daf18d659cd1cf9c1d32533237ddd84ce9b444c88a3c0f17a
source_lifecycle_audit_sha256: fabb796b15de6bc60923a88e0fb7f9d87e3b4e94e7ce6565e49bb84802c66c9b
source_controller_audit_sha256: b7f2d20cd08dfd5c3128da7af77235fdcbf0ecd14620368497b1fc5f5efba389
source_business_command_count: 23
source_provider_callsite_count: 26
source_business_command_sha256: 375a66bfade8b24a2d2ffb180bb934494754e4d5cff25401bd04b8e7e4358ba4
scope: R3-C1-S5-answer-sheet-command-and-effect-extraction
production_change_authorized: true
lifecycle_audit_owner_registration_authorized: true
provider_move_authorized: five_calls_only
other_command_modules_authorized: false
c2_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-C1-S5 答题卡模板、Effect 与学生页命令迁移合同

## 1. 唯一目标

本批只新增 `fixedIntakeAnswerSheetCommands.ts`，机械迁移：

- 答题卡模板状态读取 effect 的 provider/claim/guard runner；
- 选择空白答题卡并分析模板；
- 老师确认模板、刷新模板状态并继续学生页处理；
- 已确认高质量学生页的逐页处理、部分成功和失败保留。

精确移动 5 个 provider 调用点：模板状态读取 1、模板分析 1、模板确认 1、确认后状态刷新 1、学生页处理 1。其余默写 4 个 provider、默写模板读取 effect 和命令均留在 Tab。

## 2. Effect 与模块内部 continuation

- factory 返回 `answerSheetTemplateScopeKey` 与 `startAnswerSheetTemplateStatusReadEffect`；根 hook 的 `useEffect` 只按 scope key 调用 runner。
- provider、`startFixedIntakeReadEffect`、claim 和 `isCurrent` 必须在同一命名 owner 中；H1 owner 清单只新增 `startAnswerSheetTemplateStatusReadEffect → answer_sheet_template_status`，其他判据不改。
- 状态读取 ready 后调用本模块 `processAnswerSheetPages`；模板确认刷新 ready 后也调用本模块同一动作，不经根 hook 或其他命令模块转发。
- answer-sheet 模块不得 import grouping-quality、ordinary、dictation 或 Tab。

## 3. 不可变业务合同

1. 23 个命令、26 个调用点和规范 hash 不变；
2. H1 17/17、26/26、模板 status/template/page key、选择文件前 claim、scope/page/run identity 与 stale guard 不变；
3. 空白卡 dialog 类型、扩展名、幂等 key 文本与 UUID 时机不变；
4. 模板确认仍先 confirm、再刷新 status、ready 后才处理学生页；
5. 学生页仍筛选质量通过且老师确认，retry 只取失败页，并为每页独立同步 claim；
6. 逐页成功/失败/finished、失败保留和错误文案不变；
7. API 参数/次数/顺序、reducer/runtime/view model、面板 props、Rust/SQL/DTO/样式不改；
8. C2 语义动作改名不混入。

## 4. 精确允许修改

仓库内仅：

1. 本合同；
2. 新增 `src/pages/exam/fixedIntakeAnswerSheetCommands.ts`；
3. 修改 `src/pages/exam/useFixedIntakeController.ts`；
4. 修改 `src/pages/exam/FixedIntakeTab.tsx`；
5. 修改 H1 审计器，只新增 answer-sheet status runner owner 登记。

仓库外只允许写 S5 回执与权威索引/进度/修改记录。不得改测试、C1 审计器、已完成命令模块、package/依赖或其他生产文件；不得暂存、提交、tag、合并、推送或发布。

## 5. 验收

必须取得：

1. H1 跨六文件仍为 17/17、26/26、exit 0；
2. C1 库存 23/26/原 hash 且无 inventory error；
3. answer-sheet 不超过 400 行、provider=5、无命令模块 import；
4. Tab provider 9→4、dialog 仍为 1、UUID 2→1、读取 effect 2→1，答题卡 handler 全部移除；
5. 根 hook provider=0，并组合 answer-sheet 和持有其读取 effect；
6. 65/65、TypeScript、Vite、Python 编译、27/27 UI 与 whitespace 通过；
7. HEAD/index/staged 与 S4 受保护文件不漂移；
8. 展开 porcelain 146→148，只新增本合同与 answer-sheet 文件。

本批不证明 C1 完成、真实 OMR/OCR/provider、真实答题卡模板、老师减负、独立 `.app`、集成或发布。下一批只能 C1-S6 迁默写模板/effect/学生页命令并收口薄 Tab。
