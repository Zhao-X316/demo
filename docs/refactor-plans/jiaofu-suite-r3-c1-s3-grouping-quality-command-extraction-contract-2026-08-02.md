---
title: jiaofu-suite R3-C1-S3 grouping and quality command extraction contract
date: 2026-08-02
status: authorized_for_implementation
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_index_sha256: b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b
source_porcelain_count: 142
source_fixed_intake_sha256: 1532e2f53b0a08345c2311ac5c911b91e2f55721169c83fd2bd4b21066d67936
source_root_controller_sha256: 7dcb1e51838922e89b7edd48bd09d3241eaa08f2901665cc45031dca414f7223
source_upload_commands_sha256: 92bca583a5b0cca48abda162bbe24524283dabe9be694e6faae0b27d5bd0e01c
source_answer_source_commands_sha256: 4461da58f9601283ea1170c23f20de58fdec427e2e537ae9e94dbb70f7f55b56
source_lifecycle_audit_sha256: 7d8d48fc1d12adbbaae32c304409eba2fba6547fb696d0fc2e00359f503aded9
source_controller_audit_sha256: 24be2b5334af94f167afdf15f82526bb488480a338a6a4bae2d7ef7147cc1dd4
source_business_command_count: 23
source_provider_callsite_count: 26
source_business_command_sha256: 375a66bfade8b24a2d2ffb180bb934494754e4d5cff25401bd04b8e7e4358ba4
scope: R3-C1-S3-grouping-quality-command-and-effect-extraction
production_change_authorized: true
lifecycle_audit_owner_registration_authorized: true
controller_audit_dialog_alignment_authorized: true
provider_move_authorized: seven_calls_only
other_command_modules_authorized: false
c2_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-C1-S3 材料、归组、质量、重拍与证据 effect 迁移合同

## 1. 唯一目标

本批只新增 `fixedIntakeGroupingQualityCommands.ts`，机械迁移：

- 材料类型确认；
- 归组起点/缺交草稿动作与归组确认；
- 归组证据读取 effect 的 provider/claim/guard runner；
- 拒绝页草稿动作与质量确认；
- 单页重拍选择、替换、证据刷新和普通卷 continuation。

精确移动 7 个 provider 调用点：材料 1、归组 1、归组证据 3、质量 1、重拍 1。其他 13 个 provider、答题卡/默写两个读取 effect、普通卷、答题卡和默写命令均留在 Tab。

## 2. Effect 与过渡 continuation

- 命令 factory 返回 `groupingEvidenceBatchId` 与 `startGroupingEvidenceReadEffect`；根 hook 的 `useEffect` 只按 batch ID 调用 runner。
- provider、`startFixedIntakeReadEffect`、claim 和 `isCurrent` 必须在命令模块同一命名 owner 中，避免 H1 跨函数观察盲区。
- H1 owner 清单只新增 `startGroupingEvidenceReadEffect → grouping_evidence` 登记，原 provider 预期与保护判据不改。
- C1 审计器的 dialog 白名单漏了负责单页重拍的 grouping-quality 模块，与既定命令归属冲突。本批只把该文件补入合法 dialog owner；Tab 仍必须最终 dialog-free，其他白名单和判据不变。
- `confirmGroupingQuality` 与 `replaceRejectedPage` 需要调用尚在 Tab 的 `analyzeOrdinaryPages`。本批由 Tab 把该已提升函数显式传入根 hook，再由根 hook注入 factory；S4 普通卷命令迁移时必须删除该过渡参数。
- 命令模块不得 import 普通卷命令或 Tab。

## 3. 不可变业务合同

1. 23 个命令、26 个调用点和规范 hash 不变；
2. H1 17/17、26/26、key/claim/identity/stale guard 不变；
3. 归组成功后三材料 reset 顺序不变；
4. 质量确认仍先写 confirmation、再刷新证据、同步投影后仅普通卷续跑；
5. 重拍仍先同步 claim，再选择文件，current rejected page 才 started/provider；成功后刷新证据，仅激活普通卷学生时续跑；
6. finally/started/release、错误文案、dialog filter 和老师门禁不变；
7. API 参数/次数/顺序、reducer/runtime/view model、面板 props、Rust/SQL/DTO/样式不改；
8. C2 语义动作改名不混入。

## 4. 精确允许修改

仓库内仅：

1. 本合同；
2. 新增 `src/pages/exam/fixedIntakeGroupingQualityCommands.ts`；
3. 修改 `src/pages/exam/useFixedIntakeController.ts`；
4. 修改 `src/pages/exam/FixedIntakeTab.tsx`；
5. 修改 H1 审计器，只新增 grouping-evidence runner owner 登记；
6. 修改 C1 审计器，只把 grouping-quality 补入合法 dialog owner。

仓库外只允许写 S3 回执与权威索引/进度/修改记录。不得改测试、C1 审计器的其他判据、已完成两个命令模块、package/依赖或其他生产文件；不得暂存、提交、tag、合并、推送或发布。

## 5. 验收

必须取得：

1. H1 跨四文件仍为 17/17、26/26、exit 0；
2. C1 库存 23/26/原 hash且无 inventory error；
3. grouping-quality 不超过 400 行、provider=7、无命令模块 import；
4. Tab provider 20→13、读取 effect 3→2、归组/质量/重拍 handler 全部移除；
5. 根 hook provider=0，并组合 runtime/view/upload/answer-source/grouping-quality；
6. 65/65、TypeScript、Vite、Python 编译、27/27 UI 与 whitespace 通过；
7. HEAD/index/staged 与 S2 受保护文件不漂移；
8. 展开 porcelain 142→144，只新增本合同与 grouping-quality 文件。

本批不证明 C1 完成、真实 provider/材料、老师减负、独立 `.app`、集成或发布。下一批只能 C1-S4 迁普通卷命令并消除过渡 continuation 参数。
