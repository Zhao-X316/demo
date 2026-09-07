---
title: jiaofu-suite R3-H1-V2 lifecycle closeout audit contract
date: 2026-08-02
status: authorized_local_verification
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_index_sha256: b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b
source_porcelain_count: 130
source_fixed_intake_sha256: 2de6b5e82f588284a126fbf84aa81c2f398d6c2689558d41642c107d668f224a
source_lifecycle_sha256: f695551aebabdca8b13a4289e538ea6d76762072f90ed92c6179a76dadec45a2
source_root_state_sha256: b905b0d11747b7f2f7dd7e129e271fd7ae35a0d5b23bcd5d6951f45798735e1b
source_domain_state_sha256: 5fdb7b5761026f3dd8e0bf5b82b8637663a78719203478c62226de1add87a602
source_direct_test_sha256: 9942359cc0ac3df950b7297f44f368c312f32c80480c505ae150f5194e20a7da
source_audit_sha256: 8454eb2159567d719e5560e5a5744d23fa4a47e4b5174653bc52b2e9111c7a22
source_exam_api_sha256: 1a796762c34c9079430423965937faead89a16c2618930e64603bd262c49c164
source_exam_shell_sha256: ed4132caec69a2d018d0e6dcc01c126c2e42b7bb0df9b1869d76d6ca32dfe9bc
source_package_sha256: a3f0afa700aaeaa813f93dd6c56f6b9e8b3452e0be48e19fc2b8883af99d69b0
scope: R3-H1-V2-lifecycle-closeout-audit
production_change_authorized: false
test_change_authorized: false
audit_implementation_change_authorized: false
controller_migration_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-H1-V2 生命周期收口审计合同

## 1. 目标与独立性

本批从 H1-S10 改后产物重新开始一次只读终审，回答唯一问题：固定上传的 17 个登记操作族、26 个 provider 调用点是否都具备同步 operation owner、完整 completion identity 和 stale continuation 门禁，并且全部现有特征测试仍保持通过。

“独立”在本合同中指：

1. 先冻结 H1-S10 改后哈希，再重新执行审计与全量自动化，不沿用 H1-S10 的命令输出作为本批证据；
2. 本批不得修改生产代码、测试断言或审计器来制造绿色；
3. 审计结论只按本批重新取得的输出判定；
4. 本批仍由当前工作模型执行，因此不冒充“异模型审查”。正式异模型审查仍走项目修改记录队列。

## 2. 冻结对象

### 2.1 生产与测试哈希

合同 frontmatter 冻结以下对象：

- `FixedIntakeTab.tsx`、`fixedIntakeLifecycle.ts`、`fixedIntakeRootState.ts`；
- 领域纯 reducer `fixedIntakeState.ts`；
- 直接测试 `fixedIntakeState.test.ts`；
- 生命周期 AST 审计器；
- `src/api/exam.ts`、`Exam.tsx`、`package.json`。

本批结束时上述哈希必须全部不变。`FixedIntakeTab.tsx` 当前 1,644 行，直接测试当前 2,610 行；H1-V2 不以行数增减作为通过依据。

### 2.2 审计全集

继续使用 H1-V1 冻结的 17 个操作族和 26 个调用点：

1. 页周期推断；
2. 批次准备；
3. 答案资料分析；
4. 答案资料三类确认；
5. 材料类型确认；
6. 学生归组确认；
7. 归组证据读取；
8. 质量确认及证据刷新；
9. 单页重拍及证据刷新；
10. 普通卷逐页分析；
11. 普通卷结构确认、题库旁路与题区识别；
12. 答题卡模板状态读取；
13. 答题卡模板分析、确认与状态刷新；
14. 答题卡学生页处理；
15. 默写模板状态读取；
16. 默写模板分析与确认；
17. 默写学生页处理。

业务命令全集必须仍为 23 个，规范 SHA-256 必须仍为 `375a66bfade8b24a2d2ffb180bb934494754e4d5cff25401bd04b8e7e4358ba4`。任何新增、删除或改名都视为清单漂移，H1-V2 不通过。

## 3. 唯一允许修改

仓库内只允许新增本合同。不得修改：

- `src/` 下任何生产文件；
- `tests/` 或 `scripts/` 下任何测试、夹具与审计器；
- API、Rust、SQL、DTO、依赖、样式或文案；
- 现有 H1 合同和历史验收回执。

审计完成后只允许在仓库外新增 H1-V2 验收总览，并同步重构实施标准、文档索引、任务地图和修改记录。不得暂存、提交、tag、合并、推送或发布。

## 4. 从零验证顺序

### 4.1 静态与直接验证

必须依次执行：

1. `node scripts/audit_exam_fixed_intake_lifecycle.mjs`；
2. 重新从 AST 提取 23 个唯一业务命令并计算规范 hash；
3. `node --test tests/frontend/examPure.test.ts tests/frontend/fixedIntakeLifecycle.test.ts tests/frontend/fixedIntakeState.test.ts`；
4. `npx tsc --noEmit`；
5. `npm run build`；
6. `python3 -m py_compile scripts/test_*_ui.py`；
7. `git diff --check`。

### 4.2 独立浏览器回归

关闭 H1-S10 使用的预览服务后，必须重新启动新的 `npm run preview -- --host 127.0.0.1 --port 4173` 进程，并逐一执行当前全部 27 个 `scripts/test_*_ui.py`。不得只运行 H1-S10 专项，也不得复用旧服务进程。结束后必须停止预览服务并确认 4173 端口没有监听者。

## 5. 通过标准

以下条件必须同时满足：

- AST 审计退出 0，汇总精确为 `17/17` 操作族、`26/26` 调用点；
- 业务命令为 23 个，规范 hash 不漂移；
- 直接测试精确为 `63/63 PASS`；
- TypeScript 与 Vite build 通过；
- 全部 27 组浏览器脚本通过，H1-S10 专项仍为 `8/8 PASS`；
- Python 编译与 `git diff --check` 通过；
- 冻结文件哈希、HEAD、Git index 和 staged=0 均不漂移；
- 展开 porcelain 只能由 130 增为 131，唯一新增路径必须是本合同。

满足上述条件后，只能记录为：

> H1 生命周期门禁已完成本地自动化收口，17/17、26/26 经独立只读终审复现；允许另立 C1 合同，不自动授权 C1/C2 生产迁移。

## 6. 失败标准

任一检查失败时：

- H1 保持进行中；
- 记录精确失败命令、操作族、调用点或漂移哈希；
- 不得修改测试、审计器或业务代码后仍归入 H1-V2；
- 后续必须新立单一因素修复合同，再另做 H1-V3；
- C1/C2、R4～R5、集成和发布继续冻结。

## 7. 明确不证明

H1-V2 即使通过，也不证明：

- 真实 OCR、AI/provider 的准确性、稳定性或成本；
- 真实老师已经减负或实现零学习成本；
- WebView 刷新、应用重启或跨设备恢复；
- 独立 `.app`、签名、公证、正式集成或发布；
- `FixedIntakeTab.tsx` 大文件已经解决；
- C1/C2、三个终审工作台 controller、R4～R5 已经完成。

H1-V2 的唯一后续放行是：在不修改本批冻结对象的前提下，设计并审批 R3-C1 独立迁移合同。
