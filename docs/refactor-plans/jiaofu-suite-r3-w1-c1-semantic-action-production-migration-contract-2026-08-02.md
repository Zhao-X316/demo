---
title: jiaofu-suite R3-W1-C1 semantic action production migration
date: 2026-08-02
status: authorized_semantic_interface_migration
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_index_sha256: b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b
source_porcelain_count: 165
scope: R3-W1-C1-semantic-action-production-migration
production_change_authorized: true
test_change_authorized: false
existing_audit_change_authorized: false
provider_change_authorized: false
behavior_change_authorized: false
release_authorized: false
---

# R3-W1-C1 终审工作台语义动作生产迁移合同

## 唯一因素

只把三个 controller 对 Tab 暴露的 raw setter 和 `SubjectiveComponentEditor` 的 setter props 改为语义动作：

- 公共：`selectAssessment`、`selectItem`；
- Objective：`changeManualScore`、`changeManualNote`；
- Dictation：`changeCorrection`、`changeManualScore`、`changeManualNote`、`changeManualEvidence`；
- Subjective：`changeCorrection`、`changeManualScore`、`changeManualNote`、`changeComponentScore`、`changeComponentEvidence`、`changeComponentNote`。

内部 React setter 继续私有。所有动作只封装既有 `setState(current => ({...current,[key]: value}))` 或标量 setter，不引入 reducer、校验、debounce、并发或新状态。

## 路径与行为边界

允许修改三个 controller、三个 Tab 和 `SubjectiveComponentEditor.tsx`，新增本合同并写回执/权威记录。禁止修改其他子组件、provider、`Exam.tsx`、API、测试、审计器、依赖、Rust/SQL/DTO。

DOM、文案、输入值、事件触发时机、provider 名称/参数/顺序/幂等键、老师终审和发布语义必须不变。不得提取共享 hook。

## 验收

1. W1 AST 审计 `27/27`、exit 0；三个 controller actions 无 raw setter，语义 action 齐全；Editor 无 Dispatch/setter props；
2. provider 20 命令/22 调用点/hash 与三台结构项不变；
3. C2/C1/H1、65/65、build、Python、whitespace 与 27/27 UI 全绿；
4. `Exam.tsx`、API、依赖、测试、审计器与非目标 Subjective 子组件哈希不变；
5. 展开 porcelain `165→166`，唯一新增仓库路径为本合同；HEAD/index/staged 不变。

通过 C1 只放行 W1-V1 独立只读终审，不证明 W1/R3、真实 provider、老师减负、集成或发布完成。
