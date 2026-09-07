---
signals:
  refuted_count: 2
  blindspot_count: 4
subject: "仅执行 R1-A：question_performance.rs 测试模块行为保持式外移"
verification_date: "2026-07-31"
rounds_completed: 2
saturation_status: "saturated_for_r1_only"
initial_confidence: "medium"
final_confidence: "high_for_r1a_only"
decision_boundary: "仅执行 R1 合同枚举且已具备直接回归的批次；其余候选先补测试"
execution_contract: "../refactor-plans/jiaofu-suite-r1-execution-contract-2026-07-31.md"
problem_type: "mixed"
b2_mode: "degraded"
degraded_reason: "heterogeneous_model_not_invoked"
---

# 命题压力测试报告：教辅系统首批低风险重构

> 验证范围最终收窄为 R1-A：`question_performance.rs` 测试模块的行为保持式外移。唯一执行范围见 `../refactor-plans/jiaofu-suite-r1-execution-contract-2026-07-31.md`，不能外推为“整个一站式重构已经安全”。

## 方法论局限

- 审查饱和只表示当前证据足够决定 R1-A，不代表 R1.1 或 R2—R5 已验证。
- 本轮代码场景采用 B2c 单模型降级：主线视角检查调用与事务，反例视角检查状态耦合与验收缺口；没有把同一模型的两次判断伪装成异构模型共识。
- 文件行数、函数数量和 Git churn 只能证明组织压力，不能直接证明缺陷或老师减负。
- 真实老师是否更喜欢任务步骤条、异常收件箱和统一任务壳仍未验证。

## 一、命题与决策函数

### 精确命题

在保持公开 facade、IPC 命令、SQL、事务边界和所有生产逻辑不变的前提下，
先把 `question_performance.rs` 的内联测试模块原样外移到相邻测试文件。
现有直接回归对该动作提供高置信度支持，但安全性仍须由改动后同批回执确认。

### 不在本命题中的行动

- 不重写 `FixedIntakeTab` 状态机；
- 不移动 `Exam.tsx` 纯解析函数、常量或任何顶层组件；
- 不删除题库/知识点兼容标签；
- 不新建万能任务表或统一异常表；
- 不拆事务函数；
- 不改变 API/DTO；
- 不声明老师减负已经提升。

### 决策边界

| 证据等级 | 行动 |
|---|---|
| 高，且有目标行为回归 | 仅执行 R1 合同枚举动作，改动后原样重跑 |
| 中，缺少目标行为回归 | 先补测试，不移动代码 |
| 低或出现契约冲突 | 停止代码重构，先修正方案 |

### 参照类别

本仓库内已有“模块 facade + 内部 service 文件”“顶层 React 组件 + API facade”“内嵌 Rust 测试模块”三类结构，因此存在可直接验证的仓库内参照，不需要借用外部项目基准率。

## 二、预测—验证矩阵

### P1：大文件确实包含多个可独立命名的职责

- **证伪预测**：如果“大文件”只是大量类型、数据或单一凝聚算法，那么应当找不到多个顶层业务组件/use-case，按 feature 拆分只是形式主义。
- **零假设预测**：如果组织膨胀成立，应当能从源码中找到多个独立的老师任务和明确函数边界。
- **实际数据**：
  - `Exam.tsx` 3833 行，包含 9 个可单独度量的页面/Tab 区段；
  - `QuestionBank.tsx` 3687 行，至少包含答案源、知识链接、来源导入、搜索、候选、表现、组卷等 8 个区段；
  - `question_performance.rs` 3856 行，生产逻辑覆盖查询、影响预览、计划、case、解决、发布和默认版本升级。
- **判定**：✅ 通过。

### P2：顶层组件存在边界，但当前证据不足以放行移动

- **证伪预测**：如果所有组件都依赖页面闭包、共享局部变量或跨区段修改同一状态，组件移动将强迫同步重写状态。
- **零假设预测**：如果边界存在，子组件应当已经通过 props 接收输入和回调，移动文件不必改变业务流程。
- **实际数据**：
  - `Exam` 根组件通过 props 向 `FixedIntakeTab`、三类终审和补录组件传递数据、`onDone`、`onError`；
  - `QuestionBank` 多个 Panel 已是顶层函数；
  - 反例：`FixedIntakeTab` 自身仍有 17 个 `useState`、5 个 `useEffect`、19 个异步处理函数；
  - 反例：`QuestionBank` 页根仍直接持有整套蓝图组卷状态。
