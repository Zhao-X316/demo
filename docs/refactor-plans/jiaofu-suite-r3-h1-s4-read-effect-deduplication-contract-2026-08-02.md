---
title: jiaofu-suite R3-H1-S4 read effect deduplication contract
date: 2026-08-02
status: authorized_local_implementation
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_fixed_intake_sha256: 00b13276b83e506dd209ec4a65820ba2268c8581afb0cd63fce96ad84d00d190
source_lifecycle_sha256: a3fd7b901a3d507473d3d801dd717b15c9bb2efa922de473c7c763b56723f6d5
source_root_state_sha256: 8dec762d59f2b3e239054d4b2ecac4c3583d9a4fc8691ea28f047d55efdedf80
source_lifecycle_test_sha256: a70a933043cbbbdda4312624a393fec128b960b8676f7cdfba78649d4fd099e1
source_state_test_sha256: 11db6a504ff1b932918f27bd07657108e764425c8d5d8f851ca0424ec63b64dd
scope: R3-H1-S4-read-effect-deduplication
behavior_change_authorized: lifecycle_safety_only
production_authorized: false
controller_migration_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-H1-S4 读取副作用去重合同

## 1. 本批目标

本批只加固固定上传页现有的三个读取副作用及其直接续处理：

1. `grouping-evidence:<batchId>`；
2. `answer-sheet-template-status:<scopeKey>`；
3. `dictation-template-status:<scopeKey>`；
4. 答题卡模板状态就绪后启动的 `answer-sheet-page:<pageId>`；
5. 默写模板状态就绪后启动的 `dictation-page:<pageId>`。

目标是让 React effect setup→cleanup→setup、相同续处理重入和旧 scope 晚到 completion 都不能重复调用 provider、重复启动逐页处理或写入新 scope。该批不迁移答案分析、材料确认、归组确认、质量确认、重拍、普通卷、模板创建/确认、评分、终审或发布 controller。

## 2. 改前事实

改前三个 effect 都使用局部 `cancelled`：cleanup 会阻止旧 setup 写回，但既不 claim owner-token operation key，也不能把相同 setup 的 provider/continuation 合并为一条链。答题卡与默写逐页函数还可分别由模板状态 effect 和老师确认模板两条路径调用，逐页 provider 调用前没有同步 claim，completion 也没有 `scopeKey + pageId` 门禁。

当前 H1-S1～S3 已具备组件实例级 owner-token registry、`scopeRevision + draft/batch/page/run/scopeKey` identity predicate、根 completion action 与七类输入原子失效。本批只消费这些已冻结原语。

## 3. StrictMode read runner 合同

新增纯 `startFixedIntakeReadEffect` 编排原语：

1. setup 先同步 claim，再发送 started 并调用 provider；claim 失败时不得发送 started、不得调用 provider、不得建立第二条 continuation；
2. effect cleanup 不 release 仍在飞行的 owner，也不单独使本次 owner 失效；是否允许 completion 写回只由组件仍挂载且业务 identity current 决定；
3. provider 成功、失败或同步抛错都必须最终 release；release 后同 key 才可再次读取；
4. current completion 恰好调用一次 success/error 和可选 finished；stale completion 静默丢弃全部 UI 回写及提示，但仍 release；
5. Promise、token 与 cleanup 标记不进入 reducer、SQLite 或日志。

这项合同避免“cleanup 先把 claim 释放导致第二次 provider 调用”，也避免“cleanup 永久压掉唯一 provider 的 completion，第二次 setup 又因 claim 失败而永远不加载”。

## 4. completion 身份与 scopeKey

读取/逐页 completion 必须携带发起时的根 identity，并按场景补充：

| 操作 | 必需业务身份 |
|---|---|
| 归组证据 | `batchId` |
| 答题卡模板状态 | `scopeKey = answer_sheet:<batchId>:<referencePageId>` |
| 默写模板状态 | `scopeKey = dictation:<batchId>:<referencePageId>` |
| 答题卡逐页 | 上述 `scopeKey + pageId` |
| 默写逐页 | 上述 `scopeKey + pageId` |

根状态只在当前材料类型、批次、首个已确认且质检通过的参考页仍能派生出同一 `scopeKey` 时接受模板状态 completion；逐页 completion 还要求 `pageId` 仍属于当前已确认且质检通过的证据页。

## 5. continuation 去重

答题卡/默写逐页处理在调用 provider 前同步 claim 每个 page key。多个入口同时请求同一页时只有第一个 owner 能启动 provider；重复入口跳过已占用页，不将其记为失败。已 claim 但轮到调用前 scope 已失效的页不得再调用 provider，只 release 自己的 owner。

成功、失败和 finished 都走 guarded completion。旧 scope 的异常不弹提示，不能清除新 scope 的 processing，也不能继续启动后续旧页 provider。

## 6. 失败先行测试

生产修改前新增：

1. lifecycle 直接测试：StrictMode cleanup/replay 对同 key provider 调用一次、success/finished 各一次、settle 后释放；
2. lifecycle 直接测试：旧 identity completion 不调用 success/error/finished，但 settle 后释放；
3. 根状态直接测试：模板 `scopeKey` 和逐页 `pageId` 必须同时属于当前材料 scope，错误 scope/page 被静默拒绝。

预期改前 lifecycle 新用例因 runner 不存在而失败，根状态新用例因 current identity 尚不能派生 `scopeKey/pageId` 而失败。不得在测试内放假实现。

## 7. 唯一允许修改

1. 本合同；
2. `fixedIntakeLifecycle.ts` 与其直接测试；
3. `fixedIntakeRootState.ts` 与既有根状态直接测试；
4. `FixedIntakeTab.tsx` 中三个读取 effect、组件挂载门禁和答题卡/默写逐页 continuation；
5. 独立 H1-S4 浏览器测试，只复用既有共享 mock 并注入延迟/同轮重入；
6. 本批验收总览、实施方案、任务地图、项目索引和修改记录队列。

不得修改 API、Rust/SQL/DTO、领域 reducer、共享壳、三材料展示面板、依赖、样式、老师确认/评分/发布语义；不得暂存、提交、推送、tag、合并或发布。

## 8. 必跑门禁

1. `node --test tests/frontend/fixedIntakeLifecycle.test.ts`；
2. `node --test tests/frontend/fixedIntakeState.test.ts`；
3. `node --test tests/frontend/examPure.test.ts`；
4. `npx tsc --noEmit`、`npm run build`；
5. H1 生命周期浏览器目标、固定上传共享壳、十三组既有批改台 UI 回归与学习洞察额外保护；
6. 所有目标/既有 Python 脚本 `py_compile`；
7. `git diff --check`、业务命令清单、HEAD/index/staged 与工作树边界复核。

## 9. 完成口径

通过后只能记录为：“H1-S4 三个读取 effect 与答题卡/默写直接续处理已接同步 owner-token claim 和 scope/page completion guard，本地模拟 StrictMode/旧 completion/重复 continuation 门禁通过。”

不得据此宣称其他异步 controller、真实 OCR/AI、真实图片顺序、老师减负、刷新恢复、独立 `.app`、集成或发布已经验证。下一批只允许 H1-V1 整体验证，不同时启动 C1/C2 或 R4。
