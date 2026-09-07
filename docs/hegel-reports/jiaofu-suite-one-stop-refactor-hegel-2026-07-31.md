---
signals:
  terminal_status: "converged_with_blind_spots"
  system_type: "mixed"
source_idea_pool: "../osborn-pools/jiaofu-suite-one-stop-refactor-2026-07-31.md"
execution_contract: "../refactor-plans/jiaofu-suite-r1-execution-contract-2026-07-31.md"
evidence_snapshot_commit: "6072360049b0"
standalone: true
analysis_rounds: 1
claims_extracted: 24
findings_total: 16
findings_confirmed: 13
findings_uncertain: 3
findings_dismissed: 7
---

# 收敛分析报告：教辅系统一站式体验与可持续重构

## 收敛摘要

本次使用 `ideation` lens 和 `--standalone` 模式，把 60 条发散想法收敛为 16 条架构发现和 7 条明确不采用的路径。该终态表示**方案空间已经收敛**，不表示真实减负、代码风险或老师接受度已经得到验证；这些断言留给后续 Bayes 和 Feynman。

| 指标 | 值 |
|---|---:|
| 分析轮次 | 1 次独立收敛 |
| 终态 | `converged_with_blind_spots` |
| 系统类型 | `mixed` |
| 输入想法 | 60 |
| 原子断言 | 24 |
| 核心发现 | 16 |
| 收敛决策 | 13 |
| 待验证决策 | 3 |
| 明确排除路径 | 7 |

## 定界

### 目标

在不改变老师终审、不可变版本、事务、审计、备份恢复和现有验收语义的前提下：

1. 将老师体验收敛成一个任务一个主入口；
2. 把超大前端页面和 Rust 服务拆成可独立施工、测试和回滚的 feature/use-case；
3. 保持已有 IPC、数据库和跨模块证据契约兼容；
4. 用真实任务回执而不是文件行数证明重构有效。

### 公理约束

- **AX1**：AI 只能形成建议，不能自动形成老师结论、发布成绩或正式学习证据。
- **AX2**：不得静默覆盖原始证据、历史版本、评分 revision 或图谱快照。
- **AX3**：跨表业务写入继续由 Rust 业务事务持有，不能因拆文件退化为前端多次 IPC 拼接。
- **AX4**：当前工作区已有未提交修改，重构不得覆盖、重置或混淆其归属。
- **AX5**：本轮不发布、不推送、不改真实数据库、不接触真实学生材料或生产凭据。

### 分析维度

老师一站式体验、跨模块契约、AI 证据与异常、前端组织、Rust/事务边界、测试验收、商业化与运行治理。

## 原子断言收敛

| 断言组 | 收敛后的原子断言 | 类型 | 状态 |
|---|---|---|---|
| C1 | 老师需要统一任务入口，不需要理解内部模块 | epistemic | 待 Bayes 验证老师流程 |
| C2 | 统一入口不要求合并领域存储和业务状态机 | deterministic | 收敛 |
| C3 | `Exam.tsx` 同时承载多个独立业务能力 | deterministic | 收敛 |
| C4 | `QuestionBank.tsx` 同时承载多个独立业务能力 | deterministic | 收敛 |
| C5 | `question_performance.rs` 同时承载多个 use-case 和大段测试 | deterministic | 收敛 |
| C6 | 按 feature/use-case 拆分可以在保持 facade 的情况下完成 | deterministic | 待代码特征测试验证 |
| C7 | 统一异常卡能降低老师寻找入口的成本 | epistemic | 待真实老师试点 |
| C8 | 全局万能任务表会复制已有模块事实并扩大耦合 | causal | 收敛 |
| C9 | 全局前端状态机不适合当前多个相对独立工作流 | evaluative | 收敛 |
| C10 | 每个 feature 使用局部 reducer/状态机可以减少不可能状态 | causal | 待实现验证 |
| C11 | Rust 拆分必须保持原事务函数和公开 facade | prescriptive | 收敛 |
| C12 | 先建立特征测试再移动代码能降低语义漂移风险 | causal | 收敛 |
| C13 | M3 与 M6 可以共用老师入口但不能合并事实模型 | deterministic | 收敛 |
| C14 | 代码行数只能作为预警信号，不能单独决定拆分 | evaluative | 收敛 |
| C15 | “零学习成本”必须用首次成功率和操作耗时验证 | epistemic | 收敛为验收要求 |
| C16 | 模型缓存、离线降级和托管网关应在稳定工作流边界上建设 | prescriptive | 收敛 |

