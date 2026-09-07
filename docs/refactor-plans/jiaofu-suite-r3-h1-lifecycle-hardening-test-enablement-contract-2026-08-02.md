---
title: jiaofu-suite R3-H1 lifecycle hardening test enablement contract
date: 2026-08-02
status: authorized_local_test_enablement
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_fixed_intake_sha256: 5a7e7cc9bff2f48a526aabcccacc98f4981211dd45245956dbdf1ec1481f3132
source_state_sha256: 5fdb7b5761026f3dd8e0bf5b82b8637663a78719203478c62226de1add87a602
source_state_test_sha256: c51687ba4081eef7d36a7920bff3b0be672f701059c143182517c70cbd8f96dc
source_shared_shell_sha256: dcff9b4dc01f5a158129a40992f89fa972703f4abbe1d9e54b105a2c0d00e4a7
scope: R3-H1-lifecycle-hardening-test-enablement
behavior_change_authorized: false
production_authorized: false
controller_migration_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-H1 生命周期加固测试前置合同

## 1. 本批目标

本批只把 R3 设计合同已记录的生命周期风险转成独立、可重复的红测试，并冻结下一批生产实现必须满足的重置矩阵、同步操作占用和过期响应合同。本批不修改 `FixedIntakeTab.tsx`、`fixedIntakeState.ts`、API、Rust、SQL、DTO、展示面板或既有共享壳。

新增红测试在 H1 生产实现前预期非零退出；它们暂不并入既有绿色回归门禁。既有 `fixedIntakeState`、`examPure`、共享壳和十三个 UI 回归必须继续为绿，防止把“新测试如期失败”误写成项目整体通过。

## 2. 改前事实

当前生产实现有以下可验证空档：

1. 选择新学生照片后，页周期推断只按 Promise 完成顺序写回；旧照片推断晚到时可以覆盖新照片的 `pageCycle/expectedPages`。
2. prepare 捕获发起时的输入，但完成事件不携带 `scopeRevision` 或业务身份；老师切换作业后，旧 prepare 仍可把旧批次结果写入新上下文。
3. React `busy` 只承担 UI 投影，没有同步 operation registry；同一事件循环、effect 重放或重复 continuation 缺少统一的同步 claim/release 机制。
4. `resetRequest()` 与“选择新学生照片”的手写 reset 范围不同；若干 `busy`、processing、template 与普通卷确认投影不是由一个原子生命周期事件清理。
5. grouping evidence、答题卡模板状态、默写模板状态 effect 仅以局部 `cancelled` 阻止 UI 写回，不拥有跨 setup/cleanup/setup 的同步请求去重键。

以上只说明当前存在未加固边界，不说明已经发生正式成绩、学生归属或发布数据污染。H1 继续禁止任何自动老师确认与自动发布。

## 3. 原子失效矩阵

H1 后续生产批必须把下列动作视为一个 scope 失效事件。`scopeRevision` 每次恰好递增 1；所有旧 completion 均因 revision 不匹配而被忽略。

| 动作 | context | draft | batch/答案结构/归组/质量/三材料投影 | 进行中操作 |
|---|---|---|---|---|
| 切换班级 | 写新班级；作业由合法 options 校准 | 暂按 R3-E1 保留照片、答案、固定页数和页周期 | 全清 | 当前 scope 全部失效；旧 Promise 可结束但不可写回 |
| 切换作业 | 写新作业版本 | 暂按 R3-E1 保留 | 全清 | 同上 |
| 选择新学生照片 | context 不变 | 写新路径；立即清空旧 `pageCycle`，固定页数先保留到新推断成功 | 全清 | 旧页周期与旧批次全部失效 |
| 修改固定页数 | context 不变 | 只写固定页数 | 全清 | 旧 prepare 与后续链失效 |
| 选择/修改/清除答案 | context 与学生照片不变 | 保持文件/文本互斥并写新值 | 全清 | 旧 prepare、答案结构与后续链失效 |
| options 等值刷新 | 不改仍合法的选择 | 全保留 | 全保留 | 不递增 revision |
| 完整 WebView 刷新 | 从合法默认 context 重新开始 | fail-closed 回上传起点 | 不从前端存储恢复 | 不触发老师写入 |

本合同不在 H1 中擅自改变“切换班级/作业时保留草稿”的 R3-E1 可见行为。若以后为防误投决定清空照片或答案，必须另立 UX 行为变更合同和迁移提示。

“全清”是 UI 投影范围，不删除已落库批次、AI run、OCR、人工评分 revision 或发布 revision。旧后端事实继续从终审工作台读取。

## 4. 同步 operation registry 合同

H1 后续生产实现必须提供组件实例级、非序列化的 operation registry。它只防前端重复发起，不替代后端幂等、事务或 run 状态。

要求：

