---
title: jiaofu-suite R3-H1-S3 prepare and page-cycle lifecycle connection contract
date: 2026-08-02
status: authorized_local_connection
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_index_sha256: b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b
source_fixed_intake_sha256: 15e942c9c751926a89e37a97072d7497d1f9e25ea6bf5b1ecb81beb45bdf3911
source_upload_form_sha256: 811224fe87e08dca2a3da748330de4a27bf60e9808c0507ecc20d228cb73ab8f
source_root_state_sha256: 3c5e5462a64c342c02b098ca197215ee94f58835f69907b0c55e1f6473e8762c
source_state_sha256: 5fdb7b5761026f3dd8e0bf5b82b8637663a78719203478c62226de1add87a602
source_state_test_sha256: 6b53340262678994eaac5ca9596a8370c6559074e1b7c9abce82520b96db6adb
source_lifecycle_sha256: a3fd7b901a3d507473d3d801dd717b15c9bb2efa922de473c7c763b56723f6d5
source_lifecycle_test_sha256: a70a933043cbbbdda4312624a393fec128b960b8676f7cdfba78649d4fd099e1
source_lifecycle_ui_sha256: 12eb1e54202e5e91f50e335ce5ffab539e01324d3d36c5abdb6288c77a2a1275
scope: R3-H1-S3-input-invalidation-page-cycle-prepare-lifecycle-connection
other_async_controller_connection_authorized: false
effect_change_authorized: false
api_change_authorized: false
backend_change_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-H1-S3 prepare 与页周期生命周期接线合同

## 1. 目标

本批只把 H1-S1/S2 已存在但尚未被生产消费的生命周期能力接到两个入口：

1. 班级、作业、学生试卷、每人页数、答案文件、答案文本和清空答案，均以单个 `FIXED_INTAKE_SCOPE_INVALIDATED` action 原子修改输入、递增 revision 并清空旧处理投影；
2. 学生试卷页周期推断携带选择完成时的 scope/draft identity，旧推断不得覆盖后选文件；当前推断在同一根状态 transition 中写入 `pageCycle` 与推断页数；
3. prepare 在调用 provider 前同步 claim 当前 scope/draft 的 operation key；同一事件轮次重复进入只能有一个 provider 调用；
4. prepare success/finished 使用 guarded completion，切换作业后的旧结果、旧 busy 结束和旧后续分析不得回写当前 scope；
5. operation registry 的 active keys 只投影为字符串数组，Promise、release token 和其他运行时对象不进入 React state。

本批不连接答案分析、分组、质量、普通卷、答题卡或默写的其他异步控制器；这些继续留给 H1-S4 及后续小批。

## 2. 改前事实

- `FixedIntakeUploadForm` 对班级、作业、页数和答案输入同时调用 setter 与额外 reset，一次逻辑输入跨多个 transition；清空答案还连续写文件、文本和 reset。
- `pickStudentPapers` 先写路径、连续 reset 五个域，再直接回写页周期和页数；较早 Promise 可以覆盖后选文件的推断。
- `submit` 在同轮第二次进入时仍能调用 provider；prepare success/finished 直接 dispatch，切换 assessment 后旧 Promise 能重新写入 result。
- H1 红测稳定观测：旧页周期覆盖 `new=0/stale=1`、旧 prepare 结果 1、同轮 prepare provider 调用 2。

## 3. 输入失效接线

上传表单每个逻辑输入只调用一个父级回调；表单不再拥有 `resetRequest` 或 `invalidatePreparedBatch`：

```text
班级             class_selected
作业             assessment_selected
学生文件         student_paths_selected
每人页数         expected_pages_changed
答案文件         answer_file_selected
答案文本         answer_text_changed
清空答案         answer_cleared
```

答案文件与文本互斥、所有旧处理投影清理、revision 递增继续由 H1-S2 根 reducer 一次完成。不得在回调后追加 session/answer/ordinary/answer-sheet/dictation reset。

为覆盖“文件选择器打开期间 scope 已变化”的情况，父组件用纯 reducer 预演同一个 mutation 并同步更新 identity ref，再把同一 action 交给 React reducer；预演不得触发 API、消息或其他副作用。

