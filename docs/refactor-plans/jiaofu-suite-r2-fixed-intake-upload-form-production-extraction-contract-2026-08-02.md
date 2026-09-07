---
title: jiaofu-suite R2 FixedIntake upload form production extraction contract
date: 2026-08-02
status: authorized_local_production_extraction
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_exam_sha256: 21f953a71d67df35ab1fd9568d3a13d761efea8ad71527bef782ec3d93ad5f21
candidate_sha256: acd1e4a43cae769fcf3c1d56bf14b9c1aac88ce8571e36429cce07f55e337bf2
scope: R2-fixed-intake-upload-form-display
integration_authorized: false
release_authorized: false
---

# R2 固定上传共享表单生产外移合同

## 目标

在 R2-E12 固定上传共享壳特征测试保护下，仅把“上传后自动整理”三步表单迁到独立受控展示组件。父页继续持有班级/作业、学生文件、答案资料、页周期、幂等键、批次结果等 state，以及选文件、页周期推断、reset 和提交 handler；本批不改变上传、排序、答案权限或后续三材料处理语义。

## 候选边界取证

1. 候选是 `src/pages/Exam.tsx` 第 1167～1278 行，共 112 行，SHA-256 为 `acd1e4a4...7bf2`。
2. 候选只展示班级/作业选择、学生文件选择与预览、页周期高级输入、答案文件/粘贴/清除、上传按钮和安全说明；没有直接调用 Tauri/API。
3. 候选使用父页 `classOptions / assessmentOptions / studentPaths / pageCycle / answerPath / answerText / busy` 等值，并通过父页 setter、`resetRequest`、`pickStudentPapers`、`pickAnswer`、`submit` 完成交互。
4. `pickStudentPapers` 仍负责文件类型检查、原选择顺序保留、页周期推断和失败保留；`submit` 仍负责请求键、固定作业版本、答案候选和上传编排。
5. R2-E12 浏览器测试已覆盖空态、文件类型/顺序/预览、2 页周期、手动页数、答案文件/粘贴/清除、准备失败后输入保留与同幂等键重试。

## 施工快照

- `Exam.tsx`：1,702 行，SHA-256 `21f953a7...5f21`。
- 候选：112 行，第 1167～1278 行，SHA-256 `acd1e4a4...7bf2`。
- 批前展开 porcelain：77 个路径；Git index SHA-256 `0273c39f...6578`；staged diff 为空。
- 分支 `codex/t2-artifacts`，HEAD `6072360049b09c3155726773039da139834294fe`。

## 唯一允许修改

1. 新建 `src/pages/exam/FixedIntakeUploadForm.tsx`，承接候选 JSX、受控值、setter、显式回调和唯一所需纯展示 helper `fileName`。
2. `src/pages/Exam.tsx` 只新增组件 import、用显式 props 调用替换候选，并移除已无父页消费者的 `fileName` import。
3. 所有 state、effect、`resetRequest`、选文件/页周期推断/提交 handler、十一类共享业务调用和三材料处理继续留在父页。
4. 完成后只回写对应验收、任务地图和修改记录。

## 必须冻结的当前行为

1. 班级改变继续更新班级并 reset 当前请求；作业改变继续更新固定作业版本并 reset。
2. 学生文件仍只经父页选择流程进入，界面显示前四个文件名和剩余数量；组件不得排序、过滤或改写路径。
3. 自动页周期来源/置信度/人工确认文案不变；手动页数变化继续清除请求键与旧批次结果。
4. 答案文件继续支持既有类型并禁用粘贴框；无文件时可粘贴，变更继续 reset。
5. 清除答案继续同时清空文件和粘贴文本并 reset；答案资料仍为选填候选，不变成权威答案。
6. 上传按钮 busy 门禁、文案和 `submit` 调用不变；失败后父页继续保留输入和幂等键供重试。
7. 安全说明继续明确原文件保留、本机提取和不自动计分/发布。
8. 本批不得触发终审确认、成绩发布、题库/知识点写入或学习证据写入。

## 机械变换证明

1. 新组件 JSX 主体必须与候选 112 行逐字相同。
2. 将父页组件调用逆替换回候选并恢复 import 后，必须重建改前 `Exam.tsx` 的同一 SHA-256。
3. import 区只允许增加 `FixedIntakeUploadForm`、移除父页已无消费者的 `fileName`。
4. 排除本合同和新组件后，批前 77 个展开路径不得发生非目标增删。

## 禁止范围

- 不修改文件排序、扩展名校验、页周期推断、请求键、答案来源、批次准备或错误处理。
- 不移动 state、effect、handler、API/DTO、普通卷/答题卡/默写处理、终审路由或样式。
- 不修改现有测试、Rust、SQL、迁移或依赖。
- 不暂存、不提交、不 tag、不合并、不推送、不发布。

## 必跑验收

1. 组件候选主体逐字同源证明和父页逆重建证明。
2. `npx tsc --noEmit`、`node --test tests/frontend/examPure.test.ts`、`npm run build`。
3. 既有十一组 R2 UI、普通卷题库旁路 UI 和固定上传共享壳 UI，共十三组浏览器回归。
4. `git diff --check`、staged diff、展开 porcelain、Git index、HEAD 和批前非目标路径保护。

## 停止条件

- 需要修改 state、effect、handler、API/DTO、样式或现有测试才能完成；
- 新组件开始排序/过滤文件、生成幂等键、调用业务 API 或拥有批次状态；
- 机械同源或逆重建证明失败；
- 任一既有测试失败，或非目标文件、暂存区、index、HEAD 变化。

## 完成口径

只有所有验收通过，才允许记为“固定上传共享表单完成本地生产外移，行为等价、未集成”。不得写成真实拍照顺序可靠、OCR/AI 识别准确、老师减负、整个 `FixedIntakeTab` 已重构完成或发布已验证。