- **判定**：🟡 仅作为 R2 候选部分通过。当前缺少完整 Tab 级直接回归，因此不产生 R1 施工许可。

### P3：Rust 服务可以在保持调用 facade 的情况下拆分

- **证伪预测**：如果外部调用方直接依赖大量私有帮助函数或文件内部结构，拆文件会产生大范围调用方改写。
- **零假设预测**：如果 facade 可保持，外部调用应集中在少数公开函数，内部帮助函数可以留在父模块或分配给 use-case 子模块。
- **实际数据**：
  - Tauri 的消费者集中在 `src-tauri/src/knowledge_commands.rs`；
  - fixture 通过公开函数验证表现、影响和未来默认版本；
  - `service/mod.rs` 只公开 `question_performance` 模块，尚未形成函数级 re-export facade，但可以保持原模块路径；
  - 当前公开 use-case 数量有限，私有 scope/helper 与各 use-case 可按依赖归组。
- **判定**：✅ 通过，但拆分时必须保持 `service::question_performance::*` 兼容出口。

### P4：测试模块外移不会改变运行时业务

- **证伪预测**：如果测试代码参与生产编译或依赖无法在子测试模块访问的特殊作用域，外移可能改变构建或迫使公开私有实现。
- **零假设预测**：如果测试是标准子模块，`#[cfg(test)]` 边界清晰，外移后仍可通过 `super::*` 使用私有成员。
- **实际数据**：
  - `question_performance.rs` 第 2793 行出现 `#[cfg(test)] mod tests`；
  - 其后约 1060 行不参与正常生产构建；
  - 目标过滤测试 9/9 通过；
  - 全 `module-exam` 208/208 通过。
- **判定**：✅ 通过。这是当前证据最强、风险最低的首刀。

### P5：现有行为回执足以保护整个前端大规模搬迁

- **证伪预测**：如果关键 Tab 缺少自动化，当前 build/UI smoke 通过仍可能漏掉移动造成的交互回归。
- **零假设预测**：如果覆盖充分，应当能找到七个批改标签及题库全部面板的独立行为冒烟或组件测试。
- **实际数据**：
  - `npm run build` 通过；
  - 普通卷来源接线、题库综合流程、题目表现与版本影响三条 UI smoke 通过；
  - `package.json` 没有 Vitest/Jest/React Testing Library；
  - 当前 `scripts/test_*.py` 未为 `Exam` 七个标签分别提供可直接运行的 UI 冒烟；
  - 既有隔离 `.app` 验收证明历史纵切可运行，但不能替代本次结构重构后的同批重跑。
- **判定**：❌ 推翻“足以保护整个大规模搬迁”。当前只足以支持合同枚举的 R1-A。

### P6：仅移动组件就能解决前端状态耦合

- **证伪预测**：如果组件内部仍同时持有大量状态、effect 和异步流程，文件变小后状态复杂度不会下降。
- **零假设预测**：如果纯组件外移足够，主要复杂度应来自 JSX 体积而不是状态迁移。
- **实际数据**：
  - `FixedIntakeTab`：约 1532 行、17 个 state、5 个 effect、19 个异步函数；
  - `QuestionBank` 多个 Panel 各自有 4—11 个 state；
  - `Exam` API import 单次引入约 70 个类型和命令；
  - `QuestionBank` 同样从一个 API 模块引入大量类型和命令。
- **判定**：❌ 推翻。外移只能降低文件冲突；状态/use-case 重构必须作为后续独立批次验证。

### P7：当前增长主要是连续纵切追加，而不是一次性生成的大文件

- **证伪预测**：如果文件规模来自稳定单次设计，Git 历史不会表现为大量净新增和多次功能触达。
- **零假设预测**：如果是累积膨胀，应看到高新增/低删除和多次功能提交。
- **实际数据**：
  - `Exam.tsx`：32 次记录触达，累计新增 4009、删除 176；
  - `QuestionBank.tsx`：12 次记录触达，累计新增 3747、删除 60；
  - `question_performance.rs`：5 次记录触达，累计新增 4029、删除 173。
