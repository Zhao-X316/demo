---
status: "current_execution_ssot"
implementation_status: "r1_a_automation_passed_2026-08-01"
review_status: "pending_cross_model_change_record_review"
integration_status: "uncommitted_worktree_no_tag_merge_push_release"
source_commit: "6072360049b0"
scope: "R1"
receipt: "/Users/zzx/agent-shared/笔记/教辅系统/验收/2026-08-01-r1-a-refactor/00-R1-A测试模块外移验收总览.md"
supersedes:
  - "../hegel-reports/jiaofu-suite-one-stop-refactor-hegel-2026-07-31.md#r1零业务变化的组织拆分"
  - "../reports/jiaofu-suite-one-stop-refactor-bayes-2026-07-31.md#r1-a-首批组织拆分"
---

# 教辅系统 R1 行为保持式重构执行合同

> 本文件是 R1 唯一执行口径。Hegel、Bayes、Feynman 报告保存分析和审计历史，不再各自定义可执行范围。
>
> **治理边界**：R1-A 当前实现位于以 `6072360049b0` 为改前基线的未提交验收工作树；异模型修改记录审查仍待处理。审查完成前不得 tag、合并、推送或发布。本文不授权 R1.1 代码施工。

## 一、目标

在不改变任何生产逻辑、公开符号、SQL、事务、IPC、DTO、文案或界面的前提下，把 `question_performance.rs` 末尾的内联测试模块原样移入相邻测试文件，降低生产文件的阅读噪声。

本批只能证明“测试组织可以安全外移”，不能证明：

- 文件变小后业务状态更简单；
- 整体重构已经安全；
- 老师操作已经减少；
- R1.1、R2 或后续任务可以自动放行。

## 二、唯一允许的改动

| 批次 | 原文件 | 新文件 | 允许动作 |
|---|---|---|---|
| R1-A | `crates/module-exam/src/service/question_performance.rs` | `crates/module-exam/src/service/question_performance_tests.rs` | 将现有 `#[cfg(test)] mod tests { ... }` 的模块体原样外移；原文件只保留带显式 `#[path]` 的测试模块声明 |

允许为外移后的相对模块路径做最小语法调整，但不得：

- 修改测试断言、fixture 数据、测试名或测试数量；
- 将生产私有函数改成 `pub` 以迁就测试；
- 修改生产函数、SQL、事务、audit/outbox 或错误语义；
- 修改前端文件、API/DTO 或数据库迁移；
- 顺手格式化或重命名相邻生产代码。

## 三、明确不属于 R1

| 候选工作 | 当前归属 | 放行条件 |
|---|---|---|
| 外移 `Exam.tsx` 纯解析函数和常量 | R1.1 候选 | 为目标函数建立可直接运行的行为测试并重新审查 |
| 外移 `FixedIntakeTab` 或其他顶层 Tab/Panel | R2 | 完整 Tab 特征测试和 R1.1 回执 |
| reducer、hook、请求编排重写 | R3 | 状态迁移表、并发/刷新测试和独立设计审查 |
| Rust 生产 use-case 拆分 | R4 | 调用图、事务所有权表和失败注入回执 |
| 一站式任务壳 | R5 | 稳定 feature、组合读模型及真实老师对照指标 |
| provider、订阅、成本账本和离线治理 | 独立商业化/运行治理项目 | 不从本重构计划自动继承施工许可 |

## 四、R1-A 验收合同

### 结构不变量

1. `question_performance.rs` 在原测试模块之前的生产代码保持字节级不变。
2. `question_performance_tests.rs` 的测试模块体与原内联模块体保持语义和断言一致。
3. 生产构建不包含测试文件。
4. 公开 Rust 函数集合、Tauri command、SQL、迁移和 Cargo 依赖均不变化。
5. 目标 diff 不得混入目标文件以外的现有未提交修改；若要进一步证明全部非目标路径在施工前后字节未动，必须在施工前另存状态/哈希快照，不能用施工后的单次 `git status` 反推。

