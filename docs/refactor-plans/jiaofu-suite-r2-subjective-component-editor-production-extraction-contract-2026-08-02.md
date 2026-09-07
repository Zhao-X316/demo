---
title: jiaofu-suite R2 SubjectiveComponentEditor production extraction contract
date: 2026-08-02
status: authorized_local_production_extraction
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_exam_sha256: cc1ef00fab75f5da0c4cfb01962e204d8ce12d07fc388e0338aebe1aa2b88f47
source_component_block_sha256: 863928a9a0e7713a1f17bff694edb1400e37e24bd324e0def42bf7d49d8fc7b1
source_characterization_sha256: 591059b40b038de29248dc87f702d58387fa5a3ed36501beae4cf4da71a5c128
source_test_contract_sha256: 0b21b4167faf7b91b3af8a51408de381f34575ca7e2ea0409380c9fc088b1967
scope: R2-SubjectiveComponentEditor-production-extraction
integration_authorized: false
release_authorized: false
---

# R2 `SubjectiveComponentEditor` 生产外移执行合同

## 目标

在 R2-E8 浏览器特征测试已通过的前提下，只把 `src/pages/Exam.tsx` 当前第 2374～2441 行老师“按空格/评分点逐项确认”编辑器机械外移到 `src/pages/exam/SubjectiveComponentEditor.tsx`。父级 `SubjectiveReviewTab` 继续创建并持有所有输入状态、解析组件规则、执行校验、调用 Tauri 和刷新工作台。

## 唯一允许的生产修改

1. 新建 `src/pages/exam/SubjectiveComponentEditor.tsx`：
   - 只定义当前组件规格、机器逐点结果和父级 setter/callback 的 TypeScript props；
   - 接收 `row / componentSpecs / pointResults / componentScores / componentEvidence / componentNotes / manualNotes / busy`；
   - 接收父级既有四个 setter 与 `correctComponents`，不得在子组件复制状态或校验；
   - 原样包含改前第 2374～2441 行，并仅增加无真实 DOM 的 fragment 外壳。
2. 修改 `src/pages/Exam.tsx`：
   - 增加唯一组件 import；
   - 用固定组件调用替换改前第 2374～2441 行；
   - 把既有状态、setter、已解析组件/逐点结果、busy 与 `correctComponents` 直接传入。

## 禁止范围

- 不移动或修改 `correctComponents` 的整体依据、规则完整性、分数范围、正分证据校验、DTO 映射、busy、成功/失败处理。
- 不移动 `shortAnswerPointResults`、`fillAnswerSlots`、`shortAnswerRubricPoints` 或 `componentSpecs / pointResults` 的解析时机。
- 不新增子组件 `useState`、effect、memo、Tauri/API 调用、真实 DOM 包装、默认值、数据转换或业务文案。
- 不修改 OCR 校正/重试、机器评分生成/接受、整题人工记分、答案/rubric 沉淀、链接、发布或已确认逐项结果展示。
- 不修改 API/DTO、Rust、Tauri command、SQL、迁移、依赖、CSS 或现有测试。
- 不暂存、不提交、不 tag、不合并、不推送、不发布。

## 机械不变量

1. 新组件去掉固定 import、类型、props、函数/return 与 fragment 外壳后，迁移 JSX 必须与改前第 2374～2441 行逐字节一致。
2. 改后父文件必须与“改前副本 + 唯一组件 import + 固定组件调用 - 原 68 行编辑器块”逐字节一致。
3. `SubjectiveComponentEditor` 定义 1 处、父级调用 1 处；父级六项状态及其 setter 定义、`correctComponents` 和 `examAnswerSheetSubjectiveCorrectComponents` 调用继续只在父级。
4. 子组件不得包含 `useState / useEffect / invoke / examAnswerSheet*`；当前全部 11 个主观题写调用继续留在父级。
5. R2-E8 特征测试和测试前置合同哈希不得变化。

## 必跑验收

1. 子组件迁移块与父文件固定变换机械比较。
2. `python3 -m py_compile scripts/test_exam_subjective_component_editor_ui.py`。
3. `node --test tests/frontend/examPure.test.ts`。
4. `npm run build`。
5. KnowledgeTab、QuestionTab、SubjectiveLinkPanel、GradeTab、SubjectiveAttemptPublishPanel、SubjectiveReviewToolbar、SubjectiveEvidenceSummary、SubjectiveComponentEditor 八组 UI 回归。
6. 非目标工作树哈希、完整 porcelain、Git index、HEAD 与 whitespace 保护。

## 停止条件

- 需要修改测试、父级校验、状态结构、默认值或 DTO 才能通过；
- 机械比较不能由唯一 import、固定组件调用和原 68 行删除解释；
- 子组件直接持有 Tauri/API 写命令或新增本地状态；
- 任一显示条件、展开规则、默认值、输入隔离、校验、提交参数或成功/失败语义变化；
- 非目标既有文件、暂存区或 HEAD 变化。

## 完成口径

只有所有验收通过，才允许记为“`SubjectiveComponentEditor` 行为保持式生产外移自动化通过，未集成”。不得写成主观题评分准确、老师终审自动化、真实老师减负或整个 `SubjectiveReviewTab` 已拆完。