- **判定**：✅ 通过。增量交付策略解释了规模，也支持采用增量重构而非推倒重写。

### P8：模块化单体仍是当前更简单的解释

- **证伪预测**：如果模块化单体已经失效，应看到循环 crate 依赖、无法分离的部署边界或必须跨进程扩展的硬需求。
- **零假设预测**：如果仍合理，crate 依赖应由 Rust 编译约束为无环，本地事务和 SQLite 应继续构成主要一致性边界。
- **实际数据**：
  - workspace 以 `suite-core` 和业务 crate 组成；
  - Rust crate 依赖图由 Cargo 保证无环；
  - Tauri 外壳依赖各业务模块；
  - `module-profile`/`module-wrongbook` 作为聚合消费者依赖 exam/knowledge，符合读模型方向；
  - `cargo check --manifest-path src-tauri/Cargo.toml` 通过。
- **判定**：✅ 通过。未发现引入微服务或新框架的硬证据。

### P9：当前工作区允许在不覆盖既有改动的情况下开始目标重构

- **证伪预测**：如果目标文件已有未提交修改，重构会混入既有施工，难以归因或回滚。
- **零假设预测**：如果可安全起步，首批目标文件应当在当前 worktree 中无修改。
- **实际数据**：
  - `git status --short --` 对 `Exam.tsx`、`QuestionBank.tsx`、`question_performance.rs`、`src/api/exam.ts`、`src/api/knowledge.ts` 无输出；
  - 工作区其他 M6/构建/文档改动仍存在，必须保持不动。
- **判定**：✅ 通过，仅适用于不触碰当前已修改文件的独立小批。

### P10：系统已经存在验证“零学习成本”的数据合同

- **证伪预测**：如果没有度量基础，统一任务壳的价值只能依赖主观评价。
- **零假设预测**：如果已有基础，应能找到老师主动耗时、操作次数、异常复核和机器修正等字段。
- **实际数据**：
  - `module-exam/src/teacher_shadow.rs` 已包含 `teacher_active_seconds`、`teacher_action_count`、`exception_review_count`、`corrected_machine_decision_count`；
  - M1 也有人工基线/辅助复核的试点指标合同；
  - 当前没有统一任务壳改造后的真实老师成对数据。
- **判定**：🟡 部分通过。度量基础存在，效果数据不存在。

## 三、两轮更新记录

### Round 1：结构与依赖

| 发现 | 更新 |
|---|---|
| 三个目标文件存在多职责和明显顶层边界 | 支持后续拆分方向，但不直接放行组件移动 |
| `FixedIntakeTab` 内部状态密度高 | 降低“直接重写状态”可行性 |
| Rust 外部消费者集中 | 提高保持 facade 拆分可行性 |
| 目标文件当前无 worktree 重叠 | 提高首批可逆性 |

证据从“值得拆分”推进到“Rust 测试外移具有明确边界”；前端组件移动仍保持中等置信度并继续冻结。

### Round 2：运行行为与反例

| 运行 | 结果 |
|---|---|
| `cargo test -p module-exam question_performance -- --nocapture` | 9/9 通过 |
| `cargo test -p module-exam` | 208/208 通过 |
| `cargo check --manifest-path src-tauri/Cargo.toml` | 通过 |
| `npm run build` | 通过，58 modules transformed |
| 普通试卷来源接线 UI smoke | 通过 |
| 题库综合 UI smoke | 通过 |
| 题目表现与版本影响 UI smoke | 通过 |

三条 UI smoke 首次运行均因未启动 `127.0.0.1:4173` 而 `ERR_CONNECTION_REFUSED`；启动 `npm run preview -- --host 127.0.0.1 --port 4173` 后原样重跑通过。这说明测试本身有效，也暴露了运行前置未封装的可重复性缺口。

目标 Rust 测试已有直接过滤测试和全模块测试，因此 R1-A 提升为高置信度候选；前端缺少完整 Tab 级组件自动化，仍不足以进入执行区。

