---
title: jiaofu-suite R3-C1-S1 runtime and view model extraction contract
date: 2026-08-02
status: authorized_for_implementation
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_index_sha256: b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b
source_porcelain_count: 134
source_fixed_intake_sha256: 2de6b5e82f588284a126fbf84aa81c2f398d6c2689558d41642c107d668f224a
source_fixed_intake_lines: 1644
source_runtime_sha256: missing
source_root_controller_sha256: missing
source_view_model_sha256: missing
source_controller_test_sha256: missing
source_controller_audit_sha256: 24be2b5334af94f167afdf15f82526bb488480a338a6a4bae2d7ef7147cc1dd4
source_lifecycle_audit_sha256: 8454eb2159567d719e5560e5a5744d23fa4a47e4b5174653bc52b2e9111c7a22
source_business_command_count: 23
source_provider_callsite_count: 26
source_business_command_sha256: 375a66bfade8b24a2d2ffb180bb934494754e4d5cff25401bd04b8e7e4358ba4
scope: R3-C1-S1-runtime-options-view-model-extraction
production_change_authorized: true
test_change_authorized: true
provider_move_authorized: false
command_module_creation_authorized: false
c2_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-C1-S1 Runtime、options 校准与纯 view model 提取合同

## 1. 唯一目标

本批只从 `FixedIntakeTab.tsx` 提取三类无 provider 职责：

1. 根 reducer/ref/mounted/operation registry/scope mutation 运行时；
2. 班级和作业 options 校准及现有 set/clear 动作；
3. 面板所需的纯派生 view model。

新增一个尚不组合领域命令的 `useFixedIntakeController` 根入口，让 Tab 恰好调用一次；23 个业务命令和 26 个 provider 调用点本批全部留在 Tab，调用参数、顺序、owner、identity、文案和 UI props 均不变。

## 2. 测试先行

生产修改前新增 `tests/frontend/fixedIntakeController.test.ts`，锁定：

- 班级 options 按首次出现顺序去重，同 ID 名称保持旧实现的最后值覆盖；
- assessment options 只保留当前班级且保持原顺序；
- 初始 view model 的 route、归组候选、三类进度计数和材料 scope 均为当前语义；
- 输入 state 不被纯派生函数修改。

首次运行必须因 `fixedIntakeViewModel.ts` 尚不存在而非零退出；不允许通过删断言或降低预期转绿。

## 3. 允许新增的生产边界

### 3.1 `fixedIntakeControllerRuntime.ts`

只允许持有：

- `useReducer`、state ref、mounted ref；
- operation registry 创建、claim/release 与 active key 投影；
- scope mutation 的同步 ref 前移；
- continuation 前的同步 reducer/ref 前移；
- completion identity 当前性判定。

不得导入或调用任何 `exam*` provider、Tauri dialog、UUID、面板或 JSX；不超过 300 行。

### 3.2 `fixedIntakeViewModel.ts`

只允许纯函数：

- options 去重与筛选；
- state slice 展平；
- route label、归组候选；
- eligible page、模板 target/scope；
- 普通卷、答题卡、默写统计。

不得出现 React hook、provider、dispatch、ref、随机值或副作用；不超过 300 行。

### 3.3 `useFixedIntakeController.ts`

本批只组合 runtime、options 校准、scope actions 和 view model。不得导入 provider、Tauri dialog、UUID、展示面板或 JSX；不超过 350 行。六个领域命令模块留待 S2～S6，根 hook 当前不要求假装已组合它们。

## 4. Tab 本批允许变化

`FixedIntakeTab.tsx` 只允许：

- 删除已迁移的 `useMemo/useReducer/useRef`、runtime helper、scope setter、options 校准和纯派生实现；
- 调用一次 `useFixedIntakeController(options)` 并消费 `runtime/view/actions`；
- 保留现有三个 provider read effect 和全部 26 个 provider 调用；
- 保留所有现有 JSX、面板 props、按钮、空态和文案。

因此 C1 审计本批仍应非零：Tab provider/dialog/UUID/effect/dispatch 和命令文件缺失都属于后续批次目标红，不得误写成回归失败。

## 5. 永久不变量

1. 业务命令精确 23 个、provider 调用点精确 26 个、规范 hash 不变；
2. H1 生命周期审计始终 17/17 operation、26/26 callsite；
3. operation key、claim/release、completion identity 和 stale guard 不变；
4. API 参数、调用次数、顺序、幂等键和错误/成功文案不变；
5. reducer/domain state/API/Rust/SQL/DTO/样式/面板 props 不改；
6. `FixedIntakeTab.tsx` 的 provider owner 本批不移动；
7. 不得新增 localStorage、前端持久化或刷新恢复假象。

## 6. 精确允许修改

仓库内仅：

1. 本合同；
2. 新增 `tests/frontend/fixedIntakeController.test.ts`；
3. 新增 `src/pages/exam/fixedIntakeControllerRuntime.ts`；
4. 新增 `src/pages/exam/fixedIntakeViewModel.ts`；
5. 新增 `src/pages/exam/useFixedIntakeController.ts`；
6. 修改 `src/pages/exam/FixedIntakeTab.tsx`。

仓库外只允许写 C1-S1 验收总览、实施标准、文档索引、任务地图和修改记录。不得修改既有测试断言、H1/C1 审计器、package 或依赖；不得暂存、提交、tag、合并、推送或发布。

## 7. 验收

必须取得：

1. 新测试 failure-first 后转绿；
2. 全部直接测试由 63 增至实际新总数且全绿；
3. C1 审计库存 23/26/hash 无 `inventory_error`，边界通过项较 1/19 单调增加；
4. 根 hook 恰好一次、root/runtime/view model 行数与 purity 门禁通过；
5. H1 审计 17/17、26/26；
6. `npx tsc --noEmit`、Vite build、Python UI 编译与全部 27 组 UI 通过；
7. `git diff --check` 通过；
8. HEAD/index/staged 与所有冻结源文件哈希不漂移；
9. 展开 porcelain 134→139，仅新增本合同、测试和三个 production 文件，Tab 仍是既有脏路径。

本批不证明 C1 终态、真实 provider、老师减负、独立 `.app`、集成或发布。下一批只能另立 C1-S2 合同，移动 upload/prepare 与答案资料命令。
