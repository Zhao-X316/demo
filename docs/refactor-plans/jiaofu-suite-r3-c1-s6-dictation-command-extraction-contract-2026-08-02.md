---
title: jiaofu-suite R3-C1-S6 dictation command extraction contract
date: 2026-08-02
status: authorized_for_implementation
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_index_sha256: b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b
source_porcelain_count: 148
source_fixed_intake_sha256: 9dbfd1133e23034204dcc79cc55ca240b0a76d85088a5ba2337325b312f803f6
source_root_controller_sha256: 4024773f008e0c1ca803b3debe0f68d17d388a551b212f4df46ed8f7f9184235
source_answer_sheet_commands_sha256: 48c259ec480cdbe2c70dff87367648ac72ecfaf9561d4a8b58c0e0044a8bfdc3
source_lifecycle_audit_sha256: 27f8ec3ff586626b03ed74cd003e801108e5b0147b54653282c7411ac06502bf
source_controller_audit_sha256: b7f2d20cd08dfd5c3128da7af77235fdcbf0ecd14620368497b1fc5f5efba389
source_business_command_count: 23
source_provider_callsite_count: 26
source_business_command_sha256: 375a66bfade8b24a2d2ffb180bb934494754e4d5cff25401bd04b8e7e4358ba4
scope: R3-C1-S6-dictation-command-and-effect-extraction
production_change_authorized: true
lifecycle_audit_owner_registration_authorized: true
provider_move_authorized: four_calls_only
thin_tab_closeout_authorized: true
c2_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-C1-S6 默写模板、Effect 与学生页命令迁移合同

## 1. 唯一目标

本批只新增 `fixedIntakeDictationCommands.ts`，机械迁移：

- 默写模板状态读取 effect 的 provider/claim/guard runner；
- 选择空白默写页并分析模板；
- 老师确认模板并保留 legacy run/status 语义；
- 已确认高质量学生页的逐页处理、部分成功和失败保留。

精确移动最后 4 个 provider 调用点：模板状态读取 1、模板分析 1、模板确认 1、学生页处理 1。完成后 Tab 中 provider、Tauri dialog、UUID、React effect、runtime 直接 dispatch 均应归零。

## 2. Effect 与薄 Tab 收口

- factory 返回 `dictationTemplateScopeKey` 与 `startDictationTemplateStatusReadEffect`；根 hook 的 `useEffect` 只按 scope key 调用 runner。
- H1 owner 清单只新增 `startDictationTemplateStatusReadEffect → dictation_template_status`，其他判据不改。
- 状态读取存在 active template 后调用本模块 `processDictationPages`；模板确认成功后也调用同一动作。
- 删除 Tab 中最后的 `useEffect`、Tauri `open`、`GroupedPageEvidence`、lifecycle/root helper 与四个 provider import；Tab 只保留 controller 调用、空态、展示映射和 JSX/面板组合。
- 根 hook 继续只做模块组合，不内联默写请求；目标不超过 350 行。

## 3. 不可变业务合同

1. 23 个命令、26 个调用点和规范 hash 不变；
2. H1 17/17、26/26、模板 status/template/page key、选择文件前 claim、scope/page/run identity 与 stale guard 不变；
3. 空白页 dialog 类型、扩展名、幂等 key 文本与 UUID 时机不变；
4. 模板确认仍把 provider 返回模板映射为 active status，并保留既有 run；
5. 学生页筛选、retry、逐页独立 claim、成功/失败/finished、失败保留和错误文案不变；
6. API 参数/次数/顺序、reducer/runtime/view model、八个面板 props、Rust/SQL/DTO/样式不改；
7. 不以 C1 全绿冒充真实 OCR/provider、老师减负或发布；
8. C2 语义动作改名不混入。

## 4. 精确允许修改

仓库内仅：

1. 本合同；
2. 新增 `src/pages/exam/fixedIntakeDictationCommands.ts`；
3. 修改 `src/pages/exam/useFixedIntakeController.ts`；
4. 修改 `src/pages/exam/FixedIntakeTab.tsx`；
5. 修改 H1 审计器，只新增 dictation status runner owner 登记。

仓库外只允许写 S6 回执与权威索引/进度/修改记录。不得改测试、C1 审计器、已完成命令模块、package/依赖或其他生产文件；不得暂存、提交、tag、合并、推送或发布。

## 5. 验收

必须取得：

1. H1 跨七文件仍为 17/17、26/26、exit 0；
2. C1 边界审计 19/19、exit 0，库存 23/26/原 hash；
3. dictation 不超过 400 行、provider=4、无命令模块 import；
4. Tab 不超过 350 行，provider/dialog/UUID/runtime hook/direct dispatch 全为 0，且只调用一次根 controller；
5. 根 hook 不超过 350 行、provider=0，并组合六组命令/runtime/view；
6. 65/65、TypeScript、Vite、Python 编译、27/27 UI 与 whitespace 通过；
7. HEAD/index/staged 与 S5 受保护文件不漂移；
8. 展开 porcelain 148→150，只新增本合同与 dictation 文件。

本批通过后也只能进入 C1-V1 独立只读终审，不得直接开始 C2、集成或发布。
