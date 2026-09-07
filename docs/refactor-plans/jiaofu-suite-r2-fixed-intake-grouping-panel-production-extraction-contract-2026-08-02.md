---
title: jiaofu-suite R2 FixedIntake grouping panel production extraction contract
date: 2026-08-02
status: authorized_local_production_extraction
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_exam_sha256: b34e69c43316d5c773e76df9bf18607781505b60bb88a999ab175e6abc723db8
candidate_sha256: 7fd50a1ce663fef84345edf3bc77f0e2e716f5bb4ad75f5a16e89047e574e5c2
scope: R2-fixed-intake-material-and-grouping-display
integration_authorized: false
release_authorized: false
---

# R2 固定上传材料类型与学生归组面板生产外移合同

## 目标

在 R2-E12 固定上传共享壳特征测试保护下，仅把材料类型一次确认和未确认学生顺序/缺交选择迁到独立受控展示组件。父页继续持有材料类型、起始学号、缺交学生、busy state，以及材料确认、缺交切换和原子归组 handler；本批不改变照片排序、学生归属或三材料识别语义。

## 候选边界取证

1. 候选是 `src/pages/Exam.tsx` 第 1228～1277 行，共 50 行，SHA-256 为 `7fd50a1c...e5c2`。
2. 候选只在材料类型待确认时显示普通试卷/答题卡/默写三个入口；材料已确认且归组可继续时，显示起始学生、缺交学生和一次确认按钮。
3. 候选使用父页 `result / confirmingType / groupingStartNo / absentStudentNos / groupingAbsenceCandidates / confirmingGrouping`，并只通过父页 setter/callback 选择起点、清空缺交、切换缺交和确认。
4. 候选没有直接调用 Tauri、`examFixedIntake*` 或其他业务 API，没有自己的 state/effect，也不触发识别、评分或发布。
5. R2-E12 浏览器测试已覆盖材料类型失败保留、普通卷/答题卡/默写精确参数、起始学生选择、缺交筛选、归组失败保留和成功后联系表。

## 施工快照

- `Exam.tsx`：1,542 行，SHA-256 `b34e69c4...3db8`。
- 候选：50 行，第 1228～1277 行，SHA-256 `7fd50a1c...e5c2`。
- 批前展开 porcelain：81 个路径；Git index SHA-256 `b54ac3d5...d4b`；staged diff 为空。
- 分支 `codex/t2-artifacts`，HEAD `6072360049b09c3155726773039da139834294fe`。

## 唯一允许修改

1. 新建 `src/pages/exam/FixedIntakeGroupingPanel.tsx`，承接候选 JSX、受控值和显式回调。
2. `src/pages/Exam.tsx` 只新增组件 import，并用显式 props 调用替换候选。
3. 所有 state、effect、材料确认、缺交切换、归组事务、证据加载、十一类共享业务调用和三材料处理继续留在父页。
4. 完成后只回写对应验收、任务地图和修改记录。

## 必须冻结的当前行为

1. 材料类型置信度不足时只显示一次确认；三个按钮继续精确传递 `ordinary_paper / answer_sheet / dictation`。
2. 材料确认 busy 时三个按钮继续同时禁用；失败仍保留本批结果和重试入口。
3. 材料类型确认后，只有 `groupingRoute !== blocked` 且尚未确认归组时显示归组面板。
4. 起始学生继续按 roster 顺序列出；改变起点时继续清空已勾选缺交学生。
5. 缺交候选继续只包含起始学生之后的学生；勾选/取消行为和摘要文案不变。
6. 起始学生为空或归组 busy 时继续禁用确认按钮；失败保留老师选择，成功后由父页重置下游状态并进入联系表。
7. 本批不得触发页面质量确认、识别、老师终审、计分、发布、题库或学习证据写入。

## 机械变换证明

1. 新组件 JSX 主体必须与候选 50 行逐字相同。
2. 将父页组件调用逆替换回候选并删除新 import 后，必须重建改前 `Exam.tsx` 的同一 SHA-256。
3. import 区只允许增加 `FixedIntakeGroupingPanel`。
4. 排除本合同和新组件后，批前 81 个展开路径不得发生非目标增删。

## 禁止范围

- 不修改照片顺序、材料类型、roster、缺交集合、归组 DTO、归组事务或错误处理。
- 不移动 state、effect、handler、业务 API/DTO、页面质量、普通卷/答题卡/默写处理、终审路由或样式。
- 不修改现有测试、Rust、SQL、迁移或依赖。
- 不暂存、不提交、不 tag、不合并、不推送、不发布。

## 必跑验收

1. 组件候选主体逐字同源证明和父页逆重建证明。
2. `npx tsc --noEmit`、`node --test tests/frontend/examPure.test.ts`、`npm run build`。
3. 既有十一组 R2 UI、普通卷题库旁路 UI 和固定上传共享壳 UI，共十三组浏览器回归。
4. `git diff --check`、staged diff、展开 porcelain、Git index、HEAD 和批前非目标路径保护。

## 停止条件

- 需要修改 state、effect、handler、API/DTO、样式或现有测试才能完成；
- 新组件开始维护业务状态、调用 Tauri/API 或自行恢复失败；
- 机械同源或逆重建证明失败；
- 任一既有测试失败，或非目标文件、暂存区、HEAD 变化。

## 完成口径

只有所有验收通过，才允许记为“固定上传材料类型与学生归组展示完成本地生产外移，行为等价、未集成”。不得写成真实照片排序准确、学生归属可靠、老师减负、整个 `FixedIntakeTab` 已重构完成或发布已验证。
