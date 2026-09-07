---
title: jiaofu-suite R2 SubjectiveLinkPanel production extraction contract
date: 2026-08-02
status: authorized_local_production_extraction
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_exam_sha256: ad1f1bb6fba6170954c73d42b061f9fa00c59456045191223a702a2335ccd8c9
source_characterization_sha256: bee826c768419b41fd0c17105a7f8efef73944d3034e8cb413548e890a1d97af
scope: R2-SubjectiveLinkPanel-production-extraction
integration_authorized: false
release_authorized: false
---

# R2 `SubjectiveLinkPanel` 生产外移执行合同

## 目标

在 R2-E3 浏览器特征测试已通过的前提下，只把 `src/pages/Exam.tsx` 中的两个链接展示常量和 `SubjectiveLinkPanel` 组件机械外移到 `src/pages/exam/SubjectiveLinkPanel.tsx`，使主观题知识/能力链接编辑器拥有独立文件所有权。

## 唯一允许的生产修改

1. 新建 `src/pages/exam/SubjectiveLinkPanel.tsx`：
   - 从 React 导入 `useEffect`、`useState`；
   - 从 `../../api/exam` 导入 `examSubjectiveLinkEditor`、`examSubjectiveLinkSave`；
   - 从 `../../api/exam` 类型导入 `SubjectiveLinkEditor`、`SubjectiveSourceLinkInput`；
   - 原样包含改前 `KNOWLEDGE_RELATIONS`、`ABILITY_RESPONSE_MODES` 和 `SubjectiveLinkPanel`，只给组件增加 `export`。
2. 修改 `src/pages/Exam.tsx`：
   - 增加唯一 `SubjectiveLinkPanel` 组件 import；
   - 从原 API import 列表删除只由该组件使用的两种类型和两条命令；
   - 删除改前固定第 1983～2200 行，即两个常量和内联组件以及其尾部分隔空行。

## 禁止范围

- 不修改组件 props、初始状态、`useEffect` 依赖、加载/保存时序、默认链接值、确认文案、错误处理、成功提示或 CSS class。
- 不修改 `SubjectiveReviewTab` 的题目选择、OCR、评分点、终审、rubric 更新、发布或证据状态。
- 不修改 API/DTO、Rust、Tauri command、SQL、迁移、依赖、CSS 或测试夹具。
- 不暂存、不提交、不 tag、不合并、不推送、不发布。

## 机械不变量

1. 新组件去掉固定 imports，并把 `export function` 还原为 `function` 后，必须与改前第 1983～2199 行逐字节一致。
2. 改后父文件必须与“改前副本 + 唯一组件 import - 四个失效 API imports - 固定第 1983～2200 行”逐字节一致。
3. `SubjectiveLinkPanel` 定义只剩新模块 1 处，父级调用仍为 1 处。
4. `KNOWLEDGE_RELATIONS`、`ABILITY_RESPONSE_MODES`、两种类型和两条命令不得在 `Exam.tsx` 残留。
5. R2-E3 测试脚本哈希不得变化。

## 必跑验收

1. 组件与父文件机械比较。
2. `node --test tests/frontend/examPure.test.ts`。
3. `npm run build`。
4. `python3 scripts/test_exam_knowledge_tab_ui.py`。
5. `python3 scripts/test_exam_question_tab_ui.py`。
6. `python3 scripts/test_exam_subjective_link_panel_ui.py`。
7. 非目标工作树哈希、porcelain、Git index 与 whitespace 保护。

## 停止条件

- 机械比较不能由唯一 import/固定删除解释；
- 需要更改测试才能通过；
- 任一现有 UI 文案、命令参数或调用次数发生变化；
- 非目标既有脏文件或暂存区变化。

命中任一条件即停止，本合同不自动扩大为行为修复、状态重构或整 Tab 外移。

## 完成口径

只有所有验收通过，才允许记为“`SubjectiveLinkPanel` 行为保持式生产外移自动化通过，未集成”。不得写成主观题终审已重构、老师已减负或可发布。
