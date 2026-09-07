---
title: jiaofu-suite R3-C1-S4 ordinary command extraction contract
date: 2026-08-02
status: authorized_for_implementation
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_index_sha256: b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b
source_porcelain_count: 144
source_fixed_intake_sha256: bcc18a9fdf56895c2f5ffc13a93714f4161957cd073d82f679dfc9bace7f0daf
source_root_controller_sha256: f5d54d69b7f9df26401e0eca88d81bf60fb8eb7bdb6db5061270ace21ca857cb
source_grouping_quality_sha256: 7368c1c32473a3f6e4943a4b4f4fe727b070404531ee3ec45e521c1d51e7dfa1
source_lifecycle_audit_sha256: fabb796b15de6bc60923a88e0fb7f9d87e3b4e94e7ce6565e49bb84802c66c9b
source_controller_audit_sha256: b7f2d20cd08dfd5c3128da7af77235fdcbf0ecd14620368497b1fc5f5efba389
source_business_command_count: 23
source_provider_callsite_count: 26
source_business_command_sha256: 375a66bfade8b24a2d2ffb180bb934494754e4d5cff25401bd04b8e7e4358ba4
scope: R3-C1-S4-ordinary-command-extraction
production_change_authorized: true
provider_move_authorized: four_calls_only
legacy_continuation_removal_authorized: true
other_command_modules_authorized: false
c2_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-C1-S4 普通卷命令迁移合同

## 1. 唯一目标

本批只新增 `fixedIntakeOrdinaryCommands.ts`，机械迁移：

- `analyzeOrdinaryPages`：筛选已确认且质量通过页面，串行调用普通卷结构分析并保留逐页部分成功；
- `confirmReadyOrdinaryPages`：串行确认 ready 页面结构，题库同步失败只记录旁路异常，继续逐题区识别。

精确移动 4 个 provider 调用点：普通卷页面分析 1、结构确认 1、题库同步 1、客观题区识别 1。其余 9 个答题卡/默写 provider、两个模板读取 effect 和对应命令均留在 Tab。

## 2. 组合与 S3 过渡消除

- 根 hook 先创建 ordinary factory，再把其 `analyzeOrdinaryPages` 注入 grouping-quality factory；两个命令模块不得互相 import。
- 删除根 hook 的第三个 `legacyContinuations` 参数、`GroupedPageEvidence` 临时类型 import，以及 Tab 调用根 hook时传入的 `{ analyzeOrdinaryPages }`。
- grouping-quality 的 continuation 参数名、调用位置和行为不改，只把来源从 Tab 提升函数切换为 ordinary factory 返回动作。
- ordinary factory 返回 `analyzeOrdinaryPages`、`confirmReadyOrdinaryPages`，并由根 hook 暴露到 actions；Tab 只消费动作。

## 3. 不可变业务合同

1. 23 个命令、26 个调用点和规范 hash 不变；
2. H1 17/17、26/26、`ordinary-analyze:` / `ordinary-confirm:` owner key、claim 时机、batch/page/run identity 与 stale guard 不变；
3. 普通卷分析按页面顺序串行；retry key 继续使用每次新 UUID，初次 key 继续固定 `structure:v1`；
4. 分析失败只加入 failures，其余页面继续；完成 action 和 release 时机不变；
5. 结构确认后仍先写 confirmation，再尝试题库同步，再逐题区识别；题库失败绝不阻断当前作业识别；
6. `pageLoop` 的 stale break、计数、部分成功、错误/成功文案与 `onDone` 触发条件不变；
7. API 参数/次数/顺序、reducer/runtime/view model、面板 props、Rust/SQL/DTO/样式不改；
8. C2 语义动作改名不混入。

## 4. 精确允许修改

仓库内仅：

1. 本合同；
2. 新增 `src/pages/exam/fixedIntakeOrdinaryCommands.ts`；
3. 修改 `src/pages/exam/useFixedIntakeController.ts`；
4. 修改 `src/pages/exam/FixedIntakeTab.tsx`。

仓库外只允许写 S4 回执与权威索引/进度/修改记录。不得改测试、H1/C1 审计器、已完成命令模块、package/依赖或其他生产文件；不得暂存、提交、tag、合并、推送或发布。

## 5. 验收

必须取得：

1. H1 跨五文件仍为 17/17、26/26、exit 0；
2. C1 库存 23/26/原 hash 且无 inventory error；
3. ordinary 不超过 400 行、provider=4、无命令模块 import；
4. Tab provider 13→9、UUID 3→2、ordinary handler 全部移除；
5. 根 hook provider=0、无 legacy continuation 参数，并组合 ordinary 后注入 grouping-quality；
6. 65/65、TypeScript、Vite、Python 编译、27/27 UI 与 whitespace 通过；
7. HEAD/index/staged 与 S3 受保护文件不漂移；
8. 展开 porcelain 144→146，只新增本合同与 ordinary 文件。

本批不证明 C1 完成、真实 provider/材料、老师减负、独立 `.app`、集成或发布。下一批只能 C1-S5 迁答题卡模板/effect/学生页命令。