## 确认的发现

> 除 R1 执行合同明确枚举的 R1-A 外，本节均为架构候选结论，不构成代码施工许可。

### Major F1：一站式应统一老师任务壳，不统一领域事实

- **架构候选**：建立统一的任务进度、异常入口、证据抽屉和下一步动作；M1、M2、K1、M3、M6 继续维护自己的领域事实和版本语义。
- **原因**：老师需要的是“现在该做什么”，不是一张跨模块万能表。领域合并会把背诵 effect、成绩 publication、题目版本和图谱快照压成不清晰的公共状态。
- **落地形态**：先做组合读模型和 UI 协议，不立即新建全局 `task_session` 表。
- **置信等级**：高；领域事实分离有现行模型证据，统一任务壳的老师收益未验证。
- **effective_severity**：major。

### Major F2：`Exam.tsx` 是当前前端第一重构目标

- **证据**：文件 3833 行；顶层页面从第 381 行开始，同时包含入口、固定材料导入、主观链接、主观终审、默写、客观终审、成绩、题目和知识点组件。
- **关键切口**：第 521 行开始的 `FixedIntakeTab` 延续至约第 2052 行，单一组件约 1530 行。
- **架构候选**：按 feature 抽取，不先重写业务状态。
- **置信等级**：高；文件职责边界已验证，实际抽取效果未验证。
- **effective_severity**：major。

### Major F3：`QuestionBank.tsx` 应按面板能力拆分

- **证据**：文件 3687 行，包含答案来源、知识链接、题面来源、结构化/语义搜索、候选晋级、版本影响、题目表现和蓝图组卷。
- **架构候选**：保留 `QuestionBank` 页面为编排器，把现有顶层 Panel 函数逐个迁入 feature；不在同一批改变 API 或交互。
- **置信等级**：高；面板边界已验证，实际抽取效果未验证。
- **effective_severity**：major。

### Major F4：前端拆分边界应是业务能力而不是视觉碎片

- **架构候选**：目标包为 `exam-intake`、`objective-review`、`subjective-review`、`dictation-review`、`grade-publication`、`question-source`、`answer-source`、`knowledge-link-review`、`question-search` 和 `blueprint-paper`。
- **反例**：只把 JSX 切成大量无业务含义的 `Card1/Card2` 文件，行数下降但状态和 IPC 耦合不变。
- **置信等级**：中；业务能力边界合理，目录粒度尚未通过实际拆分校准。
- **effective_severity**：major。

### Major F5：页面只能协调 feature，不继续持有业务语义

- **架构候选**：页面保留路由、标签、班级/作业上下文和跨 feature 刷新信号；解析、校验、请求键、状态迁移和 IPC 由 feature adapter/hook 负责。
- **边界**：不引入全局状态库作为前置条件，先使用 React 现有能力和局部 reducer。
- **置信等级**：中；目标职责清晰，hook/reducer 边界尚未实现验证。
- **effective_severity**：major。

### Major F6：Rust 拆分应保持公开 facade 和事务所有权

- **证据**：`question_performance.rs` 3856 行，生产逻辑覆盖题目表现、版本影响、复核 case、评分解决、再发布和默认版本升级。
- **架构候选**：未来可按 `performance_query`、`impact_preview`、`impact_plan`、`impact_review`、`impact_publish`、`default_upgrade` 拆分；原模块继续统一 re-export，调用方无需同步大改。
- **事务原则**：每个 use-case 的连接、事务、audit/outbox 和幂等边界保持在同一服务函数内。
- **置信等级**：高；facade 与事务约束已验证，生产 use-case 拆分尚未放行。
- **effective_severity**：major。

### Minor F7：Rust 测试外移是最低风险的第一刀

- **证据**：`question_performance.rs` 第 2793 行起进入 `#[cfg(test)] mod tests`，约 1060 行。
- **执行边界**：唯一允许动作以 R1 执行合同为准；测试迁入相邻 `question_performance_tests.rs`，仅改变组织，不改变生产符号、SQL 或断言。
- **置信等级**：高；存在目标过滤测试和全模块回归，改动后仍须原样重跑。
- **effective_severity**：minor。

