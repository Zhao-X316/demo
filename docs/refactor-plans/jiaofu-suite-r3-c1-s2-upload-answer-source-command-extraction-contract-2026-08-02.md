---
title: jiaofu-suite R3-C1-S2 upload and answer source command extraction contract
date: 2026-08-02
status: authorized_for_implementation
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_index_sha256: b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b
source_porcelain_count: 139
source_fixed_intake_sha256: 1352a5f610ebb5d106fb144e54521d351b354364f9a1956f18b6ce0faf2e5209
source_root_controller_sha256: 24b609122be2bd67b909c900fd210119f8ec3feebd7a3d38ca98e4dcf81ed2d2
source_runtime_sha256: 72021417991117f940581966c900b310de91c82d6976cf0db575361a45e908fd
source_view_model_sha256: f5f8171f9caea6220d4c61a37c52c1bc1321bb5fd91719a1debe49f27f00492f
source_controller_test_sha256: 71a750d00a1f5c9a20524fbf03f02f90e7cadc098827cb19c3fc77869056b1fb
source_lifecycle_audit_sha256: 8454eb2159567d719e5560e5a5744d23fa4a47e4b5174653bc52b2e9111c7a22
source_controller_audit_sha256: 24be2b5334af94f167afdf15f82526bb488480a338a6a4bae2d7ef7147cc1dd4
source_business_command_count: 23
source_provider_callsite_count: 26
source_business_command_sha256: 375a66bfade8b24a2d2ffb180bb934494754e4d5cff25401bd04b8e7e4358ba4
scope: R3-C1-S2-upload-prepare-answer-source-command-extraction
production_change_authorized: true
test_change_authorized: false
lifecycle_audit_migration_authorized: true
provider_move_authorized: six_calls_only
other_command_modules_authorized: false
c2_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-C1-S2 Upload/prepare 与答案资料命令迁移合同

## 1. 唯一目标

本批只把以下现有命令从 `FixedIntakeTab.tsx` 机械迁入两个普通 TypeScript factory，并由根 hook 组合：

### `fixedIntakeUploadCommands.ts`

- `pickStudentPapers`：文件选择、扩展名检查、页周期 claim/infer/guarded projection；
- `pickAnswer`：答案文件选择与扩展名检查；
- `submit`：输入门禁、prepare claim、幂等 key、provider、guarded completion 与答案分析 continuation。

### `fixedIntakeAnswerSourceCommands.ts`

- `analyzeAnswerSource`；
- `confirmMatchingAnswerSource`；
- `keepCurrentBoundAnswers`；
- `adoptAnswerSourceAsNewVersion`；
- `changeRubricPointMapping`。

本批只移动 6 个 provider 调用点：页周期、prepare、答案分析与三类答案处置。其他 20 个调用点、三个读取 effect、归组质量、普通卷、答题卡、默写全部留在 Tab。

## 2. 审计器迁移先行

生产移动前，先机械修改 `scripts/audit_exam_fixed_intake_lifecycle.mjs`：

- 扫描 Tab 及六个命令目标中实际存在的文件；
- 每个 AST 使用自己的 source file 定位 owner/行号；
- operation/provider 预期清单、owner 名、key 前缀、claim/guard 判据和退出条件不改；
- 移动前必须仍输出 17/17、26/26、23 个命令并退出 0；
- 移动后必须跨 Tab/upload/answer-source 仍输出相同结果。

该审计器修改只恢复迁移后的观察能力，不得放松判据、删 provider 预期或通过跨文件字符串拼接伪造 owner 保护。

## 3. Factory 依赖与顺序

- 两个 factory 均只接收 runtime、当前 view 投影和必要回调；不是 React hook，不持模块级状态。
- answer-source factory 先由根 hook创建；upload factory 再注入其 `analyzeAnswerSource` continuation。
- factory 之间不得互相 import；upload 不得直接 import answer-source 模块。
- `submit` 仍仅在 prepare 返回且 identity current、答案文档数大于 0 时异步启动答案分析。
- 所有函数名保持不变，确保 H1 owner 合同和现有面板回调语义不漂移。

## 4. 不可变业务合同

1. 23 个唯一业务命令、26 个 provider 调用点及规范 hash 不变；
2. H1 17 个 operation、key 前缀、claim 时机、identity 和 stale guard 不变；
3. API 参数、次数、串行顺序、幂等 key 文本和 retry UUID 时机不变；
4. 文件扩展名白名单、错误/成功文案和老师门禁逐字不变；
5. prepare 失败继续复用 request key；输入失效/成功仍由原 reducer 清理；
6. rubric 映射缺失/重复的前端拒绝和 Rust 兜底不变；
7. state/reducer/runtime/view model/API/Rust/SQL/DTO/面板 props/样式不改；
8. C2 命名和语义动作重写不混入。

## 5. 精确允许修改

仓库内仅：

1. 本合同；
2. 新增 `src/pages/exam/fixedIntakeUploadCommands.ts`；
3. 新增 `src/pages/exam/fixedIntakeAnswerSourceCommands.ts`；
4. 修改 `src/pages/exam/useFixedIntakeController.ts`；
5. 修改 `src/pages/exam/FixedIntakeTab.tsx`；
6. 修改 `scripts/audit_exam_fixed_intake_lifecycle.mjs`，仅作多文件扫描适配。

仓库外只允许写 S2 回执、实施标准、文档索引、任务地图和修改记录。不得改测试断言、C1 审计器、package/依赖或其他生产文件；不得暂存、提交、tag、合并、推送或发布。

## 6. 验收

必须取得：

1. H1 审计器迁移前后均为 17/17、26/26、23 命令、exit 0；
2. C1 库存始终 23/26/原 hash、无 inventory error；
3. upload 与 answer-source 各不超过 400 行、无互相 import；
4. 6 个 provider 调用点只存在于对应命令模块，Tab 从 26 降至 20；
5. Tab 不再 import upload/答案资料 provider、`hasExtension`、`shortAnswerRubricPoints` 或 upload dialog 常量；
6. 根 hook provider=0，并明确组合 runtime、view model、upload、answer-source；
7. 65/65、TypeScript、Vite、Python 编译、全部 27 组 UI 与 `git diff --check` 通过；
8. HEAD/index/staged 与 S1 受保护文件不漂移；
9. 展开 porcelain 139→142，只新增本合同和两个命令模块，其他均为既有路径修改。

本批不证明 C1 完成、真实 provider/材料、老师减负、独立 `.app`、集成或发布。下一批只能另立 C1-S3，迁材料类型、归组、质量、重拍和归组证据 effect。
