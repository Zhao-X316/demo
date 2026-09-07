---
title: jiaofu-suite R2 ObjectiveReviewTab production extraction contract
date: 2026-08-02
status: authorized_local_production_extraction
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_exam_sha256: 1176d253ae266cecf47a354a62ea4c0ae6a91712c632ac01ec207a4d1c211efa
test_contract_sha256: fee52744da3f41e85145518b26a13fcee40dc81f8ea9d6af4113a2217bfdea7a
characterization_test_sha256: 073b9a9122beef906827a590c018d135aabe72c1838869937cbc49ff51a1a721
scope: R2-objective-review-tab-production-extraction
integration_authorized: false
release_authorized: false
---

# R2 `ObjectiveReviewTab` 生产外移执行合同

## 前置证据

R2-E9 浏览器特征测试已冻结完整标准卷客观题终审 Tab 的空态、作业/题目回退、数字学号排序、证据、0.95 严格批量、逐条接受、人工记分、识别重试、整卷发布和五类失败保留。九组 UI、6/6 纯函数、Vite 68 modules build、61/61 批前文件与 Git 保护均通过，生产 `Exam.tsx` 保持 SHA-256 `1176d253...1efa`。

## 唯一允许的生产变更

1. 新建 `src/pages/exam/ObjectiveReviewTab.tsx`。
2. 原样迁移 `OBJECTIVE_TYPE_LABEL / OBSERVATION_LABEL / EXCLUSION_LABEL`、`displayObservedAnswer`、`objectiveOutcomeLabel` 和完整 `ObjectiveReviewTab`。
3. `src/pages/Exam.tsx` 只增加唯一组件 import，删除已迁移块与失效的 `ObjectiveWorkbenchRow`、`examObjectiveAccept`、`examObjectiveCorrect`、`examObjectivePublishAttempt`、`examObjectiveStrictBatchAccept` import。
4. `examObjectiveRecognizeRegion` 同时被固定上传流程和该 Tab 使用，因此父文件与新组件各自保留/引入，不得误删父级调用。

## 必须保持

1. 新组件 props 仍只有 `workbench / onDone / onError`；所有内部状态、effect、memo、排序、统计和函数体逐字保持。
2. 0.95 严格批量阈值、当前题全部 suggestion IDs、幂等键前缀、人工分数与证据门禁、浏览器发布确认均不变。
3. 五类客观题写命令、参数、成功/失败提示、busy 语义及 `onDone` 触发父级刷新均不变。
4. 原图/缺图、观察/识别/建议/终审展示和空态文案不变。
5. 父级继续持有顶层 Tab、所有模块加载、全局 error/toast 和 `done → load()`；其他六个 Tab 不移动。
6. R2-E9 合同和测试内容及哈希不得变化。

## 禁止范围

- 不重命名、重排、简化或重写候选内部代码。
- 不修改客观题阈值、DTO、API、Rust、SQL、迁移、CSS、测试断言或夹具。
- 不移动父级加载刷新、固定上传、答题卡主观题、默写、老师补录、题库或知识点。
- 不顺带提取共享 hook/service，不改变 `Exam` 顶层错误和成功提示。
- 不暂存、不提交、不 tag、不合并、不推送、不发布。

## 机械比较

1. 新文件中的 22 行标签块和 291 行 helper/组件块必须分别与快照逐字节一致。
2. 改后父文件通过移除唯一新 import、恢复 5 个失效 import、在原锚点插回两个冻结块，可逐字节重建改前 `Exam.tsx`。
3. `ObjectiveReviewTab` 定义和父级调用各 1 处；新文件只出现五类目标客观题命令。
4. `Exam.tsx` 中固定上传的 `examObjectiveRecognizeRegion` 调用继续存在。

## 必跑验收

1. 两个迁移块与父文件固定变换机械比较。
2. `npm run build`。
3. 新测试 Python 语法与 `node --test tests/frontend/examPure.test.ts`。
4. R2 九组浏览器 UI 回归全部通过。
5. 62/62 批前非目标文件、完整 porcelain、Git index、HEAD 与 whitespace 保护。

## 停止条件

- 机械比较不一致；
- 需要修改生产语义、现有测试或 CSS 才能通过；
- 父级固定上传识别调用消失，或新组件出现目标五类之外的写命令；
- 非目标文件、暂存区或 HEAD 变化。

## 完成口径

全部验收通过后只能记为“R2 完整 `ObjectiveReviewTab` 行为保持式生产外移自动化通过，未集成、未发布”。不得写成客观题识别准确、整卷自动发布、老师真实减负或 R2 整体重构完成。
