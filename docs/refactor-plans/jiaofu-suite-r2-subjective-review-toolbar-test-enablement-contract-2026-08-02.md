---
title: jiaofu-suite R2-E6 SubjectiveReviewToolbar test enablement contract
date: 2026-08-02
status: authorized_local_test_enablement
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_exam_sha256: d8e8223f5ca48da3724babb219d605d3358a9db0138ff2919648751f0e6cfc3f
scope: R2-E6-subjective-review-toolbar-characterization
production_extraction_authorized: false
integration_authorized: false
release_authorized: false
---

# R2-E6 `SubjectiveReviewToolbar` 测试前置执行合同

## 目标

在不修改生产代码的前提下，为 `src/pages/Exam.tsx` 当前第 2294～2318 行的主观题工作台顶部工具栏建立浏览器特征保护，随后才能单独评估把这段纯展示/选择回调边界外移为 `SubjectiveReviewToolbar`。

## 候选边界取证

1. 候选块只展示作业版本、按题终审选择器、四项统计和固定说明。
2. 候选块不直接调用 Tauri，不持有 OCR、评分、人工终审、答案/rubric 沉淀、知识链接或整卷发布命令。
3. 父级继续负责作业/题目选项派生、有效选择回退、学生行筛选排序、统计计算和全部业务命令。

## 唯一允许修改

1. 新建 `scripts/test_exam_subjective_review_toolbar_ui.py`。
2. 测试只扩展浏览器 Tauri mock；不得改生产 `Exam.tsx`、现有测试、API/DTO 或样式。
3. 完成后只回写对应验收、任务地图和修改记录，不借此扩大生产授权。

## 必须冻结的当前行为

1. 默认选择第一项作业版本；题目选择器按 `order_index` 排序并自动选择该作业的第一题。
2. 题目选项显示题号、题型中文标签和题干摘要。
3. 四项统计只针对当前作业版本与当前题目：作答总数、已确认、有明确建议、需人工记分。
4. 切换题目后，证据列表和四项统计同步切换，且学生按数字学号升序展示。
5. 切换作业版本后，题目选择自动回退到新作业按 `order_index` 排序后的第一题，旧作业证据不残留。
6. 选择器切换不触发任何写命令；固定说明继续强调填空精确匹配、简答逐点引用原文且不自动确认。

## 禁止范围

- 不修改 `SubjectiveReviewTab` 状态、effect、过滤、排序、统计或命令编排。
- 不创建生产组件，不修改 OCR、评分、逐项/整题终审、答案/rubric 沉淀、链接、发布事务或 CSS。
- 不修改其他 Tab、Rust、Tauri command、SQL、迁移、依赖或现有测试。
- 不暂存、不提交、不 tag、不合并、不推送、不发布。

## 必跑验收

1. 新脚本 Python 语法检查。
2. `node --test tests/frontend/examPure.test.ts`。
3. `npm run build`。
4. KnowledgeTab、QuestionTab、SubjectiveLinkPanel、GradeTab、SubjectiveAttemptPublishPanel 与新增工具栏 UI 回归。
5. 生产 `Exam.tsx` 与 R2-E6 改前 SHA-256 保持不变。
6. 既有脏文件哈希、完整展开 porcelain、Git index、HEAD 与 whitespace 保护。

## 停止条件

- 需要修改生产实现或现有测试才能通过；
- mock 无法用真实 `SubjectiveWorkbench` 字段表达多作业、多题和四类行状态；
- 选择器切换意外触发业务写命令；
- 非目标既有脏文件、暂存区或 HEAD 变化。

## 完成口径

只有所有验收通过，才允许记为“R2-E6 主观题工作台工具栏测试前置自动化通过，生产仍内联、未集成”。不得写成主观题工作台已拆完、评分/发布逻辑已优化或老师已减负。
