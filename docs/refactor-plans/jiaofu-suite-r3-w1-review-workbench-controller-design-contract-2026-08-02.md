---
title: jiaofu-suite R3-W1 review workbench controller design
date: 2026-08-02
status: design_complete_test_enablement_only_next
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_index_sha256: b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b
source_porcelain_count: 156
scope: R3-W1-review-workbench-controller-design
production_change_authorized: false
test_change_authorized: false
new_audit_authorized_next: true
r3_stage_closeout_authorized: false
r4_authorized: false
r5_authorized: false
release_authorized: false
---

# R3-W1 三个终审工作台 Controller 收口设计合同

## 1. 结论

客观题、默写和主观题三个终审 Tab 都不是“短且纯展示、无需迁移”的例外。它们分别直接持有 5/6/11 个 provider 调用点、5/7/9 个本地状态、2 个 options 协调 effect 和 5/6/9 个异步业务动作；主观题还直接向子组件传递 4 个 React state setter。三者均须各自迁入独立 controller，不能直接进入 R3-A。

三者的评分证据、校正、批量终审和发布语义不同，当前不建立万能 workbench hook。只允许在三个独立 controller 全部稳定后再次盘点完全相同的 scope 选择逻辑；没有充分证据就保留少量重复。

## 2. 源库存

| 工作台 | 行数 | state | effect | 异步动作 | provider 调用点 | 结论 |
|---|---:|---:|---:|---:|---:|---|
| `ObjectiveReviewTab.tsx` | 328 | 5 | 2 | 5 | 5 | 迁移独立 objective controller |
| `DictationReviewTab.tsx` | 337 | 7 | 2 | 6 | 6 | 迁移独立 dictation controller |
| `SubjectiveReviewTab.tsx` | 570 | 9 | 2 | 9 | 11 | 迁移独立 subjective controller |

### 源哈希

- Objective Tab：`709b3689e9fc46134b866ac82554784a0f5441c746647171842c006fade9f56c`
- Dictation Tab：`e6c102e4ff5ca77bb2177074ccdceb987a2a3dec857cbb7aa3e59c44c0e82551`
- Subjective Tab：`97bdf611acca0668b099bfdb9032259408c66c56a6c837d7dcf48f60f010c024`
- Subjective toolbar/editor/publish：`9bcba435...a44e` / `70dcd0fb...e98c` / `cc34532d...c47`
- 三个专项 UI：`073b9a91...a721` / `47c6a372...62b1` / `4256704a...a10c`
- `Exam.tsx` / Exam API：`ed4132ca...e9bc` / `1a796762...c164`
- package / lock：`a3f0afa7...69b0` / `8f627a00...adf4`

## 3. Provider 库存

### Objective：5/5

`examObjectiveAccept`、`examObjectiveCorrect`、`examObjectiveStrictBatchAccept`、`examObjectiveRecognizeRegion`、`examObjectivePublishAttempt` 各 1 个调用点。

### Dictation：6/6

`examDictationCorrectTranscription`、`examDictationRecognizeRegion`、`examDictationAccept`、`examDictationCorrectGrade`、`examDictationStrictBatchAccept`、`examDictationPublishAttempt` 各 1 个调用点。

### Subjective：9 个命令 / 11 个调用点

`examAnswerSheetGradeShortAnswer` 有 3 个调用点；`examAnswerSheetCorrectSubjectiveTranscription`、`examAnswerSheetRecognizeSubjectiveRegion`、`examAnswerSheetSubjectiveAccept`、`examAnswerSheetSubjectiveCorrect`、`examAnswerSheetSubjectiveCorrectComponents`、`examAnswerSheetPromoteAcceptedAnswer`、`examAnswerSheetPromoteRubricEvidence`、`examAnswerSheetSubjectivePublishAttempt` 各 1 个调用点。

以上名称、数量、参数、相对顺序、幂等键、确认框、成功/错误文案和 `onDone→Exam.load()` 刷新语义必须冻结。

## 4. 目标结构

