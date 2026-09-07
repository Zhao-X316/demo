---
title: jiaofu-suite R2 FixedIntake quality panel production extraction contract
date: 2026-08-02
status: authorized_local_production_extraction
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_exam_sha256: 0cb5103b66adeab1b9ee48e2e3b637756c1a5968cbe8f8cc1a14b7757cf768f8
candidate_sha256: 4ac894fed5ab663cf8049e51722164fce053eec1a3c2b83328d76ae9cd029a96
scope: R2-fixed-intake-quality-and-retake-display
integration_authorized: false
release_authorized: false
---

# R2 固定上传页面质量与重拍面板生产外移合同

## 目标

在 R2-E12 固定上传共享壳特征测试保护下，仅把学生顺序确认摘要、页面联系表、质量确认和重拍入口迁到独立展示组件。父页继续持有归组证据、拒绝页、loading/busy/重拍 state，以及证据加载、拒绝切换、原子质量确认和重拍替换 handler；本批不改变学生归属、页面质量或三材料识别语义。

## 候选边界取证

1. 候选是 `src/pages/Exam.tsx` 第 1278～1359 行，共 82 行，SHA-256 为 `4ac894fe...a96`。
2. 候选只展示已确认学生范围、六页联系表、需重拍标记、质量确认门禁和已拒绝页重拍入口；没有直接调用业务 API。
3. 候选使用父页 `result / groupingEvidence / rejectedPageIds / loadingEvidence / confirmingQuality / retakingPageId`，并只通过四个父页回调完成拒绝切换、质量确认和重拍替换。
4. `convertFileSrc` 只把已归档本机路径转为可显示 URL，是候选唯一前端展示 helper；父页没有其他消费者。
5. R2-E12 浏览器测试已覆盖六页联系表、拒绝页选择、质量确认失败保留、确认后重拍入口、JPG/JPEG 约束和替换后重新读取联系表。

## 施工快照

- `Exam.tsx`：1,613 行，SHA-256 `0cb5103b...68f8`。
- 候选：82 行，第 1278～1359 行，SHA-256 `4ac894fe...a96`。
- 批前展开 porcelain：79 个路径；Git index SHA-256 `0273c39f...6578`；staged diff 为空。
- 分支 `codex/t2-artifacts`，HEAD `6072360049b09c3155726773039da139834294fe`。

## 唯一允许修改

1. 新建 `src/pages/exam/FixedIntakeQualityPanel.tsx`，承接候选 JSX、受控值、显式回调和唯一展示 helper `convertFileSrc`。
2. `src/pages/Exam.tsx` 只新增组件 import、用显式 props 调用替换候选，并移除已无父页消费者的 `convertFileSrc` import。
3. 所有 state、effect、证据加载、拒绝切换、质量确认、重拍替换 handler、十一类共享业务调用和三材料处理继续留在父页。
4. 完成后只回写对应验收、任务地图和修改记录。

## 必须冻结的当前行为

1. 归组确认后继续显示第一/最后学号和学生数；质量异常只影响对应页组。
2. 质量未确认时默认全部清楚；老师可逐页切换“清楚/需重拍”，图片和学生/页码定位不变。
3. 联系表加载中保持保守提示；没有证据或正在确认时按钮继续禁用。
4. 质量确认仍由父页事务 handler 原子建立页面归属；失败保留拒绝页选择供重试。
5. 确认完成后继续显示进入识别/需重拍人数，并明确未生成分数或发布。
6. 只对后端标记为 reject 的页面显示重拍；重拍期间所有入口继续锁定，文案和页面定位不变。
7. 重拍替换仍由父页验证文件、归档、替换并重新读取联系表；组件不得修改证据或自行恢复状态。
8. 本批不得触发识别、老师终审、计分、发布、题库或学习证据写入。

## 机械变换证明

1. 新组件 JSX 主体必须与候选 82 行逐字相同。
2. 将父页组件调用逆替换回候选并恢复 import 后，必须重建改前 `Exam.tsx` 的同一 SHA-256。
3. import 区只允许增加 `FixedIntakeQualityPanel`、移除父页已无消费者的 `convertFileSrc`。
4. 排除本合同和新组件后，批前 79 个展开路径不得发生非目标增删。

## 禁止范围

- 不修改学生归组、联系表 DTO、质量状态、拒绝页集合、重拍校验或原子事务。
- 不移动 state、effect、handler、业务 API/DTO、材料类型、普通卷/答题卡/默写处理、终审路由或样式。
- 不修改现有测试、Rust、SQL、迁移或依赖。
- 不暂存、不提交、不 tag、不合并、不推送、不发布。

## 必跑验收

1. 组件候选主体逐字同源证明和父页逆重建证明。
2. `npx tsc --noEmit`、`node --test tests/frontend/examPure.test.ts`、`npm run build`。
3. 既有十一组 R2 UI、普通卷题库旁路 UI 和固定上传共享壳 UI，共十三组浏览器回归。
4. `git diff --check`、staged diff、展开 porcelain、Git index、HEAD 和批前非目标路径保护。

## 停止条件

- 需要修改 state、effect、handler、API/DTO、样式或现有测试才能完成；
- 新组件开始加载证据、维护拒绝状态、验证重拍文件或调用业务 API；
- 机械同源或逆重建证明失败；
- 任一既有测试失败，或非目标文件、暂存区、index、HEAD 变化。

## 完成口径

只有所有验收通过，才允许记为“固定上传页面质量与重拍展示完成本地生产外移，行为等价、未集成”。不得写成真实图像质量识别准确、学生归属可靠、老师减负、整个 `FixedIntakeTab` 已重构完成或发布已验证。
