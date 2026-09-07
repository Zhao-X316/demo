---
title: jiaofu-suite R3-V1 rubric mapping validation alignment contract
date: 2026-08-02
status: authorized_local_behavior_alignment
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_fixed_intake_state_sha256: 89613bed9aa74470391fa41c213b408f7d9564317cd42056e8faa3dd1b805750
source_fixed_intake_test_sha256: a82018b9280b58596bab49cabfd7d5e2bc0382656c0645b3944a7e92770bda0d
source_fixed_intake_ui_sha256: 1e588651b004fc54210a545accae126f704a4f10dfd915453bcd8d45c8784003
source_rust_answer_source_sha256: 9602f1fe2880298432e828f5fba0a27848c92ae489c39744a7eab1d68b8df988
scope: R3-V1-duplicate-rubric-mapping-front-end-gate
behavior_change_authorized: duplicate-reuse-only
integration_authorized: false
release_authorized: false
---

# R3-V1 重复评分点映射前端门禁对齐合同

## 目标

修正 `rubricPointMappingsReady` 中唯一已确认的前端校验缺口：同一道简答题的多个上传评分点不得重复沿用同一个旧评分点。前端按钮应在老师作出重复选择时立即禁用，Rust 服务继续保留事务级最终拒绝与回滚保护。

## 改前证据

1. selector 当前使用 `!reused.add(selected)` 判断重复；JavaScript `Set.add()` 返回 Set 本身而非“是否新增”的布尔值，因此条件永远不会因重复旧 ID 变为 true。
2. 直接测试明确冻结了错误现状：`old-0 / old-0 / new` 返回 ready。
3. “采用答案与评分点，另存新版本”按钮直接以该 selector 的反值作为 `disabled`，所以重复选择仍可点击并发出请求。
4. Rust `validate_short_answer_candidate` 使用 `BTreeSet::insert` 正确拒绝重复旧 `stable_id`，错误为“同一个旧评分点不能对应多个新评分点”。
5. Rust 专项 `incomplete_or_duplicate_teacher_mapping_rolls_back_every_new_version` 改前通过 1/1，并验证拒绝后作业版本、rubric 版本、采用账本和映射账本保持 `(1,1,0,0)`。
6. R3-S2 已完成 reducer 机械接线；业务命令、老师门禁和十三组浏览器保护网已通过。R3-V1 不重新迁移任何 state。

## 施工快照

- 分支：`codex/t2-artifacts`
- HEAD：`6072360049b09c3155726773039da139834294fe`
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`
- 展开 porcelain：100 条；staged：0
- `fixedIntakeState.ts` SHA-256：`89613bed...5750`
- 直接测试 SHA-256：`a82018b9...a0d`
- 共享壳 UI 脚本 SHA-256：`1e588651...4003`
- `FixedIntakeTab.tsx` SHA-256：`7fb2e00e...df46`
- `FixedIntakeAnswerSourcePanel.tsx` SHA-256：`cb68d502...cc2c`
- `package.json` SHA-256：`a3f0afa7...69b0`
- `tsconfig.json` SHA-256：`e09b1d73...f906`

## 唯一允许修改

1. 本合同。
2. `src/pages/exam/fixedIntakeState.ts`：只修正 `rubricPointMappingsReady` 的重复旧点评分映射判断。
3. `tests/frontend/fixedIntakeState.test.ts`：把错误现状拆成明确的允许/拒绝测试。
4. `scripts/test_exam_fixed_intake_shared_shell_ui.py`：只在既有答案冲突场景加入重复选择时按钮禁用、恢复唯一选择后重新启用和非法状态零采用请求的浏览器断言。
5. 完成后的验收总览、重构标准、文档索引、任务地图和修改记录。

不得修改 `FixedIntakeTab`、答案面板、API、Rust/SQL/DTO、样式、文案、其他状态域、依赖或其他 UI 脚本。

## 正确规则

对每一道 `short_answer + conflict`：

1. 每个上传评分点都必须有映射；空值为 not ready。
2. `NEW_RUBRIC_POINT` 可被多个上传评分点共同选择，因为每次都代表建立不同的新点评分身份。
3. 非 `NEW_RUBRIC_POINT` 的旧 `stable_id` 在同一道题内最多出现一次；第二次出现立即 not ready。
4. 不同题目的旧点评分身份在各自题目内独立校验，不能跨题误判重复。
5. selector 只决定前端就绪/按钮禁用，不生成映射、不调用 API、不改变老师选择。

实现应显式使用 `has` 再 `add`，不得继续依赖 `Set.add()` 的返回值表达布尔语义。

## 改前失败特征

在修改生产 selector 前，先使以下新期望失败：

- 直接测试：`old-0 / old-0 / NEW_RUBRIC_POINT` 必须返回 false；改前实际为 true。
- 浏览器：把第二个下拉改为第一个旧点后，采用按钮必须 disabled 且 `exam_answer_source_adopt_new_version` 调用数为 0；改前按钮仍 enabled。

失败只用于证明测试能命中缺口；随后立即修正 selector并恢复全绿。

## 必跑验收

1. 改前红灯：直接测试和目标共享壳场景均能命中重复映射缺口。
2. `node --test tests/frontend/fixedIntakeState.test.ts`，预期 10/10。
3. `cargo test -p module-exam incomplete_or_duplicate_teacher_mapping_rolls_back_every_new_version -- --nocapture`，预期 1/1。
4. `node --test tests/frontend/examPure.test.ts`，预期 6/6。
5. `npx tsc --noEmit`。
6. `npm run build`。
7. 十三个 R2/R3 UI 脚本全部 `py_compile` 并逐组运行。
8. `git diff --check` 与目标文件尾随空白扫描。
9. 业务命令清单、`FixedIntakeTab`、答案面板、Rust 服务、`package.json`、`tsconfig.json`、HEAD、Git index 和 staged 保持不变。
10. 展开 porcelain 只允许因本合同新增 1 条；其余两个修改文件已在当前用户工作树中存在。

## 停止条件

- 需要修改按钮 JSX、API、Rust、错误文案、映射 DTO 或老师确认流程；
- 多个 `NEW_RUBRIC_POINT` 被误判为重复；
- 跨题目的旧 stable ID 被错误视为同一道题内重复；
- 非目标业务命令、状态、UI 场景或工作树保护项漂移；
- 任一直接测试、Rust 最终防线、TypeScript、构建或浏览器回归失败。

## 完成口径

全部验收通过后，只能记为“R3-V1 前端重复评分点映射门禁与 Rust 既有最终拒绝语义对齐”。不得写成评分点导入已全面验证、并发/生命周期风险已解决、真实答案资料已验证、老师已减负、已提交、已集成或已发布。
