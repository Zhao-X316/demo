---
title: jiaofu-suite R2-E8 SubjectiveComponentEditor test enablement contract
date: 2026-08-02
status: authorized_local_test_enablement
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_exam_sha256: cc1ef00fab75f5da0c4cfb01962e204d8ce12d07fc388e0338aebe1aa2b88f47
scope: R2-E8-subjective-component-editor-characterization
production_extraction_authorized: false
integration_authorized: false
release_authorized: false
---

# R2-E8 `SubjectiveComponentEditor` 测试前置执行合同

## 目标

在不修改生产代码的前提下，为 `src/pages/Exam.tsx` 当前老师“按空格/评分点逐项确认”编辑区建立浏览器特征保护。只有当前默认值、校验、状态隔离和提交 DTO 全部被真实浏览器测试冻结后，才能另开合同评估生产外移。

## 候选边界取证

1. 候选区只在当前建议尚未确认且存在填空槽位或简答评分点时显示。
2. 候选区读取 `row / componentSpecs / pointResults`，持有 `componentScores / componentEvidence / componentNotes / manualNotes` 受控输入，并通过父级 `correctComponents(row)` 提交。
3. 候选区本身不直接调用 Tauri；唯一业务提交仍由父级执行 `exam_answer_sheet_subjective_correct_components`。
4. OCR 校正、识别重试、生成/接受机器建议、整题人工记分、答案写法沉淀和 rubric 证据沉淀继续留在父级，本批不得顺带迁移。

## 唯一允许修改

1. 新建 `scripts/test_exam_subjective_component_editor_ui.py`。
2. 测试可复用既有主观题 workbench 浏览器 mock，并只拦截逐项确认命令；不得修改生产、API/DTO、样式或现有测试。
3. 完成后只回写对应验收、任务地图和修改记录，不借此扩大生产授权。

## 必须冻结的当前行为

1. 未确认且存在组件时显示编辑器；已确认或无组件时不显示。
2. 填空显示“按空格逐项确认”，简答显示“按评分点逐项确认”；简答或多组件默认展开，单空填空保持折叠。
3. 简答每个评分点默认得分和证据来自机器逐点评分，备注默认为空。
4. 单空填空默认得分来自整题建议，默认证据依次回退老师校正、规范化文本、原始 OCR。
5. 多空填空不得把整题建议和整题 OCR 猜测分配到各槽位，逐项得分与证据默认留空。
6. 受控输入按 `suggestion_id + source_public_id` 隔离，不得串到其他学生或组件。
7. 本题整体判定依据必填；缺失时不得调用业务命令，并显示当前错误提示。
8. 每项得分必须是 `0..max_score` 的有限数值；正分必须提供学生作答证据；失败时不得调用业务命令。
9. 合法提交只调用一次 `exam_answer_sheet_subjective_correct_components`，组件字段保持 snake_case，顶层参数保持当前 Tauri 包装约定；成功后显示汇总得分并刷新工作台。
10. 展开、编辑、校验失败和提交成功全过程不得触发其他主观题写命令。

## 禁止范围

- 不创建生产组件，不移动 `correctComponents`，不修改 `Exam.tsx`。
- 不修改 OCR、机器评分、接受建议、整题人工记分、答案/rubric 沉淀、链接、发布事务或 CSS。
- 不修改其他 Tab、Rust、Tauri command、SQL、迁移、依赖或现有测试。
- 不暂存、不提交、不 tag、不合并、不推送、不发布。

## 必跑验收

1. 新脚本 Python 语法检查。
2. `node --test tests/frontend/examPure.test.ts`。
3. `npm run build`。
4. KnowledgeTab、QuestionTab、SubjectiveLinkPanel、GradeTab、SubjectiveAttemptPublishPanel、SubjectiveReviewToolbar、SubjectiveEvidenceSummary 与新增逐项编辑器 UI 回归。
5. 生产 `Exam.tsx` 与 R2-E8 改前 SHA-256 保持不变。
6. 既有脏文件哈希、完整展开 porcelain、Git index、HEAD 与 whitespace 保护。

## 停止条件

- 需要修改生产实现或现有测试才能通过；
- 只能用真实 DTO 不可能产生的状态组合才能覆盖编辑器；
- 校验失败仍触发写调用，或合法提交触发逐项确认以外的写命令；
- 非目标既有文件、暂存区或 HEAD 变化。

## 完成口径

只有所有验收通过，才允许记为“R2-E8 老师逐项确认编辑器测试前置自动化通过，生产仍内联、未集成”。不得写成逐项编辑器已生产外移、主观题机器评分准确或老师真实减负已验证。
