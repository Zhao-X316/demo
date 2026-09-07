---
title: jiaofu-suite R2-E11 SubjectiveReviewTab test enablement contract
date: 2026-08-02
status: authorized_local_test_enablement
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_exam_sha256: b9e47374c6c7b0ae2109191b36d5f8d980bfad4c3243f09f5e94f439413817c4
scope: R2-E11-subjective-review-tab-characterization
production_extraction_authorized: false
integration_authorized: false
release_authorized: false
---

# R2-E11 `SubjectiveReviewTab` 测试前置执行合同

## 目标

在不修改生产代码的前提下，为 `src/pages/Exam.tsx` 当前完整“答题卡主观题” Tab 建立浏览器特征保护。只有作业/题目筛选、原始证据、转写修正、识别重试、简答评分建议、老师终审、未来答案/评分规则晋级和整份发布的串联行为均被冻结后，才能另开合同评估整个 Tab 的生产外移。

## 候选边界取证

1. 候选是完整 477 行 `SubjectiveReviewTab`，以及只服务该 Tab 的 `teacherComponentResults`、`rubricEvidencePromotions`、`shortAnswerPointResults` 三个解析 helper；不是已经外移的五个子面板。
2. 候选只接收 `SubjectiveWorkbench / onDone / onError`，内部拥有作业/题目选择、busy、转写校正、整题人工分、逐项分数/证据/备注状态。
3. 候选拥有九类命令、十一条业务调用路径：转写修正及简答评分串联、区域重识别及简答评分串联、直接生成简答评分、接受建议、整题人工记分、逐项人工记分、填空可接受答案晋级、简答评分点证据晋级、整份发布。
4. `SubjectiveReviewToolbar`、`SubjectiveLinkPanel`、`SubjectiveEvidenceSummary`、`SubjectiveComponentEditor`、`SubjectiveAttemptPublishPanel` 已有独立特征测试；本批只补完整 Tab 串联合同，不重复改写子面板。
5. 父级继续加载 workbench 并持有顶层 Tab、全局错误/成功提示和统一 `done → load()`；固定上传、标准卷、默写、老师补录、题库和知识点均不属于候选。

## 施工快照

- 路径：`/tmp/jiaofu-r2-e11-subjective-review-tab.rXDAQb`
- `Exam.tsx`：2,400 行，SHA-256 `b9e47374...817c4`。
- `SubjectiveReviewTab` 原块：477 行，SHA-256 `1abe2266...459`。
- 两段专属 helper：49 行与 21 行，SHA-256 分别为 `79b45dc3...95e`、`4e5687cd...4fd`。
- 批前展开 porcelain：69 个文件；Git index SHA-256 `0273c39f...6578`；staged diff 为空。

## 唯一允许修改

1. 新建 `scripts/test_exam_subjective_review_tab_ui.py`。
2. 测试可复用既有浏览器 Tauri mock，只覆盖完整 Tab 的 workbench 和九类命令；不得修改生产、API/DTO、样式或现有测试。
3. 完成后只回写对应验收、任务地图和修改记录，不借此扩大生产授权。

## 必须冻结的当前行为

1. workbench 为空时显示保守空态，明确填空采用确定性答案比对、篇幅受控简答按评分点整理原文证据；不得出现终审或发布按钮。
2. 作业版本、题目选项和统计由 workbench 派生；切换作业时题目回退到该版本首题，同题学生按数字学号升序排列。
3. 已外移的工具栏、知识/能力链接、证据摘要、逐项编辑器和整卷发布面板在完整 Tab 中继续可见、可交互。
4. 转写空文本前端阻断；合法修正只追加老师校正文本。简答题修正成功后以新 revision 串联评分，评分失败不得回滚已保存转写，并以“评分建议暂未生成”提示。
5. 识别失败记录可重试；识别成功且为简答题时以新 revision 串联评分。串联评分失败不得伪报识别失败，旧转写/失败记录仍保留。
6. 已识别但尚无简答分析的记录可以直接生成逐点评分建议；机器建议必须由老师接受后才形成终审，整份答题卡仍需显式发布。
7. 整题人工得分必须是 `0..max_score` 有限值并填写判定依据；逐项人工记分继续沿用已冻结的槽位/评分点、正分需原文证据和自动汇总规则。
8. 单空填空只有在老师修正后满分、机器原建议非正确且尚未晋级时，才允许经确认把老师确认写法加入未来答案版本；取消不写，历史成绩不变。
9. 已确认简答逐项证据只有正分且存在原文、尚未晋级时，才允许经确认加入未来评分规则；结果提示沿用的知识/能力链接数，历史成绩不变。
10. 整份发布只对 `can_publish` attempt 开放，必须经浏览器确认；取消不写，确认后提示总分与 publication revision。
11. 外层命令失败时保留当前输入、按钮和未确认状态，不显示成功提示；全过程不得触发客观题、默写、补录、题库或知识点写命令。

## 禁止范围

- 不创建生产组件，不移动 helper/状态/命令，不修改 `Exam.tsx`。
- 不修改主观题状态、评分规则、校验、DTO、确认/发布语义、CSS 或五个已外移子面板。
- 不修改固定上传、普通卷、默写、老师补录、题库、知识点、Rust、Tauri command、SQL、迁移、依赖或现有测试。
- 不暂存、不提交、不 tag、不合并、不推送、不发布。

## 必跑验收

1. 新脚本 Python 语法检查。
2. `node --test tests/frontend/examPure.test.ts`，保持 6/6。
3. `npm run build`。
4. 既有十组 UI 与新增完整 `SubjectiveReviewTab` 共十一组 UI 回归。
5. 生产 `Exam.tsx` 与 R2-E11 改前 SHA-256 保持不变。
6. 69 个批前脏文件哈希、排除本合同/新测试后的完整展开 porcelain、Git index、HEAD 与 whitespace 保护。

## 停止条件

- 需要修改生产实现或现有测试才能通过；
- 只能用真实 DTO 不可能产生的状态组合才能覆盖终审流程；
- 前端校验或确认取消仍触发写调用；
- 串联评分失败回滚已成功的转写/识别，或外层失败丢失当前输入；
- 非目标既有文件、暂存区或 HEAD 变化。

## 完成口径

只有所有验收通过，才允许记为“R2-E11 主观题终审 Tab 测试前置自动化通过，生产仍内联、未集成”。不得写成 Tab 已生产外移、OCR/评分准确、真实答题卡 GUI 或老师真实减负已验证。
