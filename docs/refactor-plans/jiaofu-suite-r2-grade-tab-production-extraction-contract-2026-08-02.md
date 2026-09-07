---
title: jiaofu-suite R2 GradeTab production extraction contract
date: 2026-08-02
status: authorized_local_production_extraction
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_exam_sha256: 581df186e452f5cc810aec14e5ddab3074dd52a396d0f9a91feffc02d812f3f9
source_characterization_sha256: c2928ca6e86935e603ee1b23e2313138425828f3c75eb3b3d2ca9b9c3b683cd7
scope: R2-GradeTab-production-extraction
integration_authorized: false
release_authorized: false
---

# R2 `GradeTab` 生产外移执行合同

## 目标

在 R2-E4 浏览器特征测试已通过的前提下，只把 `src/pages/Exam.tsx` 中的 `GradeTab` 机械外移到 `src/pages/exam/GradeTab.tsx`，使“老师补录”拥有独立文件所有权。

## 唯一允许的生产修改

1. 新建 `src/pages/exam/GradeTab.tsx`：
   - 从 React 导入 `useState`；
   - 从 `../../api/exam` 导入 `examAnswerHumanDecide`、`examAnswerSuggest`；
   - 类型导入 `AnswerDetail`、`Question`；
   - 从 `../../api/manage` 类型导入 `Student`；
   - 从 `./examPure` 导入 `displayTime`，从 `./questionTypes` 导入 `TYPE_LABEL`；
   - 原样包含改前 `GradeTab`，只增加 `export`。
2. 修改 `src/pages/Exam.tsx`：
   - 增加唯一 `GradeTab` 组件 import；
   - 删除只由该组件使用的 `examAnswerHumanDecide`、`examAnswerSuggest`、`displayTime` 和父级 `TYPE_LABEL` import；
   - 删除改前固定第 3197～3363 行（组件前分隔空行与内联组件）；组件正文仍固定为第 3198～3363 行。

## 禁止范围

- 不修改 props、客观题筛选、初始表单状态、机器建议参数、备注 trim、四种老师决定提示、错误处理、成功刷新或 CSS class。
- 不修改 API/DTO、Rust 事务、Tauri command、SQL、迁移、依赖、CSS 或任何测试。
- 不修改 `SubjectiveReviewTab`、`DictationReviewTab`、`ObjectiveReviewTab` 或三材料上传。
- 不暂存、不提交、不 tag、不合并、不推送、不发布。

## 机械不变量

1. 新组件去掉固定 imports，并把 `export function` 还原为 `function` 后，必须与改前第 3198～3363 行逐字节一致。
2. 改后父文件必须与“改前副本 + 唯一组件 import - 四个失效 imports - 固定第 3197～3363 行”逐字节一致。
3. `GradeTab` 定义只剩新模块 1 处，父级调用仍为 1 处。
4. 两条命令、`displayTime` 调用和 `TYPE_LABEL` 引用不得在 `Exam.tsx` 残留；`QuestionTab` 自身的 `TYPE_LABEL` import 不变。
5. R2-E4 测试脚本哈希不得变化。

## 必跑验收

1. 组件与父文件机械比较。
2. `node --test tests/frontend/examPure.test.ts`。
3. `npm run build`。
4. `KnowledgeTab`、`QuestionTab`、`SubjectiveLinkPanel`、`GradeTab` 四组 UI 回归。
5. 非目标工作树哈希、porcelain、Git index 与 whitespace 保护。

## 停止条件

- 机械比较不能由唯一 import/固定删除解释；
- 需要修改 R2-E4 测试或任何现有测试；
- 任一机器建议/老师终审文案、参数、调用次数或刷新行为变化；
- 非目标既有脏文件或暂存区变化。

## 合同边界纠正

首次父文件机械比较在启动构建前停止。只发现改前第 3197 行组件前分隔空行随内联组件一并删除；imports、组件正文和其余父文件均一致。故在不改动生产代码的前提下，将父文件固定删除边界由第 3198～3363 行纠正为第 3197～3363 行；新组件正文比较边界保持第 3198～3363 行不变。

## 完成口径

只有所有验收通过，才允许记为“`GradeTab` 行为保持式生产外移自动化通过，未集成”。不得写成评分逻辑已优化、老师已减负或可发布。
