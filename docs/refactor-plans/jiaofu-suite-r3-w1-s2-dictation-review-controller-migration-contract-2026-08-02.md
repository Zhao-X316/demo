---
title: jiaofu-suite R3-W1-S2 dictation review controller migration
date: 2026-08-02
status: authorized_mechanical_production_migration
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_index_sha256: b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b
source_porcelain_count: 161
scope: R3-W1-S2-dictation-review-controller-migration
production_change_authorized: true
test_change_authorized: false
existing_audit_change_authorized: false
objective_change_authorized: false
subjective_change_authorized: false
release_authorized: false
---

# R3-W1-S2 默写终审台 Controller 机械迁移合同

## 唯一因素

只新建 `useDictationReviewController.ts`，并把 `DictationReviewTab.tsx` 中的 7 个 state、2 个 options 协调 effect、派生列表/计数、6 个 provider 命令、2 个 UUID 与 1 个发布确认框机械迁入该 hook。Tab 只保留纯显示 helper、原 DOM/文案和 JSX。

## 行为冻结

- 作业/题目 options、失效时回落首项、学号排序与四项统计不变；
- OCR 校正、识别重试、逐条接受、人工得分/备注/证据校验、0.95 严格批量和发布确认不变；
- provider 名称、参数、调用顺序、幂等键、成功/错误文案和 `onDone` 刷新语义不变；
- 机器原文、老师校正、标准答案、可接受写法、原图和 revision 边界不变；
- 当前单一 `busy` 行为不加固；raw setter 留到 W1-C1 单独改为语义 action。

## 路径边界

允许修改 `DictationReviewTab.tsx`、新增 `useDictationReviewController.ts`，以及本合同、回执和权威记录。禁止修改 Objective、Subjective、`Exam.tsx`、API、测试、审计器、依赖、Rust/SQL/DTO；不得创建共享 hook。

## 验收

1. Dictation Tab provider/hook/UUID/确认框为 0，恰好调用一次自己的 controller；6 个 provider 全部由 controller 独占；
2. W1 审计中 Dictation 结构项转绿，Objective 保持绿；总审计可因 Subjective 和语义 action 未完成而非零；
3. C2/C1/H1、65/65、build、Python、whitespace 和 27/27 UI 全绿；
4. Objective/Subjective、专项 UI、`Exam.tsx`、API、依赖与审计器哈希受保护；
5. 展开 porcelain 只允许 `161→163`，新增本合同与 hook；HEAD/index/staged 不变。

通过 S2 只放行 W1-S3 Subjective controller，不证明 W1、R3、真实 provider、老师减负、集成或发布完成。
