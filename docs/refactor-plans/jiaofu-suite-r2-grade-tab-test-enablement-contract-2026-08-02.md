---
title: jiaofu-suite R2-E4 GradeTab test enablement contract
date: 2026-08-02
status: authorized_local_test_enablement
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_exam_sha256: 581df186e452f5cc810aec14e5ddab3074dd52a396d0f9a91feffc02d812f3f9
scope: R2-E4-GradeTab-characterization
production_extraction_authorized: false
integration_authorized: false
release_authorized: false
---

# R2-E4 `GradeTab` 测试前置执行合同

## 目标

只为“改作业 → 题目批改 → 老师补录”建立浏览器特征测试，冻结当前 `GradeTab` 的客观题筛选、作答录入、机器建议、老师终审/改判及失败保留语义。生产组件本批仍内联。

## 只读结论

- `SubjectiveReviewTab` 同时持有 OCR 修正、重试、逐点评分、人工纠正、未来答案/rubric 更新和发布等 9 条写命令，不适合下一批整体移动。
- `GradeTab` 改前第 3198～3363 行，共 166 行，只依赖 `students / questions / answers / onDone / onError`、局部状态、`TYPE_LABEL`、`displayTime` 和两条命令。
- 当前仓库没有针对 `exam_answer_suggest` / `exam_answer_human_decide` 或“老师补录”Tab 的前端测试。

## 唯一允许修改

- 新建 `scripts/test_exam_grade_tab_ui.py`。

不得修改 `Exam.tsx`、现有测试、API/DTO、CSS、依赖、Rust、Tauri command、SQL、迁移或业务文案。

## 必须冻结的当前行为

1. 主观题不进入补录题目下拉；客观题标准答案、分值和待确认计数按当前数据展示。
2. 缺学生/题目/学生答案时不调用机器建议，并显示当前必填错误。
3. 机器建议必须精确调用 `exam_answer_suggest`；成功后清空学生答案、显示“必须老师确认”提示并通过父级重新加载。
4. 机器建议失败时保留学生答案、不新增作答、不显示成功提示。
5. 老师终审必须精确调用 `exam_answer_human_decide`，包含裁决和 trim 后的备注/null。
6. 首次确认、同结论同备注、同结论改备注和改判四类成功提示保持当前语义。
7. 终审失败时保留备注和待确认状态，不伪造已终审或派生统计变化。
8. 所有机器结果继续只是建议；只有老师命令产生最终结论。

## 必跑验收

1. `python3 -m py_compile scripts/test_exam_grade_tab_ui.py`。
2. `node --test tests/frontend/examPure.test.ts`。
3. `npm run build`。
4. 既有 `KnowledgeTab`、`QuestionTab`、`SubjectiveLinkPanel` UI 回归。
5. 新增 `GradeTab` 成功/失败 UI 路径。
6. `Exam.tsx`、现有测试和所有既有脏文件哈希不变。
7. 排除唯一新测试后 porcelain、Git index 与施工前逐字节一致；新文件无 whitespace 诊断。

## 停止条件

- 为使测试通过必须修改生产代码或现有测试；
- mock 不能区分机器建议与老师终审；
- 无法精确断言命令参数或失败后状态；
- 非目标工作树或暂存区发生变化。

## 完成口径

所有验收通过后，只允许记为“R2-E4 `GradeTab` 测试前置自动化通过，生产仍内联”。组件生产外移须另立合同。
