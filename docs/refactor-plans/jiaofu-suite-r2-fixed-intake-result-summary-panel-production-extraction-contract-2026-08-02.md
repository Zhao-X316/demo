---
title: jiaofu-suite R2 FixedIntake result summary panel production extraction contract
date: 2026-08-02
status: authorized_local_production_extraction
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_exam_sha256: a938a0a0d5cefcd2b0d90f0f57667f4799427183cba343ddb3f80b575083ffa5
label_candidate_sha256: 7c573cdd9f6149cbacf1b88761eda0f987cb80a2b5477f74e1c1fe3a03b254ce
jsx_candidate_sha256: f5db34ce6db96dfb1392266386f1a2e3df771483c8ab81e785f6799fffeb4ac2
scope: R2-fixed-intake-result-summary-display
integration_authorized: false
release_authorized: false
---

# R2 固定上传共享结果摘要面板生产外移合同

## 目标

在 R2-E12 固定上传共享壳特征测试保护下，仅把批次结果计数、原因码中文提示、下一步说明和显式“进入批改终审”入口迁到独立受控展示组件，并随同迁移仅由该展示使用的原因码标签表。父页继续持有 `FixedIntakeResult`、答题卡主观区计数、Tab state、数据刷新与 `onOpenReview` handler；本批不改变任何归组、识别、评分、终审或刷新语义。

## 候选边界取证

1. 标签候选是 `src/pages/Exam.tsx` 第 77～111 行，共 35 行，SHA-256 为 `7c573cdd...54ce`；全文搜索确认唯一消费者是本次结果面板。
2. JSX 候选是 `src/pages/Exam.tsx` 第 1311～1353 行，共 43 行，SHA-256 为 `f5db34ce...4ac2`。
3. 候选只读 `result` 和父页派生的 `answerSheetSubjectiveRegionCount`，只通过 `onOpenReview` 回调请求切换到 `objective / subjective / dictation`。
4. 候选没有直接调用 Tauri、`exam*` API、React state/effect、加载或写入命令，也不生成分数、发布或学习证据。
5. R2-E12 浏览器测试已覆盖结果路由门禁、普通卷→标准卷终审、纯客观答题卡→标准卷终审、含主观区答题卡→主观题终审、默写→默写复核，以及失败状态保留。

## 施工快照

- `Exam.tsx`：1,359 行，SHA-256 `a938a0a0...ffa5`。
- 标签候选：35 行，SHA-256 `7c573cdd...54ce`。
- JSX 候选：43 行，SHA-256 `f5db34ce...4ac2`。
- 批前展开 porcelain：89 个路径；Git index SHA-256 `b54ac3d5...d4b`；staged diff 为空。
- 分支 `codex/t2-artifacts`，HEAD `6072360049b09c3155726773039da139834294fe`。

## 唯一允许修改

1. 新建 `src/pages/exam/FixedIntakeResultSummaryPanel.tsx`，承接两个冻结候选、受控值和显式回调。
2. `src/pages/Exam.tsx` 只删除唯一私有 `INTAKE_REASON_LABEL`、新增组件 import，并用显式 props 调用替换 JSX 候选。
3. 所有 state、effect、结果生成、主观区计数派生、数据刷新、Tab 切换和三材料编排继续留在父页。
4. 完成后只回写对应验收、任务地图和修改记录。

## 必须冻结的当前行为

1. 三个计数块继续显示可批量确认、需复核和受阻；只有当前 route 对应项获得 active class，无目标时显示破折号。
2. `reasonCodes` 继续只展示前三项；顺序冲突和归组问题继续去重后只展示前四项；已知 code 使用现有中文，未知 code 原样显示。
3. 下一步继续展示后端返回的 `result.nextAction`，不得由组件猜测或改写。
4. 当批次/归组受阻、材料类型待确认、归组未确认或页面质量未确认时，“进入批改终审”继续禁用。
5. 默写继续进入 `dictation`；含主观区的答题卡进入 `subjective`；普通卷及无主观区答题卡进入 `objective`。
6. 点击只回调父页 `onOpenReview`；父页仍负责 `load()` 和实际 Tab state 切换。
7. 本批不得触发识别、老师终审、计分、发布、答案/题库版本切换或学习证据写入。

## 机械变换证明

1. 新组件原因码映射与 35 行标签候选逐字相同，JSX 主体与 43 行候选逐字相同。
2. 将父页组件调用逆替换回 JSX 候选、恢复标签候选并删除新 import 后，必须重建改前 `Exam.tsx` 的同一 SHA-256。
3. 排除本合同和新组件后，批前 89 个展开路径不得发生非目标增删。

## 禁止范围

- 不修改 result 生成、计数派生、原因码顺序/去重/截断、route 计算、button 门禁或目标 Tab 规则。
- 不移动 state、effect、数据加载、`onOpenReview` 外层刷新、业务 API/DTO、三材料处理或样式。
- 不修改现有测试、Rust、SQL、迁移或依赖。
- 不暂存、不提交、不 tag、不合并、不推送、不发布。

## 必跑验收

1. 两段候选逐字同源证明和父页逆重建证明。
2. `npx tsc --noEmit`、`node --test tests/frontend/examPure.test.ts`、`npm run build`。
3. 既有十一组 R2 UI、普通卷题库旁路 UI 和固定上传共享壳 UI，共十三组浏览器回归。
4. `git diff --check`、staged diff、展开 porcelain、Git index、HEAD 和批前非目标路径保护。

## 停止条件

- 需要修改 state、effect、handler、API/DTO、样式或现有测试才能完成；
- 新组件开始维护业务状态、调用 Tauri/API、执行加载或写入；
- 机械同源或逆重建证明失败；
- 任一既有测试失败，或非目标文件、暂存区、HEAD 变化。

## 完成口径

只有所有验收通过，才允许记为“固定上传共享结果摘要完成本地生产外移，行为等价、未集成”。不得写成路由业务已重构、真实批次可靠、老师减负、整个 `FixedIntakeTab` 已完成或发布已验证。
