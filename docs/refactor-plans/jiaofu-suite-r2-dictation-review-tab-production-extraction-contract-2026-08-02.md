---
title: jiaofu-suite R2 DictationReviewTab production extraction contract
date: 2026-08-02
status: authorized_local_production_extraction
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_exam_sha256: 3edafa162da1ff59792ce0b71bff77461f8531a50db47edef0fc3b50b9d3cc35
source_candidate_sha256: 5046ec10bc873ead878cba2a89c3811ea3c2ae373252c8b80dea99c53d1a1a91
test_contract_sha256: 636ffd8bee877e69c31df58291ce11b7971231b734ffd3e50a9e7a5f1a740c00
characterization_test_sha256: 47c6a372c825af05c24a0c31f488d358b5b3f69d0d36be4f88df8666c0ea62b1
scope: R2-dictation-review-tab-production-extraction
integration_authorized: false
release_authorized: false
---

# R2 `DictationReviewTab` 生产外移执行合同

## 前置证据

R2-E10 浏览器特征测试已冻结完整默写终审 Tab 的空态、作业/题目回退、数字学号排序、四项统计、七类结果、原图/OCR/标准内容证据、0.95 严格批量、OCR 校正、逐条接受、人工记分、区域重识别、整份发布及六类失败保留。十组 UI、6/6 纯函数、Vite 69 modules build、65/65 批前文件与 Git 保护均通过；生产 `Exam.tsx` 保持 SHA-256 `3edafa16...cc35`。

本生产批次快照 `/tmp/jiaofu-r2-dictation-review-tab-production-extraction.QuQYQL` 冻结当前 67 个工作树文件、完整 porcelain、空 staged diff、HEAD、2,729 行父文件及 322 行候选原块。候选原块 SHA-256 为 `5046ec10...a91`。

## 唯一允许的生产变更

1. 新建 `src/pages/exam/DictationReviewTab.tsx`。
2. 原样迁移唯一私有 `dictationStateLabel` 和完整 `DictationReviewTab`；只允许增加组件文件所需 import 与文件末尾 export。
3. `src/pages/Exam.tsx` 只增加唯一组件 import，删除已迁移的第 2408～2729 行原块，以及失效的 `DictationWorkbenchRow` 和六个专属默写写命令 import。
4. 父级继续保留 `DictationWorkbench` 类型、`examDictationAnalyzeTemplate / ConfirmTemplate / ProcessPage / TemplateStatus / Workbench`、顶层 Tab、统一加载与 `done → load()`。

## 必须保持

1. 新组件 props 仍只有 `workbench / onDone / onError`；内部状态、effect、memo、作业/题目回退、数字学号排序、统计和函数体不重写。
2. 0.95 阈值、严格批量的当前题全部 transcription revision IDs、幂等键、OCR 空文本校验、人工分数范围、必填判定依据及浏览器发布确认不变。
3. 校正原文、区域重识别、逐条接受、人工记分、严格批量、整份发布六类命令的参数、成功/失败提示、busy 与 `onDone/onError` 语义不变。
4. 原图/缺图、机器原文、老师校正、标准内容、置信度、建议分、可接受写法、终审与七类状态展示不变。
5. 父级继续持有所有模块加载、顶层错误/成功提示和统一刷新；固定上传、客观题、主观题、补录、题库与知识点均不移动。
6. R2-E10 测试合同和特征测试内容及哈希不得变化。

## 禁止范围

- 不重命名、重排、简化或重写候选内部代码。
- 不修改默写阈值、状态、校验、DTO、API、Rust、SQL、迁移、CSS、测试断言或夹具。
- 不移动父级加载刷新、固定上传、普通卷、答题卡主观题、老师补录、题库或知识点。
- 不顺带提取共享 hook/service，不改变 `Exam` 顶层错误和成功提示。
- 不暂存、不提交、不 tag、不合并、不推送、不发布。

## 机械比较

1. 新文件移除 import、末尾 export 和其相邻固定空行后，322 行 helper/组件块必须与快照逐字节一致。
2. 改后父文件通过移除唯一新 import、恢复 7 个失效 import，并在原锚点插回冻结块，可逐字节重建改前 `Exam.tsx`。
3. `DictationReviewTab` 定义和父级调用各 1 处；新文件只出现目标六类默写写命令，父文件不再出现这些写命令。
4. 父级五类默写模板/页面/workbench 读写调用继续存在，`done → load()` 不变。

## 必跑验收

1. 候选原块与父文件固定逆变换机械比较。
2. `npm run build`。
3. `python3 -m py_compile scripts/test_exam_dictation_review_tab_ui.py` 与 `node --test tests/frontend/examPure.test.ts`。
4. R2 十组浏览器 UI 回归全部通过。
5. 66/66 批前非目标文件、完整 porcelain、Git index、HEAD 与 whitespace 保护；另单独核验目标父文件机械逆变换。

## 停止条件

- 机械比较不一致；
- 需要修改生产语义、现有测试或 CSS 才能通过；
- 父级统一加载刷新或模板/页面/workbench 调用消失，新组件出现目标六类之外的写命令；
- 非目标文件、暂存区或 HEAD 变化。

## 完成口径

全部验收通过后只能记为“R2 完整 `DictationReviewTab` 行为保持式生产外移自动化通过，未集成、未发布”。不得写成默写 OCR/评分准确、成绩自动发布、真实老师减负或 R2 整体重构完成。