### Major F8：重构顺序必须由行为回执驱动

- **架构候选**：先冻结 build、专项 UI smoke、Rust 测试、隔离数据库、同 HOME 重启回读和失败恢复；再执行纯移动式拆分；每批比较相同回执。
- **原因**：当前系统的价值在状态、版本和事务语义，TypeScript 编译通过不能证明这些语义未漂移。
- **置信等级**：高；回执原则已由现有验收事实支持，各后续批次仍需独立合同。
- **effective_severity**：major。

### Minor F9：局部 reducer 优于全局万能状态机

- **架构候选**：只对 `intake` 等拥有明确阶段和互斥状态的 feature 使用 reducer/状态机；搜索、只读统计等保留普通 query state。
- **置信等级**：中；状态密度已验证，reducer 的净收益未验证。
- **effective_severity**：minor。

### Major F10：异常协议统一在表现层和读模型，不统一持久化

- **统一字段**：来源、对象、证据摘要、影响范围、严重度、可执行主动作、是否阻断、原模块引用。
- **保持分开**：ASR failed、页面质量、OCR 分歧、评分不确定、知识链接缺失仍由各模块保存真实状态。
- **置信等级**：高；事实状态分离是现行约束，统一展示效果未验证。
- **effective_severity**：major。

### Major F11：M3 与 M6 是同一教师决策入口的两个平面

- **架构候选**：老师看到统一的“需要巩固”；M3 提供当前有效发布错题事实和订正，M6 提供带覆盖度/可信度/新鲜度的掌握推断。
- **禁止**：把错题记录直接等同于能力分，或把两者合并成可覆盖的单表。
- **置信等级**：高；事实与推断的生命周期差异已验证，统一入口体验未验证。
- **effective_severity**：major。

### Minor F12：文件规模采用预算预警，不采用机械上限

- **建议初始预警线**：页面编排器 300 行、单 feature view 600 行、hook/reducer 400 行、Rust use-case 800 行。
- **用途**：提示重新检查职责；超过并不自动失败，必须结合事务完整性和逻辑凝聚度。
- **置信等级**：低；阈值只是候选预警线，尚未用本仓库缺陷率校准。
- **effective_severity**：minor。

### Major F13：模型、离线和商业能力不能反向污染本地工作流

- **产品边界候选**：AI 阶段按 hash/版本缓存，离线保留导入、归组、证据查看和人工处理，订阅到期不锁历史数据。
- **范围边界**：归入独立商业化/运行治理候选项目，不属于本重构 R1—R5，也不会从本报告自动获得施工许可；未来只有出现经过验证的直接依赖时才单独立项。
- **置信等级**：中；原则与产品方向一致，但不属于本重构验证范围。
- **effective_severity**：major。

## 待验证发现

### F14：统一任务壳是否显著降低老师操作成本

- 需要同一老师、同一样本、同一工作量的旧/新流程对比。
- 指标：首次成功率、必要按钮数、异常定位耗时、每百份主动操作时间。
- 当前状态：未验证。

### F15：局部 reducer 是否比现有状态组织更易维护

- 需要先对 `FixedIntakeTab` 建立状态迁移表，并验证 reducer 没有引入额外刷新和过期状态。
- 当前状态：未验证。

### F16：建议的规模预警线是否适合本仓库

- 这些数值只是重构触发器初值，需要经过 2—3 个 feature 拆分后的缺陷率、审查时间和修改冲突校准。
- 当前状态：未验证。

## 已排除的路径

1. **立即建立全局万能 `task_session` 表**：当前各模块已经拥有稳定任务/批次/作业/快照 ID，先用组合读模型证明缺口。
2. **把所有异常迁入一张多态异常表**：会产生真相副本；统一展示协议即可。
3. **全应用使用一个前端状态机**：不同工作流的状态密度不同，复杂度会集中到新的万能层。
4. **按行数机械切文件**：可能把一个事务或一个完整 use-case 拆散。
5. **重构时同步更换框架或引入微服务**：当前模块化单体和本地 SQLite 与产品边界相符。
6. **立即删除 `Exam` 中题目/知识点兼容入口**：需要先验证导航、深链和老师现有流程，无证据时不删。
7. **把 M3 错题与 M6 图谱合并成一个可写模型**：事实与推断生命周期不同，只合并老师入口。

