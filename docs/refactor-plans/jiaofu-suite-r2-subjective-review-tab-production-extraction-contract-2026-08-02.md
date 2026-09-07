---
title: jiaofu-suite R2 SubjectiveReviewTab production extraction contract
date: 2026-08-02
status: authorized_local_production_extraction
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_exam_sha256: b9e47374c6c7b0ae2109191b36d5f8d980bfad4c3243f09f5e94f439413817c4
source_candidate_sha256: a150fed3f8c86251cef9187ed18236d12200ae3b776f0d8f7050d606ecef7465
test_contract_sha256: 5502a0d6b66ea5a3000b06b9733b505260b8c6ab1cc07b37d26f2f5202b67d7a
characterization_test_sha256: 4256704a9767af450fa51ad63e91adcf2b596b11756abb399b7c06e1ee34a10c
scope: R2-subjective-review-tab-production-extraction
integration_authorized: false
release_authorized: false
---

# R2 `SubjectiveReviewTab` 生产外移执行合同

## 前置证据

R2-E11 浏览器特征测试已冻结完整答题卡主观题终审 Tab 的成功路径、失败保留和空态，覆盖九个目标命令、十一条调用路径、五个已外移证据/编辑/链接/发布子面板，以及老师终审、评分点证据、答案库晋级和显式发布边界。十一组 UI、6/6 纯函数、Vite 70 modules build 均通过；生产 `Exam.tsx` 保持 SHA-256 `b9e47374...817c4`。

本生产批次快照 `/tmp/jiaofu-r2-subjective-review-tab-production-extraction.TwwRnW` 冻结当前 71 个工作树文件、完整 porcelain、空 staged diff、HEAD、2,400 行父文件及 547 行候选原块。候选原块由三个仅供主观题终审使用的解析 helper 和完整 `SubjectiveReviewTab` 按原始顺序拼接，SHA-256 为 `a150fed3...7465`。

## 唯一允许的生产变更

1. 新建 `src/pages/exam/SubjectiveReviewTab.tsx`。
2. 原样迁移唯一私有 `teacherComponentResults`、`rubricEvidencePromotions`、`shortAnswerPointResults` 和完整 `SubjectiveReviewTab`；只允许增加组件文件所需 import 与文件末尾 export。
3. `src/pages/Exam.tsx` 只增加唯一组件 import，删除已迁移的三个 helper 和第 1924～2400 行组件原块，以及失效的 `SubjectiveWorkbenchRow`、九个专属主观题写命令、五个专属子组件和 `fillAnswerSlots` import。
4. 父级继续保留 `SubjectiveWorkbench`、`examAnswerSheetSubjectiveWorkbench`、顶层 Tab、统一加载与 `done → load()`；`shortAnswerRubricPoints` 继续保留给答案整理使用。

## 必须保持

1. 新组件 props 仍只有 `workbench / onDone / onError`；内部状态、effect、memo、作业/题目筛选、数字学号排序、统计和函数体不重写。
2. OCR 校正、区域重识别、逐点评分建议、接受建议、人工总分、逐项终审、可接受答案晋级、评分点证据晋级和整份发布的参数、幂等键、确认框、校验、消息与失败保留不变。
3. 答案槽位和评分点的稳定 ID、得分范围、给分证据、人工备注、历史版本不改写、当前作业成绩不回写、显式发布等边界不变。
4. 空态、工具栏、知识/能力链接、原图/OCR/标准内容证据、逐项编辑、发布面板和所有条件展示不变。
5. 父级继续持有模块加载、顶层错误/成功提示和统一刷新；固定上传、客观题、默写、补录、题库与知识点均不移动。
6. R2-E11 测试合同和特征测试内容及哈希不得变化。

## 禁止范围

- 不重命名、重排、简化或重写候选内部代码。
- 不修改主观题状态、阈值、校验、DTO、API、Rust、SQL、迁移、CSS、测试断言或夹具。
- 不移动父级加载刷新、固定上传、普通卷、答题卡客观题、默写、老师补录、题库或知识点。
- 不顺带提取共享 hook/service，不改变 `Exam` 顶层错误和成功提示。
- 不暂存、不提交、不 tag、不合并、不推送、不发布。

## 机械比较

1. 新文件移除 import、末尾 export 和其相邻固定空行后，547 行 helper/组件候选块必须与快照逐字节一致。
2. 改后父文件通过移除唯一新 import、恢复 16 个失效 import，并在三个原锚点插回冻结块，可逐字节重建改前 `Exam.tsx`。
3. `SubjectiveReviewTab` 定义和父级调用各 1 处；新文件只出现目标九类主观题写命令，父文件不再出现这些写命令和五个专属子组件。
4. 父级 `examAnswerSheetSubjectiveWorkbench` 与 `done → load()` 不变；所有非目标调用继续存在。

## 必跑验收

1. 候选原块与父文件固定逆变换机械比较。
2. `npm run build`。
3. `python3 -m py_compile scripts/test_exam_subjective_review_tab_ui.py` 与 `node --test tests/frontend/examPure.test.ts`。
4. R2 十一组浏览器 UI 回归全部通过。
5. 70/70 批前非目标路径状态、完整 porcelain、Git index、HEAD 与 whitespace 保护；另单独核验目标父文件机械逆变换。

## 停止条件

- 机械比较不一致；
- 需要修改生产语义、现有测试或 CSS 才能通过；
- 父级统一加载刷新或 workbench 调用消失，新组件出现目标九类之外的写命令；
- 非目标文件、暂存区或 HEAD 变化。

## 完成口径

全部验收通过后只能记为“R2 完整 `SubjectiveReviewTab` 行为保持式生产外移自动化通过，未集成、未发布”。不得写成 OCR/评分准确、成绩自动发布、真实老师减负或 R2 整体重构完成。