1. `claim(key)` 在发起 provider 调用前同步执行；同一 key 已占用时立即拒绝第二次调用。
2. 成功、失败与抛错都在 Promise `finally` 释放；React effect cleanup 不能提前释放仍在飞行的请求。
3. release 必须带本次 claim 的所有权 token；旧 release 重放不得释放后来同 key 的新 claim。
4. registry 可提供 active key 的只读 UI 投影，但 Promise、AbortController 和 token 不进入 reducer、SQLite 或日志。
5. StrictMode 的 setup→cleanup→setup 对相同读取 key 只能产生一次 provider 调用与一条 continuation 链。

冻结操作键：

```text
cycle:<draftRevision>
prepare:<requestKey>
answer-source:<batchId>:<mode>
answer-resolution:<batchId>:<runId>
material:<batchId>
grouping:<batchId>
grouping-evidence:<batchId>
quality:<batchId>
retake:<batchId>:<pageId>
ordinary-analyze:<pageId>:<mode>
ordinary-confirm:<pageId>:<runId>
answer-sheet-template-status:<scopeKey>
answer-sheet-template:<scopeKey>:<runId>
answer-sheet-page:<pageId>
dictation-template-status:<scopeKey>
dictation-template:<scopeKey>:<runId>
dictation-page:<pageId>
```

## 5. completion 身份合同

每个异步命令在发起前捕获至少 `scopeRevision`；按业务需要再捕获 `draftRevision`、`batchId`、`pageId`、`runId` 或 `scopeKey`。completion 只有同时满足以下条件才可更新 UI：

1. `scopeRevision` 与当前 scope 一致；
2. 其业务身份仍属于当前 draft/batch/page/run；
3. 当前 action 对应的 operation token 仍有效；
4. 结果类型、后端 reason code 和老师确认门禁仍通过原校验。

不匹配 completion 静默丢弃 UI 写回并正常 release 自己的 operation；不得弹出属于旧 scope 的成功或失败提示，不得清除新 scope 的 busy，也不得触发答案分析或三材料逐页 continuation。

## 6. 测试分层与预期

### 6.1 纯生命周期合同测试

独立文件 `tests/frontend/fixedIntakeLifecycle.test.ts` 冻结：

- 同 key 同步 claim 只成功一次；
- release 后可重新 claim；
- 旧 release 重放不能释放新 owner；
- StrictMode cleanup 不提前释放在飞行 read；
- scopeRevision 或业务身份任一不匹配时 completion 判为 stale。

生产 helper 尚不存在时，该测试必须稳定红；不能用测试内假实现把它伪装为绿。

### 6.2 浏览器风险复现

独立文件 `scripts/test_exam_fixed_intake_lifecycle_ui.py` 使用既有共享 mock 的只读扩展，至少逐项报告：

- 第一组照片页周期晚到，不能覆盖第二组照片的新建议；
- 旧作业 prepare 晚到，不能在切换后的作业上下文显示旧批次；
- 同步重复 prepare 只能调用 provider 一次（若浏览器事件层当前已阻断，该项可先绿，但 operation registry 纯测试仍必须红）。

脚本必须汇总所有案例后非零退出，避免只看到第一处失败；不得触发终审、评分、发布或题库权威写命令。

## 7. 唯一允许修改

1. 本合同；
2. 新增独立纯生命周期红测试；
3. 新增独立浏览器生命周期红测试；
4. 测试前置验收总览、重构实施方案、任务地图、项目索引与修改记录队列。

不得修改生产代码、既有测试、共享 mock、依赖、样式、API、Rust/SQL/DTO；不得暂存、提交、推送、打 tag、合并或发布。

## 8. 必跑证据

1. 改前/改后既有门禁：`node --test tests/frontend/fixedIntakeState.test.ts` 35/35、`node --test tests/frontend/examPure.test.ts` 6/6、`npx tsc --noEmit`、共享壳 PASS。
2. 新纯生命周期测试：预期因生产 helper 未存在而红，并保存完整错误。
3. 新浏览器生命周期测试：逐案例保存 PASS/RED 与实际调用数/可见状态；整体预期红。
4. 新脚本 `py_compile` 通过；`git diff --check` 通过。
5. `FixedIntakeTab.tsx`、`fixedIntakeState.ts`、共享壳、API、三材料面板、package/tsconfig 哈希保持不变。
6. HEAD、Git index 与 staged 保持不变。

## 9. 后续生产拆批

红测试证据成立后，H1 生产实现仍需分批：

1. H1-S1：纯 operation registry 与 scope identity helper；
2. H1-S2：workflow control slice、原子失效与 completion guard；
3. H1-S3：prepare、页周期和用户动作同步占用接线；
4. H1-S4：三个 read effect 与 continuation 去重；
5. H1-V1：红转绿、全门禁、独立应用验收边界复核。

不得在任一 H1 子批同时把 23 个请求搬入 controller，也不得启动 R3-C1/C2 或 R4。

## 10. 完成口径

本批完成后只能记录为：“R3-H1 生命周期风险已被独立红测试稳定复现，目标矩阵与下一批实现合同已冻结；生产生命周期仍未加固。”不得写成重复请求、过期响应、StrictMode、真实 AI/OCR、老师减负、集成或发布已解决。