## 4. 页周期 completion

新增根 action `FIXED_INTAKE_PAGE_CYCLE_RECEIVED`：

```text
identity = 学生路径 mutation 后的 scopeRevision + draftRevision
pageCycle = provider 返回的 PageCycleSuggestion
```

root 先用统一 completion predicate 核验 identity。stale 时返回原 state；current 时在一次 root transition 内复用既有领域 action，同时写：

```text
draft.pageCycle = pageCycle
draft.expectedPages = String(pageCycle.expectedPagesPerAttempt)
```

该机器推断属于同一份学生文件的 completion，不再次递增 revision。页周期调用使用 `page-cycle:<scopeRevision>:<draftRevision>` registry key；新 scope 可与旧 scope 的未决调用并存，但各自 completion 只能写回自身 identity。

## 5. prepare operation 与 completion

prepare 校验通过后，先同步 claim：

```text
prepare:<scopeRevision>:<draftRevision>
```

- claim 失败直接返回，不创建新 idempotency key、不调用 provider、不显示伪错误；
- claim 成功后立即投影 active keys，再 dispatch 既有 `PREPARE_STARTED`；
- success 与 finally 分别包装 `PREPARE_SUCCEEDED`、`PREPARE_FINISHED` 到 `FIXED_INTAKE_COMPLETION_RECEIVED`；
- error 只在 identity 仍为 current 时提示；stale error 静默丢弃；
- 只有 identity 仍为 current 且答案文档数大于 0 时，才允许启动既有答案分析；
- finally 必须幂等 release，并刷新 active key 投影。

operation key 不使用按钮 disabled 或 React state 更新作为互斥依据；同轮两次 DOM click 必须由 registry 同步阻止第二次 provider 调用。

## 6. 红绿测试

先在 `fixedIntakeState.test.ts` 新增根状态用例：

- stale 页周期 completion 返回同一 state；
- current 页周期 completion 同时写 suggestion 与推断页数；
- completion 不改变 scope/draft revision。

该用例在根 action 未实现时必须稳定失败，实现后状态测试目标由 41/41 增至 42/42。

既有 `scripts/test_exam_fixed_intake_lifecycle_ui.py` 不修改，三项必须全部由生产接线自然转绿：

1. 旧页周期不能覆盖后选文件；
2. 旧 prepare 不能回填切换后的作业；
3. 同轮 prepare 重入只调用 provider 一次。

## 7. 唯一允许修改

1. 本合同；
2. `tests/frontend/fixedIntakeState.test.ts`；
3. `src/pages/exam/fixedIntakeRootState.ts`；
4. `src/pages/exam/FixedIntakeTab.tsx`；
5. `src/pages/exam/FixedIntakeUploadForm.tsx`；
6. H1-S3 验收总览、重构标准、文档索引、任务地图与修改记录。

不得修改 `fixedIntakeState.ts`、`fixedIntakeLifecycle.ts`、生命周期测试/浏览器脚本、共享 mock、API、后端、材料面板、依赖、样式、老师终审门禁或业务文案。

## 8. 必跑验收

1. 新根状态测试改前稳定红、改后 42/42；
2. `fixedIntakeLifecycle.test.ts` 5/5、`examPure.test.ts` 6/6；
3. `npx tsc --noEmit`、`npm run build`；
4. H1 浏览器 3/3 PASS，且 authority writes 为 0；
5. 十三个既有 UI 回归与学习洞察额外保护全部通过；
6. `git diff --check`；既有领域 reducer、生命周期 helper/测试/脚本、共享 mock、API、依赖哈希不变；
7. HEAD、Git index 与 staged 保持不变，展开 porcelain 113→114，只增加本合同路径。

## 9. 停止条件与完成口径

若需要修改 provider API、后端、其他异步控制器、useEffect 行为、业务文案或老师门禁，停止并另立合同。

全部验收后只能记录为：“H1-S3 已完成输入原子失效、页周期 stale 丢弃、prepare stale 丢弃与同轮防重接线，H1 三条目标浏览器风险已转绿。”不得写成全部异步控制器、StrictMode effect、真实 AI/OCR、老师减负、集成或发布已完成。
