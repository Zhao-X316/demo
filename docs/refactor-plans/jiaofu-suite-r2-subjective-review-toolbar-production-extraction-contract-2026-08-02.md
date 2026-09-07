---
title: jiaofu-suite R2 SubjectiveReviewToolbar production extraction contract
date: 2026-08-02
status: authorized_local_production_extraction
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_exam_sha256: d8e8223f5ca48da3724babb219d605d3358a9db0138ff2919648751f0e6cfc3f
source_characterization_sha256: afc5efad2ebe89efafef3ac04b0521e18fe94dbb5f8db2b9d07db3a19bf1d27c
scope: R2-SubjectiveReviewToolbar-production-extraction
integration_authorized: false
release_authorized: false
---

# R2 `SubjectiveReviewToolbar` 生产外移执行合同

## 目标

在 R2-E6 浏览器特征测试已通过的前提下，只把 `src/pages/Exam.tsx` 当前第 2294～2318 行顶部工具栏机械外移到 `src/pages/exam/SubjectiveReviewToolbar.tsx`。父级 `SubjectiveReviewTab` 继续持有选项派生、有效选择回退、行筛选/排序/统计和全部业务命令。

## 唯一允许的生产修改

1. 新建 `src/pages/exam/SubjectiveReviewToolbar.tsx`：
   - 仅类型导入 `SubjectiveWorkbenchRow`；
   - 接收 `assessmentVersions / assessmentVersionId / onAssessmentVersionChange / itemOptions / assessmentItemId / onAssessmentItemChange / rowCount / confirmedCount / directCount / exceptionCount`；
   - 原样包含改前第 2294～2318 行，只把两个父级 setter 名机械替换为回调名，并把 `rows.length` 替换为已算好的 `rowCount`。
2. 修改 `src/pages/Exam.tsx`：
   - 增加唯一组件 import；
   - 用固定组件调用替换改前第 2294～2318 行；
   - 传入父级已有数组、当前值、setter 和四项已算结果。

## 禁止范围

- 不移动或修改 `assessmentVersions`、`itemOptions`、两个有效选择 `useEffect`、`rows`、`attempts` 或四项统计计算。
- 不新增子组件状态、effect、memo、Tauri 调用、真实 DOM 包装、字段转换或默认值。
- 不修改题目排序、学生数字学号排序、统计公式、选项文案、固定说明、class 或 CSS。
- 不修改 OCR、评分、逐项/整题终审、答案/rubric 沉淀、链接、发布事务或其他 Tab。
- 不修改 API/DTO、Rust、Tauri command、SQL、迁移、依赖或测试。
- 不暂存、不提交、不 tag、不合并、不推送、不发布。

## 机械不变量

1. 新组件去掉固定 import/函数/props 外壳，把两个 `on...Change` 名还原为父级 setter，并把 `rowCount` 还原为 `rows.length` 后，工具栏 JSX 必须与改前第 2294～2318 行逐字节一致。
2. 改后父文件必须与“改前副本 + 唯一组件 import + 固定组件调用 - 改前第 2294～2318 行”逐字节一致。
3. 新组件定义 1 处、父级调用 1 处；两个 setter、两个回退 effect、四项统计定义和 9 条主观题写命令均继续只在父级。
4. R2-E6 测试脚本和测试前置合同哈希不得变化。

## 必跑验收

1. 子组件内部与父文件机械比较。
2. `node --test tests/frontend/examPure.test.ts`。
3. `npm run build`。
4. KnowledgeTab、QuestionTab、SubjectiveLinkPanel、GradeTab、SubjectiveAttemptPublishPanel 与 R2-E6 工具栏六组 UI 回归。
5. 非目标工作树哈希、完整展开 porcelain、Git index、HEAD 与 whitespace 保护。

## 停止条件

- 需要移动选项/统计派生、回退 effect 或业务命令；
- 机械比较不能由唯一 import、固定替换、两个回调改名和 `rowCount` 代换解释；
- 需要修改 R2-E6 或任何现有测试；
- 任一默认选择、选项顺序、统计、学生排序、跨作业隔离或写命令行为变化；
- 非目标既有文件、暂存区或 HEAD 变化。

## 完成口径

只有所有验收通过，才允许记为“`SubjectiveReviewToolbar` 行为保持式生产外移自动化通过，未集成”。不得写成父级状态已重构、主观题 Tab 已拆完或老师已减负。
