---
title: jiaofu-suite R3-W1-S3 subjective review controller migration
date: 2026-08-02
status: authorized_mechanical_production_migration
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_index_sha256: b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b
source_porcelain_count: 163
scope: R3-W1-S3-subjective-review-controller-migration
production_change_authorized: true
test_change_authorized: false
existing_audit_change_authorized: false
objective_change_authorized: false
dictation_change_authorized: false
subjective_editor_change_authorized: false
release_authorized: false
---

# R3-W1-S3 主观题终审台 Controller 机械迁移合同

## 唯一因素

只新建 `useSubjectiveReviewController.ts`，并把 `SubjectiveReviewTab.tsx` 中的 9 个 state、2 个 options 协调 effect、派生列表/计数、9 个异步动作、11 个 provider 调用点、4 个 UUID 与 3 个确认框机械迁入该 hook。Tab 继续保留结构化证据显示 helper、子面板组合与原 JSX。

## 行为冻结

- 作业/题目 options、失效回落、学号排序、三项统计与空态不变；
- OCR 校正后短答重新评分、识别重试后条件评分、手动生成建议的调用顺序不变；
- 单条接受、整题人工分、逐槽/逐点评分、证据必填与上限校验不变；
- 未来可接受答案/评分点示例晋级的确认框、版本边界、文案与历史不变原则不变；
- provider 名称、参数、幂等键、成功/错误文案、发布确认与 `onDone` 刷新语义不变；
- `SubjectiveComponentEditor` 的 raw setter props 暂不改，统一留到 W1-C1；当前单一 `busy` 不加固。

## 路径边界

允许修改 `SubjectiveReviewTab.tsx`、新增 `useSubjectiveReviewController.ts`，以及合同、回执和权威记录。禁止修改 Objective、Dictation、Subjective 子组件、`Exam.tsx`、API、测试、审计器、依赖、Rust/SQL/DTO；不得创建共享 hook。

## 验收

1. Subjective Tab provider/hook/UUID/确认框为 0，恰好调用一次自己的 controller；11 个 provider 调用点全部由 controller 独占；
2. 三台结构迁移项全部转绿；W1 总审计只允许因公共语义 action 与 editor raw setter 未完成而非零；
3. C2/C1/H1、65/65、build、Python、whitespace 和 27/27 UI 全绿；
4. Objective/Dictation、Subjective 子组件、专项 UI、`Exam.tsx`、API、依赖与审计器哈希受保护；
5. 展开 porcelain 只允许 `163→165`，新增本合同与 hook；HEAD/index/staged 不变。

通过 S3 只放行 W1-C1 语义 action 收口，不证明 W1、R3、真实 provider、老师减负、集成或发布完成。
