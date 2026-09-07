---
title: jiaofu-suite R2 FixedIntake dictation progress panel production extraction contract
date: 2026-08-02
status: authorized_local_production_extraction
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_exam_sha256: 6c80e7fa6e20aa13f63cc21c5b298ff3763e551e187595bb145f545d1e1c10d9
candidate_sha256: 40c89bf123cb147c10e18c8922a81a97c453621715ad3a60c2aa1a8a41c36743
scope: R2-fixed-intake-dictation-progress-display
integration_authorized: false
release_authorized: false
---

# R2 固定上传默写识别面板生产外移合同

## 目标

在 R2-E12 固定上传共享壳特征测试保护下，仅把默写空白页模板状态、模板候选证据、学生默写页处理进度和显式选择/确认/继续/重试入口迁到独立受控展示组件。父页继续持有模板状态/run、学生页结果、失败与 busy state，以及模板分析、模板确认、整批默写处理和错误处理 handler；本批不改变手写 OCR、标准答案、评分或老师终审语义。

## 候选边界取证

1. 候选是 `src/pages/Exam.tsx` 第 1292～1364 行，共 73 行，SHA-256 为 `40c89bf...6743`。
2. 候选只在页面质量已确认且材料类型为 `dictation` 时展示模板缺口、模板分析证据、学生页处理统计和显式动作。
3. 候选使用父页 `dictationTemplateStatus / dictationTemplateRun / answerSheetEligiblePages / dictation*Count / processingDictationPageIds / groupingEvidence`，只回调父页的模板选择、模板确认和页面处理 handler。
4. 候选没有直接调用 Tauri、`examDictation*`、OCR 或评分 API，没有自己的 state/effect，也不生成老师分数、发布或学习证据。
5. R2-E12 浏览器测试已覆盖默写空白页、模板失败、模板可确认、处理中、失败重试和终审路由。

## 施工快照

- `Exam.tsx`：1,413 行，SHA-256 `6c80e7fa...10d9`。
- 候选：73 行，第 1292～1364 行，SHA-256 `40c89bf...6743`。
- 批前展开 porcelain：87 个路径；Git index SHA-256 `b54ac3d5...d4b`；staged diff 为空。
- 分支 `codex/t2-artifacts`，HEAD `6072360049b09c3155726773039da139834294fe`。

## 唯一允许修改

1. 新建 `src/pages/exam/FixedIntakeDictationProgressPanel.tsx`，承接候选 JSX、受控值和显式回调。
2. `src/pages/Exam.tsx` 只新增组件 import，并用显式 props 调用替换候选。
3. 所有 state、effect、模板/page evidence、模板分析/确认、页面处理、OCR、错误处理和三材料路由继续留在父页。
4. 完成后只回写对应验收、任务地图和修改记录。

## 必须冻结的当前行为

1. 仅 `qualityReviewCompleted && materialType === dictation` 时显示面板。
2. 模板状态继续区分加载中、已有 active revision 和还差空白页；空白页只建立书写框，不得当学生作答或标准答案。
3. 模板候选继续展示匹配题区数、可信度、状态和前四个 issue code。
4. 只有模板候选为 `ready` 才显示确认；模板 busy 或状态未加载时按钮继续禁用。
5. active 模板存在后继续展示已处理页、精确命中、需老师看、失败和待处理计数。
6. 只有存在待处理页面才显示继续识别；只有失败页才显示重试；处理进行中继续禁用。
7. 选择、确认、继续和重试仍调用父页原 handler；组件不得自行选择文件、生成幂等键、确认模板或运行 OCR。
8. 原始学生文字继续由父级处理链保留，不能用标准答案反向改写；本批不得触发老师终审、计分、发布、答案/题库版本切换或学习证据写入。

## 机械变换证明

1. 新组件 JSX 主体必须与候选 73 行逐字相同。
2. 将父页组件调用逆替换回候选并删除新 import 后，必须重建改前 `Exam.tsx` 的同一 SHA-256。
3. import 区只允许增加 `FixedIntakeDictationProgressPanel`。
4. 排除本合同和新组件后，批前 87 个展开路径不得发生非目标增删。

## 禁止范围

- 不修改模板分析/确认、学生页处理、手写 OCR、标准化、观察结果、重试条件、计数派生或错误处理。
- 不移动 state、effect、handler、业务 API/DTO、普通卷/答题卡处理、终审路由或样式。
- 不修改现有测试、Rust、SQL、迁移或依赖。
- 不暂存、不提交、不 tag、不合并、不推送、不发布。

## 必跑验收

1. 组件候选主体逐字同源证明和父页逆重建证明。
2. `npx tsc --noEmit`、`node --test tests/frontend/examPure.test.ts`、`npm run build`。
3. 既有十一组 R2 UI、普通卷题库旁路 UI 和固定上传共享壳 UI，共十三组浏览器回归。
4. `git diff --check`、staged diff、展开 porcelain、Git index、HEAD 和批前非目标路径保护。

## 停止条件

- 需要修改 state、effect、handler、API/DTO、样式或现有测试才能完成；
- 新组件开始维护业务状态、调用 Tauri/API、生成幂等键或自行恢复失败；
- 机械同源或逆重建证明失败；
- 任一既有测试失败，或非目标文件、暂存区、HEAD 变化。

## 完成口径

只有所有验收通过，才允许记为“固定上传默写识别展示完成本地生产外移，行为等价、未集成”。不得写成真实手写 OCR 准确、模板可靠、老师减负、整个 `FixedIntakeTab` 已重构完成或发布已验证。
