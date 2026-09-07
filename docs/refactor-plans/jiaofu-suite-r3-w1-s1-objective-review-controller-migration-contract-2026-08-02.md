---
title: jiaofu-suite R3-W1-S1 objective review controller migration
date: 2026-08-02
status: authorized_mechanical_production_migration
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_index_sha256: b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b
source_porcelain_count: 159
scope: R3-W1-S1-objective-review-controller-migration
production_change_authorized: true
test_change_authorized: false
existing_audit_change_authorized: false
dictation_change_authorized: false
subjective_change_authorized: false
release_authorized: false
---

# R3-W1-S1 客观题终审台 Controller 机械迁移合同

## 唯一因素

只新建 `useObjectiveReviewController.ts`，并把 `ObjectiveReviewTab.tsx` 中的 5 个 state、2 个 options 协调 effect、派生列表/计数、5 个 provider 命令、2 个 UUID 与 1 个发布确认框机械迁入该 hook。Tab 只保留纯显示 helper、原 DOM/文案和 JSX。

## 行为冻结

- 作业/题目 options、失效时回落首项和仍合法时保留选择的语义不变；
- 学号排序、统计、人工得分/证据校验、0.95 严格批量边界不变；
- provider 名称、参数、调用顺序、幂等键、成功/错误文案和 `onDone` 刷新语义不变；
- OMR 重试与整卷发布确认框不变；老师终审、显式发布和历史 revision 边界不变；
- 当前单一 `busy` 行为不加固；不顺手改语义 action 名称，raw setter 在 W1-C1 单独收口。

## 路径边界

允许修改：

- `src/pages/exam/ObjectiveReviewTab.tsx`
- `src/pages/exam/useObjectiveReviewController.ts`（新增）
- 本合同、仓库外回执及权威记录。

禁止修改 Dictation、Subjective、`Exam.tsx`、API、测试、现有审计器、依赖、Rust/SQL/DTO。不得创建跨工作台共享 hook。

## 验收

1. Objective Tab provider/hook/UUID/确认框为 0，恰好调用一次自己的 controller；5 个 provider 调用全部由 controller 独占；
2. W1 总审计仍可因另两台与语义动作未完成而非零，但 Objective 的结构迁移项必须转绿；
3. C2 10/10、C1 19/19、H1 17/17/26/26、65/65、build、Python、whitespace 和 27/27 UI 继续通过；
4. Dictation/Subjective/三个专项 UI、`Exam.tsx`、API、依赖和既有审计器哈希不变；
5. 展开 porcelain 只允许 `159→161`，新增本合同与 hook；HEAD/index/staged 不变。

通过 S1 只放行 W1-S2 Dictation controller，不证明 W1、R3、真实 provider、老师减负、集成或发布完成。