```text
ObjectiveReviewTab.tsx
  └── useObjectiveReviewController.ts

DictationReviewTab.tsx
  └── useDictationReviewController.ts

SubjectiveReviewTab.tsx
  ├── useSubjectiveReviewController.ts
  ├── SubjectiveReviewToolbar.tsx
  ├── SubjectiveComponentEditor.tsx
  ├── SubjectiveEvidenceSummary.tsx
  ├── SubjectiveLinkPanel.tsx
  └── SubjectiveAttemptPublishPanel.tsx
```

每个 hook 只拥有该工作台的 scope/draft/busy、派生读模型和业务动作。Tab 只保留展示常量/纯格式化、结构化证据解析与 JSX；不得直接 import provider、`useState` 或 `useEffect`。三个 controller 不互相 import，`Exam.tsx` 继续只传 workbench/onDone/onError。

## 5. 语义动作终态

### 公共 scope/draft

- `selectAssessment`
- `selectItem`
- `changeCorrection`
- `changeManualScore`
- `changeManualNote`
- `changeManualEvidence`
- `changeComponentScore`
- `changeComponentEvidence`
- `changeComponentNote`

只在对应工作台实际需要的动作中出现。不得向 Tab 或 `SubjectiveComponentEditor` 暴露 React `Dispatch<SetStateAction<...>>`。

### 业务命令

命令继续使用当前业务名称或更明确的同义名；不能统一成 `submit/confirm/process`。老师终审、人工校正、严格批量、重新识别、未来答案/rubric 晋级和发布必须保持可区分。

## 6. 行为保持矩阵

- 父级等值刷新时保留仍合法的作业/题目选择；失效时才回到首个合法项。
- 所有草稿默认值、输入校验、满分边界、证据/备注必填规则不变。
- 单条接受、人工 revision、严格批量的排除条件和 0.95 阈值不变。
- OCR/OMR 重试、主观题转写后自动重跑逐点评分的调用顺序不变。
- 答案和 rubric 晋级继续二次确认且只影响未来版本，不改当前/历史成绩。
- 发布只采用当前老师终审 revision；终审完成不等于发布。
- 原图、机器原文、老师校正、逐项证据和现有 DOM/文案不变。
- 当前单一 `busy` 布尔状态和同 tick 行为先按现状冻结；若要增加同步防重入或 stale completion 门禁，必须另立 W1-H1 行为变更合同，不得夹带进 controller 搬迁。

## 7. 分批路线

| 批次 | 唯一因素 | 生产代码 |
|---|---|---|
| W1-D | 本设计与只读库存 | 不改 |
| W1-E1 | 新增 workbench controller AST 审计器 | 不改 |
| W1-S1 | Objective 状态/派生/五个命令迁入独立 controller | 只改 Objective Tab + 新 hook |
| W1-S2 | Dictation 状态/派生/六个命令迁入独立 controller | 只改 Dictation Tab + 新 hook |
| W1-S3 | Subjective 状态/派生/九个命令迁入独立 controller | 只改 Subjective Tab + 新 hook |
| W1-C1 | 三个 Tab 与 Subjective editor 的 raw setter 改为语义动作 | 只改 controllers/Tabs/editor 的接口边界 |
| W1-V1 | 独立只读冻结哈希与全量回归 | 不改 |
| R3-A | R3 阶段收口审计 | 不改 |

各 controller 必须先独立完成，不在 S1～S3 提取共享 hook。W1-V1 前重新盘点重复；除非三份实现存在稳定且相同的状态/事件合同，否则记录“无需共享”，不为消除少量重复再加抽象。

## 8. 每批门禁

1. W1 AST 审计器报告三工作台库存与迁移进度；
2. C2 10/10、C1 19/19、H1 17/17/26/26 保持绿；
3. 65/65 直接测试与 TypeScript/Vite build；
4. 三个工作台专项和当前全部 27 组 UI；
5. Python 编译、`git diff --check`；
6. provider 命令/调用点/规范 hash、API、`Exam.tsx`、依赖和非目标工作台哈希保护；
7. HEAD、Git index、staged 和扩展 porcelain 保护。

## 9. 当前授权

本设计批只新增合同，不授权修改生产、测试或既有审计器。下一批只允许新增 W1-E1 AST 审计器并在当前生产上得到可解释目标红；W1-S1 及以后须另立合同。R3-A、R4～R5、真实 provider、老师试点、集成和发布继续冻结。