### 必跑回执

| 顺序 | 命令 | 通过条件 | 失败动作 |
|---:|---|---|---|
| 1 | `cargo test -p module-exam question_performance -- --nocapture` | 目标测试仍为 9/9 通过 | 停止，不进入下一条 |
| 2 | `cargo test -p module-exam` | 全模块仍为 208/208 通过 | 停止并恢复本批 |
| 3 | `cargo check --manifest-path src-tauri/Cargo.toml` | Tauri 外壳编译通过 | 停止并恢复本批 |
| 4 | `git diff --check -- crates/module-exam/src/service/question_performance.rs crates/module-exam/src/service/question_performance_tests.rs` | tracked diff 无空白错误；首次创建且尚未跟踪的新文件还须补做 no-index whitespace 检查并确认输出无诊断 | 修正后从第 1 条重跑 |
| 5 | 人工检查目标 diff | 除测试模块声明和新测试文件外无生产变更 | 停止并恢复本批 |

基线来自 commit `6072360049b0`，本轮已记录：

- 目标测试：9/9 通过；
- `module-exam`：208/208 通过；
- Tauri `cargo check`：通过。

这些仅是改动前基线。只有改动后原样重跑并通过，R1-A 才能完成。

> **首次创建文件的检查补充**：普通 `git diff --check` 会忽略未跟踪文件。对本合同的新测试文件还要执行 `git diff --no-index --check -- /dev/null crates/module-exam/src/service/question_performance_tests.rs` 并检查输出为空；该命令因“文件内容不同”正常返回 1，不能只按退出码判定空白错误。随后仍须以人工 diff 或逐字节比较确认内容来自原测试模块体。

### 可复演的结构不变量检查

以下命令固定使用本合同的改前基线，不以可能移动的 `HEAD` 代替。运行 shell 为 zsh：

```zsh
SOURCE_COMMIT=6072360049b0
SOURCE_FILE=crates/module-exam/src/service/question_performance.rs
TEST_FILE=crates/module-exam/src/service/question_performance_tests.rs

cmp \
  <(git show "$SOURCE_COMMIT:$SOURCE_FILE" | sed -n '1,2792p') \
  <(sed -n '1,2792p' "$SOURCE_FILE")

git show "$SOURCE_COMMIT:$SOURCE_FILE" \
  | sed -n '2795,3855p' \
  | sed 's/^    //' \
  | cmp - "$TEST_FILE"

test "$(rg -c '^#\[test\]' "$TEST_FILE")" -eq 9

git status --short -- "$SOURCE_FILE" "$TEST_FILE"
```

前三条必须以退出码 0 结束；最后一条只用于确认目标路径状态，不得据此暂存文件，也不能单独证明全部非目标路径未动。人工范围检查仍须确认原文件仅保留 `#[cfg(test)]`、显式 `#[path]` 和 `mod tests;`，且没有把目标外工作树混入本批。

R1-A 完成后没有自动继承的下一代码包。R1.1 测试启用合同已在 `docs/refactor-plans/jiaofu-suite-r1-1-test-enablement-contract-2026-08-01.md` 起草并通过无上下文同模型读者复核；复核发现的脏文件保护、fail-fast、快照覆盖和 zsh 特殊变量问题均已修正，但该结果不替代异模型修改记录审查，且合同仍为 `code_authorized=false`。只有另行取得明确施工许可后，才可执行其中 E1；当前不得创建测试框架、测试文件或修改 `Exam.tsx`。

## 五、完成和停止条件

R1-A 完成必须同时满足：

- 唯一允许改动未越界；
- 五项回执全部通过；
- Feynman 复审无阻塞项；
- 没有把工作区其他改动混入本批。

任一条件不满足，状态保持“未完成”，不得以文件行数下降或单个测试通过替代。