## 四、被数据打脸的认知

### 盲区 B1：把“文件拆小”误当成“状态变简单”

纯移动只能改善可导航性和合并冲突。`FixedIntakeTab` 的 17 个状态与 19 个异步动作仍然存在，必须在后续用状态迁移测试证明 reducer 设计。

### 盲区 B2：把历史验收当成本次重构的完整特征测试

历史真机证据证明当时版本工作，不会自动证明新文件边界无回归。当前自动化对普通卷和题库较强，对 `Exam` 全标签的直接覆盖不足。

### 盲区 B3：把顶层 API facade 当成已经细分的 use-case facade

`src/api/exam.ts` 和 `src/api/knowledge.ts` 集中了类型与 IPC，这比散落调用更好；单文件导出面过大，前端 feature 抽取时仍需兼容层和分阶段迁移。

### 盲区 B4：把测试命令当成自包含验收

三条 UI smoke 依赖外部预览服务。未来应提供一个可重复的包装命令，负责 build、启动、等待、测试和关闭，避免“连接失败”被误报为产品失败。

## 五、零假设验证

> 零假设：现有核心架构大体正确，先做保持行为的组织拆分比重写更合适。

| # | 零假设预测 | 实际数据 | 结果 |
|---|---|---|---|
| H1 | 现有业务测试应大量通过 | module-exam 208/208 | ✅ |
| H2 | Tauri 仍可从各业务 crate 编译 | Tauri cargo check 通过 | ✅ |
| H3 | 前端现有页面可生产构建 | TypeScript + Vite build 通过 | ✅ |
| H4 | 大文件内部已有可抽取顶层边界 | 多个 Tab/Panel/公开 use-case | ✅ |
| H5 | 目标文件无当前未提交冲突 | 五个目标路径 status 为空 | ✅ |
| H6 | 当前 UI 自动化覆盖所有目标流程 | 未覆盖 Exam 全标签 | ❌ |
| H7 | 组件移动本身可消除状态耦合 | FixedIntake 状态密度仍高 | ❌ |

零假设通过 5/7。核心成立，边界被收窄为合同枚举的 R1-A。

## 六、证据更新结论

### R1-A 首批组织拆分

- **置信度**：高，但只覆盖合同枚举的测试模块外移；
- **结论**：具备改动前直接回归，可进入“一次只改变测试组织”的小批施工；
- **允许动作**：仅执行 `../refactor-plans/jiaofu-suite-r1-execution-contract-2026-07-31.md` 的 R1-A；
- **禁止扩张**：不得移动前端纯函数、常量或组件，不得重写 reducer、改变 DTO、拆事务、删兼容标签或构建统一任务表。

### R1.1—R5

- 当前未进入执行区；
- 需要 R1-A 回执、目标前端直接测试、完整 Tab 特征测试或真实老师任务对照数据后，按各阶段门槛重新更新。

## 七、停止判断

- 两轮验证已经改变行动边界：从“开始整体重构”缩窄为“只执行 R1-A 测试外移”；
- 继续读取更多静态代码不会改变 R1-A 决策；
- 下一高价值信息来自实际 R1-A diff、回归结果和 Feynman 独立审查；
- 因此当前状态为 `saturated_for_r1_only`。

## 八、证据来源

证据快照：Git commit `6072360049b0`。取证时
`Exam.tsx`、`QuestionBank.tsx`、`question_performance.rs`、`src/api/exam.ts`
和 `src/api/knowledge.ts` 均无未提交修改；工作区其他既有改动未纳入本命题。

| 来源 | 层级 | 用途 |
|---|---|---|
| 当前源码行数、组件状态和调用位置 | L1 | 结构事实 |
| Cargo 依赖和 Tauri command 消费者 | L1 | 模块/facade 边界 |
| module-exam 208 项和目标 9 项测试 | L1 | 业务行为基线 |
| Tauri cargo check、前端 build | L1 | 编译基线 |
| 三条 Playwright 式 UI smoke | L1 | 关键老师流程 |
| Git numstat 历史 | L1 | 增量膨胀事实 |
| 现行项目方案和体验核心 | L2 | 设计意图 |
