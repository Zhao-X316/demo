---
title: jiaofu-suite R2 SubjectiveEvidenceSummary production extraction contract
date: 2026-08-02
status: authorized_local_production_extraction
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_exam_sha256: 5c011aa0f1ff83160c8890af68fe542da076ceba02d29effdad2318683321f0d
source_characterization_sha256: df0657d5d280381831256f8ea521656f325109c294908b13068c74309f5d9eba
source_test_contract_sha256: 08e48b0c68d704232dd51e3341472c5095ca686d486e8ce7074698d1056b2502
scope: R2-SubjectiveEvidenceSummary-production-extraction
integration_authorized: false
release_authorized: false
---

# R2 `SubjectiveEvidenceSummary` 生产外移执行合同

## 目标

在 R2-E7 浏览器特征测试已通过的前提下，只把 `src/pages/Exam.tsx` 当前第 2333～2383 行的学生身份、机器证据和简答逐点评分展示机械外移到 `src/pages/exam/SubjectiveEvidenceSummary.tsx`，并随展示块移动其两项唯一私有显示逻辑。父级 `SubjectiveReviewTab` 继续持有数据解析、老师逐项确认、整题终审、答案/评分规则沉淀和发布动作。

## 唯一允许的生产修改

1. 新建 `src/pages/exam/SubjectiveEvidenceSummary.tsx`：
   - 导入 `convertFileSrc`、`SubjectiveWorkbenchRow` 类型与纯展示函数 `answerJsonLabel`；
   - 接收 `row / rubricPoints / pointResults / promotedRubricEvidence`；
   - 原样包含改前第 2333～2383 行，并仅增加无真实 DOM 的 fragment 外壳；
   - 原样接管父文件第 282～288 行 `SHORT_ANSWER_POINT_STATUS` 和第 1963～1977 行 `subjectiveStateLabel`。
2. 修改 `src/pages/Exam.tsx`：
   - 增加唯一组件 import；
   - 删除上述两项已迁移、且只服务于该展示块的私有显示逻辑；
   - 用固定组件调用替换改前第 2333～2383 行；
   - 传入父级已经解析好的四项数据。

## 禁止范围

- 不移动或修改 `shortAnswerPointResults`、`shortAnswerRubricPoints`、`teacherComponentResults`、`rubricEvidencePromotions` 或其解析时机。
- 不移动 `componentSpecs / confirmedComponents / promotedRubricEvidence` 的后续老师逐项确认展示和写操作。
- 不新增子组件状态、effect、memo、Tauri 写命令、真实 DOM 包装、字段转换、排序或默认值。
- 不修改图片路径、alt、状态文案、答案文案、置信度、建议/终审得分、评分点状态、证据引用、兜底文案、class 或 CSS。
- 不修改 OCR、评分、逐项/整题终审、答案/rubric 沉淀、链接、发布事务或其他 Tab。
- 不修改 API/DTO、Rust、Tauri command、SQL、迁移、依赖或测试。
- 不暂存、不提交、不 tag、不合并、不推送、不发布。

## 机械不变量

1. 新组件去掉固定 import、类型、常量、函数、props 和 fragment 外壳后，三个迁移块必须分别与改前第 282～288、1963～1977、2333～2383 行逐字节一致。
2. 改后父文件必须与“改前副本 + 唯一组件 import + 固定组件调用 - 三个迁移块”逐字节一致。
3. `SubjectiveEvidenceSummary` 定义 1 处、父级调用 1 处；当前全部 11 个主观题写调用和老师逐项确认区继续只在父级。
4. R2-E7 测试脚本和测试前置合同哈希不得变化。

## 必跑验收

1. 子组件三个迁移块与父文件固定变换机械比较。
2. `python3 -m py_compile scripts/test_exam_subjective_evidence_summary_ui.py`。
3. `node --test tests/frontend/examPure.test.ts`。
4. `npm run build`。
5. KnowledgeTab、QuestionTab、SubjectiveLinkPanel、GradeTab、SubjectiveAttemptPublishPanel、SubjectiveReviewToolbar 与 R2-E7 证据摘要七组 UI 回归。
6. 非目标工作树哈希、完整展开 porcelain、Git index、HEAD 与 whitespace 保护。

## 停止条件

- 需要移动任一写命令、终审表单、数据解析或后续老师逐项确认展示；
- 机械比较不能由唯一 import、固定组件调用和三个迁移块删除解释；
- 需要修改 R2-E7 或任何现有测试；
- 任一图片、事实字段、机器状态、逐点评分、兜底或老师终审显示变化；
- 非目标既有文件、暂存区或 HEAD 变化。

## 完成口径

只有所有验收通过，才允许记为“`SubjectiveEvidenceSummary` 行为保持式生产外移自动化通过，未集成”。不得写成主观题终审已自动化、真实 OCR 已验证、老师已减负或主观题 Tab 已拆完。