## 零假设检验

> 零假设：现有架构核心大体正确，问题主要是近期功能追加造成的组织膨胀，不需要推倒重做。

| # | 零假设断言 | 结果 | 证据 |
|---|---|---|---|
| H1 | 系统已经是模块化单体 | 通过 | `crates/core`、各业务 crate、Tauri 外壳和 React 前端已分层 |
| H2 | 老师终审、版本和审计边界已经存在 | 通过 | 当前 M1/M2/K1/M6 规格及既有迁移、audit/outbox、revision 机制 |
| H3 | 超大文件内部已经存在可抽取的业务组件和函数边界 | 通过 | `Exam.tsx`、`QuestionBank.tsx` 已按顶层 Tab/Panel 函数分段 |
| H4 | 当前 IPC 已经有独立 API facade | 通过 | `src/api/exam.ts`、`src/api/knowledge.ts` 等已集中类型和调用 |
| H5 | 所有大文件都必须立刻拆分 | 不通过 | 一些大 Rust 文件包含凝聚事务或大量测试，需按职责和回执判断 |

**零假设结论**：核心架构保持，执行增量重构；不进行框架迁移、数据库总表化或微服务化。

## 未来候选顺序（非执行许可）

> R1-A 之外的每个阶段都必须重新取证、建立独立执行合同并通过审查；下列顺序只表达依赖关系。

### R0：基线冻结

- 记录当前工作区改动归属；
- 运行与目标模块相关的现有测试、build、UI smoke 和隔离 fixture；
- 保存 DTO/数据库/重启回读回执。

### R1：零业务变化的组织拆分

本节是收敛阶段的历史建议，已由
`../refactor-plans/jiaofu-suite-r1-execution-contract-2026-07-31.md`
收窄并取代。当前可执行 R1 仅包含 `question_performance.rs` 测试模块外移；
`Exam.tsx` 纯解析函数和常量外移降为 R1.1 候选，不在本批施工。

### R2：前端 feature 抽取

- 先抽 `FixedIntakeTab`；
- 再抽 objective/subjective/dictation review；
- 再抽 QuestionBank 的各 Panel；
- 页面变成薄编排器。

### R3：状态与请求编排

- 为 intake 建立局部 reducer；
- 抽取 query/command hooks；
- 统一 busy/error/toast/refresh 和幂等请求键。

### R4：Rust use-case 拆分

- 先拆只读 query；
- 再拆影响 preview/plan；
- 最后拆确认/发布/default upgrade；
- facade、事务、audit/outbox 和测试回执保持不变。

### R5：一站式任务壳

- 在稳定 feature 上组合任务进度读模型和异常协议；
- 不修改各模块事实表；
- 用真实老师对比指标决定是否继续扩大统一范围。

## 边界漂移检查

- **无目标漂移**：仍围绕一站式体验和超大文件施工风险。
- **盲区 1**：真实老师是否更喜欢步骤条而不是标签页，尚无对比数据。
- **盲区 2**：当前未提交工作区包含 M6/构建验收改动，实施时必须避免把它们误记为重构产物。
- **盲区 3**：文件规模预警阈值未经本仓库历史缺陷数据校准。
- **证据快照**：结构取证对应 Git commit `6072360049b0`；后续源码变化必须重新取证，不能沿用本文行数。
- **范围覆盖等级**：高；代码组织范围已覆盖，真实老师体验和规模阈值仍是明确盲区。

## 审计日志

- 输入想法池格式通过：60 条，P1/P2a/P2b 和全部 SCAMPER 算子齐全。
- `problem_type=mixed` 与本次分析一致：代码组织属于确定性问题，老师接受度和减负效果属于信息不足问题。
- Hegel preflight 脚本硬编码扫描 `/Users/zzx/.qoder/skills`，报告缺少依赖；当前实际技能位于 `/Users/zzx/.codex/skills`，且 pipeline 已明确使用 standalone 跳过内嵌 Bayes/Feynman。因此本次采用手工 lens 协议完成收敛，并把该工具路径问题记为流程盲区。
- 本转未执行 Bayes、Feynman、代码修改、真实老师试点或发布操作。
