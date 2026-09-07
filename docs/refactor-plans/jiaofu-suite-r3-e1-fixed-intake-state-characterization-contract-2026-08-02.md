---
title: jiaofu-suite R3-E1 fixed intake state characterization contract
date: 2026-08-02
status: authorized_local_test_enablement
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_fixed_intake_sha256: 6f8192950782c9476e14a64ca0a3a6ed8e53d918140a889492b02378676a3d93
source_test_sha256: e03db3110b6bb332ba98cb97526e6b4dc01e25b5f2698ba17a1e6a622ca2bc7e
scope: R3-E1-fixed-intake-state-request-characterization
production_state_rewrite_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-E1 固定上传状态与请求特征测试前置执行合同

## 目标

在不修改生产代码的前提下，补齐 `FixedIntakeTab` 进入 reducer/controller 迁移前缺失的浏览器特征证据。冻结上下文切换、输入变更、准备重试、换批清理、父级等值刷新和完整 reload 的当前行为；不得用本批测试提前实现或伪装 R3-H1 的防双击、StrictMode 和 stale completion 纠偏。

## 改前事实

1. `src/pages/exam/FixedIntakeTab.tsx` 为 1,121 行，持有 41 个 `useState`、5 个 `useEffect`、19 个异步块和 23 个唯一业务命令。
2. `resetRequest()` 清除当前批次、答案分析、归组/质量和三材料多数投影，但不清除学生文件、答案输入、页周期/页数，也没有清除普通卷确认、部分 busy/进行中标记。
3. `pickStudentPapers()` 使用另一套手写 reset：它保留答案输入，但额外清除普通卷确认和三材料逐页处理状态，再异步推断新文件页周期。
4. 班级或作业选择调用 `resetRequest()`，因此当前行为是“保留本机草稿、清除已生成 session”；合法 options 以等值新数组刷新时，两个校准 effect 不应重置当前合法选择。
5. prepare 失败保留 request key；页数、答案文本、答案文件或上下文变化会清除该 key，下一次提交应生成新 key。
6. 当前没有按 batchId 恢复固定上传草稿的只读 API；完整 WebView reload 只能回上传起点，已落库事实由各终审工作台读取。
7. 现有 841 行脚本已经覆盖基本输入、prepare 同输入重试、答案一致/冲突、材料类型、归组、质量、重拍和三材料路由，但未覆盖上述 reset/刷新矩阵。

## 施工快照

- 分支：`codex/t2-artifacts`
- HEAD：`6072360049b09c3155726773039da139834294fe`
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`
- 展开 porcelain：95 条；staged：0
- `FixedIntakeTab.tsx` SHA-256：`6f819295...a3d93`
- 固定上传 UI 脚本 SHA-256：`e03db311...bc7e`；改前专项浏览器测试通过

## 唯一允许修改

1. 本合同。
2. `scripts/test_exam_fixed_intake_shared_shell_ui.py`：只增加 mock 能力、辅助函数和当前行为断言。
3. 完成后的验收总览、重构标准、文档索引、任务地图和修改记录。

不得修改 `src/`、`src-tauri/`、`crates/`、API/DTO、样式、依赖、既有其他测试或数据库。

## 必须新增的特征矩阵

### 上下文和草稿

1. 同班切换作业版本：清除 result/session，保留学生文件、答案文本和每人页数；再次提交使用新 assessment version。
2. 切换班级：清除 result/session，保留相同草稿；assessment 校准到新班首个合法版本，再次提交使用该版本。
3. 父级 `load()` 返回内容相同但引用全新的 options 数组：当前合法班级、作业、文件、答案、页数和 session 不重置。

### 输入和 prepare key

4. 同输入 prepare 失败后直接重试继续复用原 key（保留既有断言）。
5. prepare 失败后修改每人页数：清除旧 key，下一次提交产生新 key并使用新页数。
6. prepare 失败后修改答案文本：清除旧 key，下一次提交产生新 key并使用修正后的去首尾空白文本。
7. 选择答案文件与粘贴答案互斥；清除答案文件后清除旧 result，保留学生文件，再提交不携带答案文件或文本。

### 换批和恢复

8. 普通卷已有结构确认后选择第二批学生文件：旧 result、runs 和 confirmations 不显示；第二批重新从 0/当前页数开始。
9. 答题卡已有模板状态和逐页失败后换批：旧失败/结果不显示；第二批重新读取模板并独立处理。
10. 默写逐页请求仍在处理中换批：旧 processing UI 立即离开，新批可独立处理；本批不对旧 Promise 晚到作未来行为断言。
11. 完整 reload 后回上传起点，文件、答案和 session 不伪恢复；不把已落库事实描述为删除。
12. 新增场景继续执行权威写入黑名单，自动化不得触发老师终审、计分、发布、题库手录或知识点写入。

## 明确不在本批断言

- 同一 tick 双击只发一次请求；
- StrictMode setup/cleanup 重放只启动一条处理链；
- 旧 batch/旧照片的晚到 completion 不写回新 scope；
- reset 语义已经纠偏为最终产品策略；
- 完整 reload 可以恢复未完成草稿。

这些是 R3-H1 的目标行为。R3-E1 只能为其建立可控 mock/计数条件，不能加入当前必然失败的断言后修改生产代码来“顺手修复”。

## 必跑验收

1. `python3 -X utf8 -m py_compile scripts/test_exam_fixed_intake_shared_shell_ui.py`。
2. `python3 -X utf8 scripts/test_exam_fixed_intake_shared_shell_ui.py`。
3. `npx tsc --noEmit`。
4. `node --test tests/frontend/examPure.test.ts`。
5. `npm run build`。
6. 十三个现有 R2 UI 脚本全部 Python 编译并逐组运行。
7. `git diff --check`。
8. `FixedIntakeTab.tsx`、HEAD、Git index SHA-256 和 staged 保持不变；展开 porcelain 只允许因本合同新增 1 条。

## 停止条件

- 新增当前行为断言必须修改生产代码才能通过；
- mock DTO 不可能由现有后端产生，或绕过真实按钮/表单路径直接改 React 状态；
- 断言把未修复的 stale response、重复请求或 reload 恢复写成已完成；
- 新场景触发任何权威写命令；
- 非目标生产、测试、暂存区或 HEAD 漂移。

## 完成口径

全部验收通过后，只能记为“R3-E1 固定上传状态与请求特征测试前置通过，R3-S1 纯 `answerSource` reducer 模型可以另立合同”。不得写成生产 reducer/controller 已接线、生命周期风险已修复、完整 R3 已完成、老师已减负或已集成发布。
