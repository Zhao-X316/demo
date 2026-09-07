---
title: jiaofu-suite R2 SubjectiveAttemptPublishPanel production extraction contract
date: 2026-08-02
status: authorized_local_production_extraction
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_exam_sha256: 23047ace9e249b2975de59e70b60f23296607eabe3ae7d9b04ea27a108113dae
source_characterization_sha256: 1c234d242e4c113775fd00735f674df74287fad0c148a5263bcebbf0e07e5125
scope: R2-SubjectiveAttemptPublishPanel-production-extraction
integration_authorized: false
release_authorized: false
---

# R2 `SubjectiveAttemptPublishPanel` 生产外移执行合同

## 目标

在 R2-E5 浏览器特征测试已通过的前提下，只把 `src/pages/Exam.tsx` 中改前第 2557～2575 行整卷发布展示块机械外移到 `src/pages/exam/SubjectiveAttemptPublishPanel.tsx`。Tauri 发布命令、确认框、成功刷新和失败处理继续由父级 `SubjectiveReviewTab` 持有。

## 唯一允许的生产修改

1. 新建 `src/pages/exam/SubjectiveAttemptPublishPanel.tsx`：
   - 类型导入 `ObjectiveAttemptSummary`；
   - 接收 `attempts`、`busy` 与 `onPublish(attemptId, studentName)`；
   - 原样包含改前第 2557～2575 行，只把回调名 `publishAttempt` 机械替换为 `onPublish`，不增加真实 DOM 包装。
2. 修改 `src/pages/Exam.tsx`：
   - 增加唯一组件 import；
   - 用组件调用替换改前固定第 2557～2575 行；
   - 直接传入父级 `attempts`、`busy` 和 `publishAttempt`。

## 禁止范围

- 不移动或修改 `publishAttempt` 函数、`examAnswerSheetSubjectivePublishAttempt` import、确认文案、成功/失败消息、父级 `load()` 或 busy 状态。
- 不修改 attempt 排序/筛选、三种状态、分数/题数、已发布总分、按钮文案、class、禁用规则或 CSS。
- 不修改 OCR、评分、老师逐项/整题终审、答案/rubric 沉淀、知识/能力链接或任何其他 Tab。
- 不修改 API/DTO、Rust、Tauri command、SQL、迁移、依赖或测试。
- 不暂存、不提交、不 tag、不合并、不推送、不发布。

## 机械不变量

1. 新组件去掉固定 import/函数/props/fragment 包装，并把 `onPublish` 还原为 `publishAttempt` 后，内部第 2557～2575 行必须与改前逐字节一致。
2. 改后父文件必须与“改前副本 + 唯一组件 import + 固定组件调用 - 改前第 2557～2575 行”逐字节一致。
3. 新组件定义 1 处、父级调用 1 处；父级 `publishAttempt` 定义和发布命令调用各保持 1 处。
4. R2-E5 测试脚本和测试前置合同哈希不得变化。

## 必跑验收

1. 子组件内部与父文件机械比较。
2. `node --test tests/frontend/examPure.test.ts`。
3. `npm run build`。
4. KnowledgeTab、QuestionTab、SubjectiveLinkPanel、GradeTab 与 R2-E5 发布面板五组 UI 回归。
5. 非目标工作树哈希、完整展开 porcelain、Git index 与 whitespace 保护。

## 停止条件

- 需要把发布命令或确认框移入子组件；
- 机械比较不能由唯一 import、固定替换和回调改名解释；
- 需要修改 R2-E5 或任何现有测试；
- 任一发布状态、门禁、参数、调用次数、成功刷新或失败保留变化；
- 非目标既有脏文件、暂存区或 HEAD 变化。

## 完成口径

只有所有验收通过，才允许记为“`SubjectiveAttemptPublishPanel` 行为保持式生产外移自动化通过，未集成”。不得写成发布事务已优化、主观题 Tab 已拆完或老师已减负。
