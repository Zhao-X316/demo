---
title: jiaofu-suite R2-E9 ObjectiveReviewTab test enablement contract
date: 2026-08-02
status: authorized_local_test_enablement
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_exam_sha256: 1176d253ae266cecf47a354a62ea4c0ae6a91712c632ac01ec207a4d1c211efa
scope: R2-E9-objective-review-tab-characterization
production_extraction_authorized: false
integration_authorized: false
release_authorized: false
---

# R2-E9 `ObjectiveReviewTab` 测试前置执行合同

## 目标

在不修改生产代码的前提下，为 `src/pages/Exam.tsx` 当前完整“标准卷终审” Tab 建立浏览器特征保护。只有按题筛选、证据展示、严格批量、逐条接受、人工记分、识别重试和整卷发布均被真实浏览器测试冻结后，才能另开合同评估整个 Tab 的生产外移。

## 候选边界取证

1. 候选是完整 `ObjectiveReviewTab`，包括其私有 `OBJECTIVE_TYPE_LABEL / OBSERVATION_LABEL / EXCLUSION_LABEL`、`displayObservedAnswer` 和 `objectiveOutcomeLabel`；不是局部按钮或展示碎片。
2. 候选只接收 `ObjectiveWorkbench / onDone / onError`，内部拥有作业/题目选择、busy、人工分数和证据备注状态。
3. 候选的五个业务动作只调用客观题领域命令：逐条接受、人工修正、严格批量、区域重识别和整卷发布。成功后由父级 `onDone` 触发统一刷新，失败由 `onError` 保留当前操作上下文。
4. 父级继续加载学生、题库、知识点、答案、三类 workbench 和上传选项，并持有顶层 Tab、全局错误/成功提示；固定上传、答题卡主观题、默写、老师补录、题库和知识点均不属于候选。

## 唯一允许修改

1. 新建 `scripts/test_exam_objective_review_tab_ui.py`。
2. 测试可复用既有浏览器 Tauri mock，只覆盖 `exam_objective_workbench` 与五个客观题写命令；不得修改生产、API/DTO、样式或现有测试。
3. 完成后只回写对应验收、任务地图和修改记录，不借此扩大生产授权。

## 必须冻结的当前行为

1. workbench 为空时显示保守空态，明确真实识别前置条件和“快速批改”兜底；不得出现终审写按钮。
2. 作业版本和题目选项从 workbench 派生；切换作业时题目回退到该版本首题，同题学生按数字学号升序排列。
3. 统计分别显示本题作答数、已确认数、严格批量合格数和需单独处理数；严格批量阈值文案固定为 0.95，异常类型明确排除。
4. 原图/缺图、观察状态、识别答案、置信度、机器建议、建议分、排除原因和老师终审事实均可见；布尔判断、选择标签与多值冲突保持当前展示规则。
5. 严格批量只在存在合格项时可触发，提交当前题全部 suggestion IDs、0.95 阈值和唯一幂等键；成功提示确认数和排除数，机器建议仍不等于发布。
6. 逐条接受只对有建议分且未确认记录可用；成功只确认当前 suggestion，并提示整卷仍需显式发布。
7. 人工记分必须是 `0..max_score` 有限数值且必须填写查看原图后的证据依据；校验失败不得调用业务命令，合法提交保持当前参数和成功提示。
8. 识别失败且未终审记录显示“重新整理本题”；命令使用 answer region revision 与唯一幂等键，成功后仍提示老师确认。
9. 整卷发布仅对 `can_publish` attempt 开放，必须经过浏览器确认；取消不调用命令，确认后保持总分和 publication revision 成功提示。
10. 五类命令失败时保留当前未确认状态和人工输入，不显示成功提示；全过程不得触发主观题、默写、补录、题库或知识点写命令。

## 禁止范围

- 不创建生产组件，不移动任何 helper/状态/命令，不修改 `Exam.tsx`。
- 不修改客观题阈值、校验、DTO、确认/发布语义或 CSS。
- 不修改固定上传、主观题、默写、老师补录、题库、知识点、Rust、Tauri command、SQL、迁移、依赖或现有测试。
- 不暂存、不提交、不 tag、不合并、不推送、不发布。

## 必跑验收

1. 新脚本 Python 语法检查。
2. `node --test tests/frontend/examPure.test.ts`。
3. `npm run build`。
4. KnowledgeTab、QuestionTab、SubjectiveLinkPanel、GradeTab、SubjectiveAttemptPublishPanel、SubjectiveReviewToolbar、SubjectiveEvidenceSummary、SubjectiveComponentEditor 与新增 ObjectiveReviewTab 共九组 UI 回归。
5. 生产 `Exam.tsx` 与 R2-E9 改前 SHA-256 保持不变。
6. 61 个批前脏文件哈希、完整展开 porcelain、Git index、HEAD 与 whitespace 保护。

## 停止条件

- 需要修改生产实现或现有测试才能通过；
- 只能用真实 DTO 不可能产生的状态组合才能覆盖终审流程；
- 校验/取消失败仍触发写调用，或任一动作触发目标以外的写命令；
- 非目标既有文件、暂存区或 HEAD 变化。

## 完成口径

只有所有验收通过，才允许记为“R2-E9 标准卷客观题终审 Tab 测试前置自动化通过，生产仍内联、未集成”。不得写成 Tab 已生产外移、客观题识别/评分准确或老师真实减负已验证。
