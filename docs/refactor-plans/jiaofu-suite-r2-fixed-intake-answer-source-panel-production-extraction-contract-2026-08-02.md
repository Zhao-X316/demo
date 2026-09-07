---
title: jiaofu-suite R2 FixedIntake answer source panel production extraction contract
date: 2026-08-02
status: authorized_local_production_extraction
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_exam_sha256: cb16ec4430b31585becea4fa4fe0afe149ecacf2876185866bf5ff00f5be3f98
candidate_sha256: 000145685e836c8a3ab7f2bcd79d5cfa631bd5523dd602e662b05a5e2d543178
scope: R2-fixed-intake-answer-source-display-panel
integration_authorized: false
release_authorized: false
---

# R2 固定上传“答案资料核对”展示面板生产外移合同

## 目标

在 R2-E12 固定上传共享壳特征测试保护下，仅把 `FixedIntakeTab` 中“答案资料核对”展示面板迁到独立组件。父页继续持有答案资料分析、冲突处理、评分点映射与版本采纳语义；本批不改变老师终审权，不改变当前批次绑定的答案版本。

## 候选边界取证

1. 候选是 `src/pages/Exam.tsx` 第 1302～1450 行，共 149 行，SHA-256 为 `00014568...3178`。
2. 候选只负责展示整理中、失败、一致/冲突/缺题、评分点对应和四个处理入口；没有直接调用 Tauri/API。
3. 父页继续持有 `answerSourceAnalysis`、`answerSourceBusy`、`answerSourceError`、`rubricPointMappings` 及其 setter。
4. 父页继续持有答案分析、确认一致、沿用当前答案、另存新版本四个 handler；新组件只能通过回调调用它们。
5. `NEW_RUBRIC_POINT`、`initialRubricPointMappings`、`rubricPointMappingsReady` 与 `shortAnswerRubricPoints` 的业务使用继续留在父页；本批不重排评分点继承规则。
6. `result.answerDocumentCount > 0` 的外层可见性门禁继续由父页决定。

## 施工快照

- `Exam.tsx`：1,837 行，SHA-256 `cb16ec44...3f98`。
- 候选：149 行，第 1302～1450 行，SHA-256 `00014568...3178`。
- 批前展开 porcelain：75 个路径；Git index SHA-256 `0273c39f...6578`；staged diff 为空。
- 分支 `codex/t2-artifacts`，HEAD `6072360049b09c3155726773039da139834294fe`。

## 唯一允许修改

1. 新建 `src/pages/exam/FixedIntakeAnswerSourcePanel.tsx`，承接候选 JSX 和纯展示所需 helper import。
2. `src/pages/Exam.tsx` 只新增组件 import、用组件调用替换候选 JSX，并移除已无父页消费者的 `answerJsonLabel` import。
3. 不移动 state、effect、handler、API/DTO、评分点初始化/校验或外层答案资料门禁。
4. 完成后只回写对应验收、任务地图和修改记录。

## 必须冻结的当前行为

1. 答案资料为空时不显示核对面板；整理中、整理失败和可重试状态文案不变。
2. 上传答案只与当前已确认答案比较，不得静默替换。
3. 一致答案仍须老师一次确认；冲突或缺题继续阻断自动采用。
4. 简答题继续显示上传评分点、当前评分点及已确认知识/能力链接。
5. 评分点结构一致时继续按顺序预填；新增点、旧点复用冲突和退役说明不变。
6. “另存新版本”仍只在来源 ready、有冲突、无缺题且对应关系完整时可用。
7. 老师仍可沿用当前作业答案继续；答案失败仍可用当前批次 ID 重试。
8. 新版本只供未来使用，本批学生照片仍按原答案批改；不会触发计分、发布或学习证据写入。

## 机械变换证明

1. 新组件 JSX 主体必须与候选 149 行逐字相同。
2. 将父页组件调用逆替换回改前条件块后，除 import 区外应与改前 `Exam.tsx` 一致。
3. import 区只允许增加 `FixedIntakeAnswerSourcePanel`、移除父页已无消费者的 `answerJsonLabel`。
4. 排除本合同和新组件后，批前 75 个展开路径不得发生非目标变化。

## 禁止范围

- 不拆 `FixedIntakeTab` 其余上传、页数、材料类型、归组、质量、重拍或三材料处理流程。
- 不修改答案来源 handler、请求参数、幂等键、DTO、API、Rust、SQL、迁移、样式或现有测试。
- 不优化文案、DOM 结构、class、按钮门禁、异步顺序或错误处理。
- 不暂存、不提交、不 tag、不合并、不推送、不发布。

## 必跑验收

1. 组件候选主体逐字同源证明和父页逆重建证明。
2. `npx tsc --noEmit`、`node --test tests/frontend/examPure.test.ts`、`npm run build`。
3. 既有十一组 R2 UI、普通卷题库旁路 UI 和 R2-E12 固定上传共享壳 UI，共十三组浏览器回归。
4. `git diff --check`、staged diff、展开 porcelain、Git index、HEAD 和批前非目标路径保护。

## 停止条件

- 需要修改状态、handler、API/DTO、样式或现有测试才能完成；
- 组件开始直接调用业务 API，或父页失去答案版本/评分点映射控制权；
- 机械同源或逆重建证明失败；
- 任一既有测试失败，或非目标文件、暂存区、index、HEAD 变化。

## 完成口径

只有所有验收通过，才允许记为“固定上传答案资料核对展示面板完成本地生产外移，行为等价、未集成”。不得写成答案识别准确率、真实老师减负、答案新版本已推广、主 `FixedIntakeTab` 已完成外移或发布已验证。
