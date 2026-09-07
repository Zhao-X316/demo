---
title: jiaofu-suite R2-E5 SubjectiveAttemptPublishPanel test enablement contract
date: 2026-08-02
status: authorized_local_test_enablement
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_exam_sha256: 23047ace9e249b2975de59e70b60f23296607eabe3ae7d9b04ea27a108113dae
scope: R2-E5-subjective-attempt-publish-panel-characterization
production_extraction_authorized: false
integration_authorized: false
release_authorized: false
---

# R2-E5 `SubjectiveAttemptPublishPanel` 测试前置执行合同

## 目标

在不修改生产代码的前提下，为 `src/pages/Exam.tsx` 改前固定第 2557～2575 行的“整份答题卡发布”面板建立浏览器特征保护，随后才能单独评估把该展示/回调边界外移为 `SubjectiveAttemptPublishPanel`。

## 唯一允许修改

1. 新建 `scripts/test_exam_subjective_attempt_publish_panel_ui.py`。
2. 测试只扩展浏览器 Tauri mock；不得改生产 `Exam.tsx`、现有测试或 API/DTO。
3. 完成后只回写对应验收、任务地图和修改记录，不借此扩大生产授权。

## 必须冻结的当前行为

1. 面板显示“整份答题卡发布”及“所有题型终审完成后才可发布”。
2. `published`、`can_publish=true`、其余未完成三类 attempt 分别显示“已发布”“待发布”“终审中”。
3. 每张卡显示老师总分/满分、已终审题数/总题数；存在 `published_total_score` 时显示当前已发布总分。
4. 已发布或未满足门禁的按钮禁用；只有 `can_publish=true` 的 attempt 可点击“确认发布整份答题卡”。
5. 点击可发布项先出现包含学生姓名且明确只采用当前老师终审 revision 的确认框；取消时不得调用发布命令。
6. 确认后只调用一次 `exam_answer_sheet_subjective_publish_attempt`，精确参数为所选 `attemptId`；成功经父级重载显示发布状态和成功消息。
7. 发布失败时显示错误，不伪造成功消息或已发布状态，原待发布卡仍可重试。

## 禁止范围

- 不修改主观题 OCR、评分点建议、老师逐项/整题终审、rubric/答案沉淀、链接、发布事务或 CSS。
- 不创建生产组件，不改变 `SubjectiveReviewTab` props、状态或命令编排。
- 不修改 `ObjectiveReviewTab`、`DictationReviewTab`、三材料上传、Rust、Tauri command、SQL、迁移或依赖。
- 不暂存、不提交、不 tag、不合并、不推送、不发布。

## 必跑验收

1. 新脚本 Python 语法检查。
2. `node --test tests/frontend/examPure.test.ts`。
3. `npm run build`。
4. `KnowledgeTab`、`QuestionTab`、`SubjectiveLinkPanel`、`GradeTab` 与新增发布面板 UI 回归。
5. 生产 `Exam.tsx` 与 R2-E5 改前 SHA-256 保持不变。
6. 既有脏文件哈希、完整展开 porcelain、Git index 与 whitespace 保护。

## 停止条件

- 需要修改生产实现或现有测试才能通过；
- 现有行为无法区分取消、成功或失败；
- mock 需要伪造与真实 Tauri DTO 不一致的同步突变；
- 非目标既有脏文件、暂存区或 HEAD 变化。

## 完成口径

只有所有验收通过，才允许记为“R2-E5 发布面板测试前置自动化通过，生产仍内联、未集成”。不得写成发布事务已优化、整卷发布可投产或老师已减负。
