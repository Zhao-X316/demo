---
title: jiaofu-suite R2-E7 SubjectiveEvidenceSummary test enablement contract
date: 2026-08-02
status: authorized_local_test_enablement
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_exam_sha256: 5c011aa0f1ff83160c8890af68fe542da076ceba02d29effdad2318683321f0d
scope: R2-E7-subjective-evidence-summary-characterization
production_extraction_authorized: false
integration_authorized: false
release_authorized: false
---

# R2-E7 `SubjectiveEvidenceSummary` 测试前置执行合同

## 目标

在不修改生产代码的前提下，为 `src/pages/Exam.tsx` 当前第 2333～2383 行每名学生卡片中的“身份、原图/事实、机器逐点评分”纯展示区建立浏览器特征保护。随后才能单独评估连同其唯一私有状态标签函数/映射外移为 `SubjectiveEvidenceSummary`。

## 候选边界取证

1. 候选展示区读取 `row / rubricPoints / pointResults / promotedRubricEvidence`，不直接修改父级状态，不调用 Tauri，也不包含老师终审按钮。
2. 当前 `subjectiveStateLabel` 与 `SHORT_ANSWER_POINT_STATUS` 只服务该展示区；如后续外移，必须与展示区一并保持唯一所有权，不得重写语义。
3. 老师逐项确认展示、加入未来规则按钮、OCR 重试、机器评分生成、接受/校正、人工记分和发布继续留在父级。

## 唯一允许修改

1. 新建 `scripts/test_exam_subjective_evidence_summary_ui.py`。
2. 测试复用 R2-E6 浏览器 mock 并只覆盖主观题 workbench 读取结果；不得改生产、现有测试、API/DTO 或样式。
3. 完成后只回写对应验收、任务地图和修改记录，不借此扩大生产授权。

## 必须冻结的当前行为

1. 学生姓名、数字学号、题号、填空/简答、满分和“已终审/需老师处理/有评分建议”标签准确显示。
2. 有 `crop_path` 时显示正确图片和 alt；缺图时显示“裁剪图不可用”。
3. 机器状态、原始 OCR、老师校正、标准答案、置信度取整、建议得分与空值回退准确显示。
4. 已终审行显示老师得分及“人工修正/接受建议”；答案写法或 rubric 示例已沉淀时显示相应标记和数量。
5. 简答逐点评分显示状态中文、建议分/满分、原因和学生原文；无原因或无原文时使用当前保守提示。
6. 简答尚无分析时显示已确认 rubric 点列表；rubric 也不完整时显示“评分点尚未完整，必须老师人工核对”。
7. 只切换作业/题目查看证据不得触发任何写命令。

## 禁止范围

- 不创建生产组件，不移动状态标签函数/映射，不修改 `Exam.tsx`。
- 不修改老师逐项/整题终审展示或操作，不修改 OCR、评分、答案/rubric 沉淀、链接、发布事务或 CSS。
- 不修改其他 Tab、Rust、Tauri command、SQL、迁移、依赖或现有测试。
- 不暂存、不提交、不 tag、不合并、不推送、不发布。

## 必跑验收

1. 新脚本 Python 语法检查。
2. `node --test tests/frontend/examPure.test.ts`。
3. `npm run build`。
4. KnowledgeTab、QuestionTab、SubjectiveLinkPanel、GradeTab、SubjectiveAttemptPublishPanel、SubjectiveReviewToolbar 与新增证据摘要 UI 回归。
5. 生产 `Exam.tsx` 与 R2-E7 改前 SHA-256 保持不变。
6. 既有脏文件哈希、完整展开 porcelain、Git index、HEAD 与 whitespace 保护。

## 停止条件

- 需要修改生产实现或现有测试才能通过；
- 夹具必须使用不可能出现在真实 DTO 中的状态组合才能覆盖展示分支；
- 查看/切换证据意外触发业务写命令；
- 非目标既有文件、暂存区或 HEAD 变化。

## 完成口径

只有所有验收通过，才允许记为“R2-E7 主观题证据摘要测试前置自动化通过，生产仍内联、未集成”。不得写成机器评分已验证准确、证据摘要已生产外移或老师已减负。
