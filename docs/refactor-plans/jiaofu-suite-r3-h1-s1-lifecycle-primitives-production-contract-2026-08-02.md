---
title: jiaofu-suite R3-H1-S1 lifecycle primitives production contract
date: 2026-08-02
status: authorized_local_pure_helper_implementation
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_index_sha256: b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b
source_h1_contract_sha256: b0a9b8a5ffee6c5f0780c78d24bfa02eb0c42646d1d6e0075fdd0884f2f5c56b
source_lifecycle_test_sha256: a70a933043cbbbdda4312624a393fec128b960b8676f7cdfba78649d4fd099e1
source_lifecycle_ui_sha256: 12eb1e54202e5e91f50e335ce5ffab539e01324d3d36c5abdb6288c77a2a1275
scope: R3-H1-S1-pure-lifecycle-primitives
production_consumer_authorized: false
reducer_change_authorized: false
controller_migration_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-H1-S1 纯生命周期原语生产合同

## 1. 目标

新增一个无 React、无 Tauri、无 API、无时间、无随机数、无持久化副作用的纯 TypeScript 模块，为后续 H1 接线提供：

1. owner-token operation registry；
2. completion identity 当前性判定。

本批只让 `tests/frontend/fixedIntakeLifecycle.test.ts` 的五个目标测试从 `ERR_MODULE_NOT_FOUND` 转绿。helper 暂无生产消费者，因此本批不声称旧页周期覆盖、旧 prepare 回填、同轮双调用、reset 不一致或 StrictMode 已修复。

## 2. 改前红证据

`node --test tests/frontend/fixedIntakeLifecycle.test.ts` 当前以 `ERR_MODULE_NOT_FOUND` 非零退出，缺少 `src/pages/exam/fixedIntakeLifecycle.ts`。浏览器 H1 脚本已稳定观测：

- 新 3 页建议被旧 2 页结果覆盖：`new=0, stale=1`；
- 切换作业后旧 prepare 结果显示 1 次；
- 同一轮 prepare provider 调用 2 次。

既有状态测试 35/35、纯函数 6/6、TypeScript、Vite 81 modules 与十三组 UI 回归为绿。

## 3. operation registry 契约

公开工厂：

```text
createFixedIntakeOperationRegistry()
  claim(key) -> release | null
  activeKeys() -> string[]
```

规则：

1. `claim` 必须同步完成；key 已存在返回 `null`，不替换 owner。
2. 首次 claim 为该 key 建立不可伪造的 owner token，并返回只释放该 owner 的闭包。
3. release 幂等；同一闭包重复调用无副作用。
4. 旧 owner 的 release 即使重放，也不能删除同 key 的新 owner。
5. `activeKeys()` 返回快照数组，不泄露内部 Map/Set；顺序使用当前 Map 插入顺序，便于稳定 UI 投影和测试。
6. registry 只存在于组件实例内存；不序列化 Promise、token 或 key 到 reducer、SQLite、日志或诊断包。
7. effect cleanup 不自动等于 release；调用方必须在请求 Promise `finally` 执行 release。

## 4. completion identity 契约

公开类型：

```text
FixedIntakeCompletionIdentity
  scopeRevision: number
  draftRevision?: number
  batchId?: number
  pageId?: number
  runId?: number
  scopeKey?: string
```

公开判定：

```text
isFixedIntakeCompletionCurrent(expected, current) -> boolean
```

规则：

1. `scopeRevision` 必须相等；
2. `expected` 中提供的每个可选业务身份必须与 `current` 相等；
3. `expected` 未提供的可选字段不参与判定；
4. 函数不修改任何输入，不读取全局状态，不生成新 revision；
5. 调用者未来仍须在 reducer/action 层执行对应事实门禁，helper 不替代业务校验。

## 5. 唯一允许修改

1. 本合同；
2. 新增 `src/pages/exam/fixedIntakeLifecycle.ts`；
3. H1-S1 验收回执、重构实施标准、文档索引、任务地图与修改记录。

`tests/frontend/fixedIntakeLifecycle.test.ts` 是已冻结目标，本批不得为迎合实现修改断言。不得修改 `FixedIntakeTab.tsx`、`fixedIntakeState.ts`、浏览器 H1 脚本、共享壳、API、三材料面板、Rust/SQL/DTO、依赖或样式。

## 6. 必跑验收

1. 改前目标测试为 `ERR_MODULE_NOT_FOUND`；改后 5/5 PASS。
2. `node --test tests/frontend/fixedIntakeState.test.ts` 35/35 与 `node --test tests/frontend/examPure.test.ts` 6/6。
3. `npx tsc --noEmit`、`npm run build`。
4. H1 浏览器脚本仍按三项现状 RED，证明纯 helper 尚未偷接生产。
5. 十三个既有 UI 脚本语法与浏览器回归、学习洞察额外回归全部为绿。
6. `git diff --check`；受保护生产/测试/API/依赖哈希不变。
7. HEAD、Git index 与 staged 不变。

## 7. 停止条件

- 需要把 helper 接入 React、reducer、effect 或 API；
- 需要改目标测试、共享壳或浏览器红测试才能通过；
- 需要新增依赖、计时器、随机数、AbortController 或持久化；
- 任一既有绿色门禁失败，或三项浏览器风险意外转绿；
- 暂存区、HEAD 或 Git index 变化。

## 8. 完成口径

全部验收后只能记录为：“R3-H1-S1 两个纯生命周期原语已实现并通过 5/5 直接测试，但尚无生产消费者，三项浏览器风险仍存在。”下一批只允许 H1-S2 control slice、原子失效与 completion guard 纯状态设计/接线，不得跳到 prepare/effect/controller。
