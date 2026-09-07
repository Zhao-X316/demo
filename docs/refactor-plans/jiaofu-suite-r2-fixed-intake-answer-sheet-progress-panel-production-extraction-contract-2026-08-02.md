---
title: jiaofu-suite R2 FixedIntake answer sheet progress panel production extraction contract
date: 2026-08-02
status: authorized_local_production_extraction
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_exam_sha256: 7e392749b1ea59de10b5aaf0b4df6744c487639bd8203cac7b962bedbd024bfd
candidate_sha256: efa7bf7a2030dea85a35b12171fd4e0c67c82bb439b4ddf0b576c7d80d3b38ad
scope: R2-fixed-intake-answer-sheet-progress-display
integration_authorized: false
release_authorized: false
---

# R2 固定上传答题卡识别面板生产外移合同

## 目标

在 R2-E12 固定上传共享壳特征测试保护下，仅把答题卡整套空白模板状态、模板候选证据、学生卡处理进度和显式选择/确认/继续/重试入口迁到独立受控展示组件。父页继续持有模板/page evidence、模板 run、学生卡结果、失败与 busy state，以及模板分析、模板确认、整套学生卡处理和错误处理 handler；本批不改变 OMR、手写 OCR、评分或终审语义。

## 候选边界取证

1. 候选是 `src/pages/Exam.tsx` 第 1269～1353 行，共 85 行，SHA-256 为 `efa7bf7a...b38ad`。
2. 候选只在页面质量已确认且材料类型为 `answer_sheet` 时展示整套模板缺口、模板分析证据、学生页处理统计和显式动作。
3. 候选使用父页 `answerSheetTemplateStatus / answerSheetTemplateRun / answerSheetEligiblePages / answerSheet*Count / processingAnswerSheetPageIds / groupingEvidence`，只回调父页的模板选择、模板确认和页面处理 handler。
4. 候选没有直接调用 Tauri、`examAnswerSheet*`、OMR、OCR 或评分 API，没有自己的 state/effect，也不生成老师分数、发布或学习证据。
5. R2-E12 浏览器测试已覆盖答题卡逐页空白模板、模板失败保留、模板可确认、整套处理、主客观统计、失败重试及终审路由。

## 施工快照

- `Exam.tsx`：1,475 行，SHA-256 `7e392749...bfd`。
- 候选：85 行，第 1269～1353 行，SHA-256 `efa7bf7a...b38ad`。
- 批前展开 porcelain：85 个路径；Git index SHA-256 `b54ac3d5...d4b`；staged diff 为空。
- 分支 `codex/t2-artifacts`，HEAD `6072360049b09c3155726773039da139834294fe`。

## 唯一允许修改

1. 新建 `src/pages/exam/FixedIntakeAnswerSheetProgressPanel.tsx`，承接候选 JSX、受控值和显式回调。
2. `src/pages/Exam.tsx` 只新增组件 import，并用显式 props 调用替换候选。
3. 所有 state、effect、模板/page evidence、模板分析/确认、页面处理、OMR/OCR、错误处理和三材料路由继续留在父页。
4. 完成后只回写对应验收、任务地图和修改记录。

## 必须冻结的当前行为

1. 仅 `qualityReviewCompleted && materialType === answer_sheet` 时显示面板。
2. 模板状态继续区分加载中、整套已确认和所缺页码；空白卡按页建立，不能把学生作答当模板。
3. 模板候选继续展示定位模式、印刷定位点或纸张边缘、客观格、主观区、可信度、状态和前四个 issue code。
4. 只有模板候选为 `ready` 才显示确认；模板 busy 或状态未加载时按钮继续禁用。
5. 模板整套 ready 后继续展示已处理页、清晰题区、需老师看、主观区已转写/需老师看、失败和待处理计数。
6. 只有存在待处理页面才显示继续识别；只有失败页才显示重试；busy 时继续禁用。
7. 选择、确认、继续和重试仍调用父页原 handler；组件不得自行选择文件、生成幂等键、确认模板或运行 OMR/OCR。
8. 本批不得触发老师终审、计分、发布、答案/题库版本切换或学习证据写入。

## 机械变换证明

1. 新组件 JSX 主体必须与候选 85 行逐字相同。
2. 将父页组件调用逆替换回候选并删除新 import 后，必须重建改前 `Exam.tsx` 的同一 SHA-256。
3. import 区只允许增加 `FixedIntakeAnswerSheetProgressPanel`。
4. 排除本合同和新组件后，批前 85 个展开路径不得发生非目标增删。

## 禁止范围

- 不修改模板分析/确认、学生卡处理、OMR、手写 OCR、重试条件、计数派生或错误处理。
- 不移动 state、effect、handler、业务 API/DTO、普通卷/默写处理、终审路由或样式。
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

只有所有验收通过，才允许记为“固定上传答题卡识别展示完成本地生产外移，行为等价、未集成”。不得写成真实 OMR/手写 OCR 准确、模板可靠、老师减负、整个 `FixedIntakeTab` 已重构完成或发布已验证。
