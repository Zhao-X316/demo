---
title: jiaofu-suite R2 FixedIntake ordinary progress panel production extraction contract
date: 2026-08-02
status: authorized_local_production_extraction
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_exam_sha256: ec5b5f739e21e4c00e3b12bbec4d7048d7e7f96c27ee84a7e056eaf6a2e848b5
candidate_sha256: 3bc1c1d40045cd9436e4f29f132cd8e4cd2646443452a708296ed1ffc41e0f42
scope: R2-fixed-intake-ordinary-progress-display
integration_authorized: false
release_authorized: false
---

# R2 固定上传普通卷识别进度面板生产外移合同

## 目标

在 R2-E12 固定上传共享壳和普通卷题库旁路特征测试保护下，仅把普通试卷页面分析进度、异常摘要和显式确认/继续/重试入口迁到独立受控展示组件。父页继续持有页面证据、分析 run、结构确认、busy state，以及页面分析、结构确认、题库沉淀和题区识别 handler；本批不改变识别、评分或题库语义。

## 候选边界取证

1. 候选是 `src/pages/Exam.tsx` 第 1253～1299 行，共 47 行，SHA-256 为 `3bc1c1d4...0f42`。
2. 候选只在页面质量已确认且材料类型为 `ordinary_paper` 时显示处理总数、五类状态计数、前三条安全失败信息和三个显式动作。
3. 候选使用父页 `ordinaryRunValues / ordinaryEligiblePageCount / ordinary*Count / analyzingPageIds / confirmingOrdinaryPageIds / groupingEvidence`，并只通过两个父页 handler 继续、确认或重试。
4. 候选没有直接调用 Tauri、`examOrdinary*`、题库或其他业务 API，没有自己的 state/effect，也不生成分数或发布。
5. 既有浏览器测试已覆盖普通卷页面分析、可确认页面显式进入批改、题库旁路失败不阻断、失败重试和终审路由。

## 施工快照

- `Exam.tsx`：1,506 行，SHA-256 `ec5b5f73...48b5`。
- 候选：47 行，第 1253～1299 行，SHA-256 `3bc1c1d4...0f42`。
- 批前展开 porcelain：83 个路径；Git index SHA-256 `b54ac3d5...d4b`；staged diff 为空。
- 分支 `codex/t2-artifacts`，HEAD `6072360049b09c3155726773039da139834294fe`。

## 唯一允许修改

1. 新建 `src/pages/exam/FixedIntakeOrdinaryProgressPanel.tsx`，承接候选 JSX、受控值和显式回调。
2. `src/pages/Exam.tsx` 只新增组件 import，并用显式 props 调用替换候选。
3. 所有 state、effect、页面证据、分析/确认 run、题库旁路、题区识别、错误处理和三材料路由继续留在父页。
4. 完成后只回写对应验收、任务地图和修改记录。

## 必须冻结的当前行为

1. 仅 `qualityReviewCompleted && materialType === ordinary_paper` 时显示面板。
2. 处理文案继续区分正在处理与已处理页数；可确认、已确认、需复核、受阻和待处理计数不变。
3. 失败摘要继续只展示前三条后端安全信息，不暴露原始请求或凭据。
4. 只有存在可确认页面时显示确认入口，分析或确认 busy 时继续禁用。
5. 只有存在待处理页面时显示继续识别入口；只有可重试失败时显示重试入口。
6. 普通继续、确认和失败重试仍调用父页原 handler；组件不得自行选择页面、生成幂等键、同步题库或识别题区。
7. 题库沉淀失败继续是可恢复旁路，不阻断当前学生作业识别和批改。
8. 本批不得触发老师终审、计分、发布、题库版本切换或学习证据写入。

## 机械变换证明

1. 新组件 JSX 主体必须与候选 47 行逐字相同。
2. 将父页组件调用逆替换回候选并删除新 import 后，必须重建改前 `Exam.tsx` 的同一 SHA-256。
3. import 区只允许增加 `FixedIntakeOrdinaryProgressPanel`。
4. 排除本合同和新组件后，批前 83 个展开路径不得发生非目标增删。

## 禁止范围

- 不修改页面分析、结构确认、题库旁路、题区识别、重试条件、计数派生或错误处理。
- 不移动 state、effect、handler、业务 API/DTO、答题卡/默写处理、终审路由或样式。
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

只有所有验收通过，才允许记为“固定上传普通卷识别进度展示完成本地生产外移，行为等价、未集成”。不得写成真实 OCR/版面识别准确、题库沉淀可靠、老师减负、整个 `FixedIntakeTab` 已重构完成或发布已验证。
