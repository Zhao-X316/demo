# Jiaofu Suite R3 前端状态与请求编排设计合同（2026-08-02）

## 1. 合同地位

本文是 R2 收口后的 R3 设计入口。R3 目标不是继续按行数搬文件，而是在不改变老师终审权、业务命令参数、失败保留和持久化事实所有权的前提下，把高风险页面中的瞬时状态、异步请求、effect 和展示职责分开。

本合同通过只允许开始 R3-E1 特征测试前置；不自动授权生产状态重写、Rust/SQL/DTO 修改、集成或发布。

## 2. 冻结基线

- 分支：`codex/t2-artifacts`
- HEAD：`6072360049b09c3155726773039da139834294fe`
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`
- 完整 porcelain：94 条
- staged：0
- `FixedIntakeTab.tsx`：1,121 行，SHA-256 `6f8192950782c9476e14a64ca0a3a6ed8e53d918140a889492b02378676a3d93`
- 固定上传共享壳特征脚本：841 行，SHA-256 `e03db3110b6bb332ba98cb97526e6b4dc01e25b5f2698ba17a1e6a622ca2bc7e`

现有脏工作树全部视为用户既有改动；R3 不得暂存、提交、覆盖、清理或借机吸收其他文件。

## 3. 当前复杂度事实

通过 TypeScript AST 对七个 Tab 做只读盘点：

| Tab | 行数 | `useState` | `useEffect` | 异步块 | 业务调用 |
|---|---:|---:|---:|---:|---:|
| `FixedIntakeTab` | 1,121 | 41 | 5 | 19 | 26 次调用 / 23 个唯一命令 |
| `SubjectiveReviewTab` | 570 | 9 | 2 | 9 | 11 |
| `DictationReviewTab` | 337 | 7 | 2 | 6 | 6 |
| `ObjectiveReviewTab` | 328 | 5 | 2 | 5 | 5 |
| `GradeTab` | 173 | 5 | 0 | 2 | 2 |
| `QuestionTab` | 109 | 8 | 0 | 1 | 1 |
| `KnowledgeTab` | 61 | 4 | 0 | 1 | 1 |

结论：R3 的首要对象是完整固定上传编排，其次是三个终审工作台。`GradeTab`、`QuestionTab`、`KnowledgeTab` 当前函数短、无 effect，不为“统一形式”强行引入 controller。

## 4. 不可变业务边界

R3 全程必须保持：

1. 机器结果只作为建议，不自动人工终审、计分或发布；
2. 普通卷、答题卡、默写三条材料链保持独立识别语义；
3. 题库沉淀是可恢复旁路，失败不能阻断当前学生作业识别和批改；
4. prepare 失败后重试必须复用原 `idempotencyKey`；成功或输入失效后才清除；
5. 普通卷逐页结构确认、题库同步、逐区识别继续按当前顺序和部分成功语义执行，不改成 fail-fast；
6. 答题卡/默写逐页结果与失败按 `pageId` 保留，单页失败不抹掉已成功页面；
7. 首次分析使用稳定幂等键，老师主动重试或重新选择模板才生成新 UUID；
8. 原图、OCR、老师修正、评分 revision 和发布 revision 继续由后端领域模块持有；前端 reducer 不成为第二事实源；
9. 页面刷新或组件卸载不能触发任何自动老师写入；
10. `Exam.tsx` 保持顶层只读加载和七个 Tab 编排，不重新吸收 feature 状态。

## 5. R3 终态结构

目标结构：

```text
Exam.tsx
  └── FixedIntakeTab.tsx
        ├── useFixedIntakeController.ts
        │     ├── fixedIntakeState.ts
        │     ├── 固定上传 read effects
        │     └── 语义化 commands
        └── 现有八个展示面板
```

职责：

| 模块 | 唯一职责 | 禁止 |
|---|---|---|
| `fixedIntakeState.ts` | 纯状态类型、初始值、事件 reducer、纯 selector | Tauri、React effect、API、随机 UUID、时间、提示副作用 |
| `useFixedIntakeController.ts` | 组合 reducer、read effects、请求占用、过期响应保护、语义命令 | JSX、老师自动终审、跨领域数据库事实 |
| `FixedIntakeTab.tsx` | 空态、页面组合、把 controller 的 view/actions 传给面板 | 直接 `exam*` 调用、原始 setter、复杂请求链 |
| 展示面板 | 渲染证据和发出语义事件 | 直接 API、直接修改其他领域状态 |

不得把 1,121 行搬成一个新的 900 行“巨型 hook”后宣称完成。controller 中单个命令应保持单一业务动作；跨材料共享只能下沉请求占用和过期响应机制，不能合并三条材料事实。

## 6. 固定上传状态域

最终 reducer 采用一个嵌套 `FixedIntakeState`，保证跨域失效可原子完成；迁移期间允许单域逐批接线，但最终不得残留同一字段的双写。

| 状态域 | 当前字段 | 所有权说明 |
|---|---|---|
| `context` | `classId`、`assessmentVersionId` | 当前选择；由 options 校准和老师选择事件修改 |
| `draft` | `studentPaths`、`answerPath`、`answerText`、`expectedPages`、`pageCycle` | 尚未提交的本机输入，不是数据库事实 |
| `batch` | `requestKey`、`busy`、`result`、`confirmingType` | prepare 与材料类型确认的 UI 投影 |
| `answerSource` | `analysis`、`busy`、`error`、`rubricPointMappings` | 答案资料机器结构与老师映射草稿 |
| `grouping` | `startNo`、`absentStudentNos`、`confirming`、`evidence`、`loadingEvidence` | 学生顺序和已确认页面证据的 UI 投影 |
| `quality` | `rejectedPageIds`、`confirming`、`retakingPageId` | 当前质量复核草稿和动作占用 |
| `ordinary` | `runs`、`analyzingPageIds`、`confirmations`、`confirmingPageIds` | 普通卷逐页机器结果和老师结构确认投影 |
| `answerSheet` | `templateStatus`、`templateStatusLoaded`、`templateRun`、`templateBusy`、`pageResults`、`pageFailures`、`processingPageIds` | 答题卡模板集与逐页处理投影 |
| `dictation` | `templateStatus`、`templateStatusLoaded`、`templateRun`、`templateBusy`、`pageResults`、`pageFailures`、`processingPageIds` | 默写模板与逐页处理投影 |
| `control` | `scopeRevision`、`activeOperations` 的 UI 投影 | 过期响应判定和同步请求占用；真实 Promise/Abort 信息留在 ref，不序列化 |

`activeOperations` 不能替代后端 run 状态，只用于按钮禁用、进度展示和同一前端实例内防重入。

## 7. 事件与迁移合同

### 7.1 上下文和输入

| 事件 | 必须结果 | 当前行为保护 |
|---|---|---|
| `OPTIONS_RECONCILED` | 仅当当前 class/assessment 已不存在时选择首个合法值 | 合法值不得因 options 数组引用变化而重置 |
| `CLASS_SELECTED` | 写入新 class，按当前 reset 语义失效已有 session | R3 行为保持批不擅自决定是否清空已选照片 |
| `ASSESSMENT_SELECTED` | 写入新版本，失效已有 session | 已选照片/答案是否保留先按当前行为测试冻结 |
| `STUDENT_PATHS_SELECTED` | 写入自然顺序路径，清理旧处理结果，等待页周期结果 | 不排序第二次、不猜学生 |
| `PAGE_CYCLE_RESOLVED` | 仅在同一文件 scope 下写 `pageCycle/expectedPages` | 旧文件的晚到结果不得覆盖新文件 |
| `EXPECTED_PAGES_CHANGED` | 写草稿并使旧 prepare 结果失效 | 不自动提交 |
| `ANSWER_FILE_SELECTED` | 文件与粘贴文本互斥，失效旧 session | 不解析为正式答案 |
| `ANSWER_TEXT_CHANGED` | 写草稿并失效旧 session | 不触发自动评分 |
| `ANSWER_CLEARED` | 清空答案输入并失效旧 session | 沿用已确认作业答案 |

### 7.2 prepare 与答案资料

| 事件 | 必须结果 |
|---|---|
| `PREPARE_STARTED` | 同步占用 `prepare:<requestKey>`，保持 requestKey |
| `PREPARE_SUCCEEDED` | 保存 result、初始化起始学号、清空缺交草稿、清除 requestKey；有答案资料时显式启动结构分析 |
| `PREPARE_FAILED` | 保留全部输入和 requestKey，解除 busy，显示原错误 |
| `ANSWER_SOURCE_STARTED/SUCCEEDED/FAILED/FINISHED` | 失败保留旧输入与可重试路径；成功初始化当前 rubric 映射 |
| `ANSWER_SOURCE_RESOLUTION_SUCCEEDED` | 更新 review，并只移除答案资料相关 reason code |
| `RUBRIC_MAPPING_CHANGED` | 只修改指定 `assessmentItemId:orderIndex`，不改后端 rubric |

### 7.3 归组和质量

| 事件 | 必须结果 |
|---|---|
| `MATERIAL_CONFIRM_*` | 失败保留候选与按钮；成功只合并后端确认结果 |
| `GROUPING_CONFIRM_*` | 成功后原子清理所有旧材料处理投影，再由 batchId 读取归组证据 |
| `GROUPING_EVIDENCE_*` | 只接受当前 batchId/scopeRevision 的结果；失败解除 loading 并保留当前页面 |
| `QUALITY_REJECTION_TOGGLED` | 质量终审完成后拒绝再改草稿 |
| `QUALITY_CONFIRM_*` | 失败保留拒绝页；成功刷新证据，普通卷按当前规则继续分析 |
| `RETAKE_*` | 单页占用；失败保留原拒绝页，成功刷新证据并只激活对应学生 |

### 7.4 三材料处理

三类均使用 `PAGE_OPERATION_STARTED/SUCCEEDED/FAILED/FINISHED`，但 state slice、API 和结果类型保持分开。

- 普通卷：`ordinary.runs/confirmations`；结构确认成功后题库旁路失败只进入 failure 摘要，之后仍继续题区识别；
- 答题卡：`answerSheet.template*` 与 `pageResults/pageFailures`；模板集未 ready 不处理学生页；
- 默写：`dictation.template*` 与 `pageResults/pageFailures`；没有 active 模板不处理学生页；
- 每个 completion 必须带 `scopeRevision + batchId + pageId`，不匹配则 reducer 忽略。

## 8. 当前 reset 语义与纠偏边界

当前 `resetRequest()` 清除批次、答案分析、归组证据、质量选择、普通卷 runs、答题卡/默写模板及页面结果，但没有清除 `ordinaryConfirmations`、`confirmingOrdinaryPageIds`、两类 `templateBusy` 和若干进行中标记；选择新学生文件的另一套手写 reset 又清除了其中部分字段。

这是已发现的 reset 不一致风险，但不能在 reducer 机械接线批中悄悄修正。执行顺序必须是：

1. R3-E1 用浏览器特征测试冻结各输入动作当前保留/清除范围；
2. reducer 接线批按测试保持行为；
3. 另立“生命周期纠偏”行为变更合同，明确 class/assessment/学生文件/答案变化各自的保留策略；
4. 纠偏批以安全性和傻瓜式体验为依据原子化 reset，并单独验收。

历史遗漏不能被写成“已经没有问题”。

## 9. 请求所有权与防重入

### 9.1 操作键

| 操作 | 同步占用键 | 幂等/重试要求 |
|---|---|---|
| 页周期推断 | `cycle:<draftRevision>` | 晚到结果按 revision 丢弃 |
| prepare | `prepare:<requestKey>` | 失败重试复用 requestKey；成功清除 |
| 答案分析 | `answer-source:<batchId>:<mode>` | 首次稳定 key；主动 retry 新 UUID |
| 答案三类确认 | `answer-resolution:<batchId>:<runId>` | 同时只允许一个 resolution |
| 材料类型 | `material:<batchId>` | 同一前端实例防双击 |
| 学生归组 | `grouping:<batchId>` | 失败保留输入 |
| 归组证据读取 | `grouping-evidence:<batchId>` | read effect 去重、旧 scope 丢弃 |
| 质量确认 | `quality:<batchId>` | 失败保留拒绝页 |
| 单页重拍 | `retake:<batchId>:<pageId>` | 每页独立占用 |
| 普通卷分析 | `ordinary-analyze:<pageId>:<mode>` | 首次稳定 key；retry 新 UUID |
| 普通卷确认 | `ordinary-confirm:<pageId>:<runId>` | 页面独立占用；顺序和部分成功保持 |
| 模板状态读取 | `<material>-template-status:<scopeKey>` | StrictMode 重放不得启动两条相同后续处理链 |
| 模板分析/确认 | `<material>-template:<scopeKey>:<runId>` | 老师重新选择才新 UUID |
| 学生页处理 | `<material>-page:<pageId>` | 页面独立占用，成功/失败分别落本地投影 |

### 9.2 同步占用规则

React `busy` state 只能显示状态，不能作为防双击锁，因为同一事件循环内第二次调用可能读到旧闭包。controller 必须在发起请求前用 `useRef` 中的 operation registry 同步 claim；finally release。相同键已占用时直接返回，不创建第二个 provider 调用或第二个随机幂等键。

### 9.3 过期响应规则

每次会使 session 失效的语义动作递增 `scopeRevision`。异步命令捕获 revision；completion dispatch 同时携带 revision 和业务身份。只检查 mounted/cancelled 不足以防止旧 batch/旧照片结果写回新的仍挂载组件。

这些属于生命周期可靠性行为变更，必须在 reducer 行为保持接线完成后，以单独测试和合同实施。

## 10. effect、刷新和恢复

当前 5 个 effect：

1. class options 校准；
2. assessment options 校准；
3. grouping evidence 读取；
4. 答题卡模板状态读取，ready 后继续逐页处理；
5. 默写模板状态读取，active 后继续逐页处理。

R3 规则：

- effect 只发 read/continue 事件，不直接持有展示 JSX；
- 依赖使用稳定 scope key，不依赖每次 render 新建的数组/对象；
- React StrictMode 的 setup→cleanup→setup 不得产生两条相同处理链；
- 组件仍挂载时的父级 `load()`/options 刷新不得清空合法 session；
- 完整 WebView 刷新当前没有“按 batchId 恢复固定上传草稿”的只读 API，因此 R3 不使用 `localStorage` 复制学生路径或后端事实，也不伪称可继续草稿；刷新后必须 fail-closed 回上传起点，已落库批次/识别事实仍从对应终审工作台查看；
- 若未来需要恢复固定上传草稿，必须在 R4 另立只读恢复 use-case 和权限/数据生命周期合同，不能由前端猜数据库状态。

## 11. 展示组件接口收口

R3 结束时，展示面板不得继续接收以下原始 setter：

- `setClassId`、`setAssessmentVersionId`；
- `setExpectedPages`、`setRequestKey`、`setResult`；
- `setAnswerText`、`setAnswerPath`；
- `setRubricPointMappings`；
- `setGroupingStartNo`、`setAbsentStudentNos`。

替换为语义动作：

```text
selectClass
selectAssessment
changeExpectedPages
selectStudentFiles
selectAnswerFile
changeAnswerText
clearAnswerSource
changeRubricPointMapping
changeGroupingStart
toggleAbsentStudent
```

这保证 reset、scopeRevision 和安全门禁只有 controller 一个写入入口。

## 12. R3 分批施工路线

| 批次 | 唯一因素 | 生产代码 |
|---|---|---|
| R3-D | 本设计合同、只读证据与阶段路线 | 不改 |
| R3-E1 | 固定上传状态/reset/请求顺序/失败保留特征测试前置 | 不改 |
| R3-S1 | `answerSource` reducer slice 纯模型与直接测试 | 不接生产 |
| R3-S2 | `answerSource` slice 生产接线 | 只替换该域 4 个 state 与相关 dispatch |
| R3-S3 | `grouping + quality + batch` reducer slice 与生产接线 | 不移动请求函数 |
| R3-S4 | `ordinary` reducer slice 与生产接线 | 不改调用顺序 |
| R3-S5 | `answerSheet` reducer slice 与生产接线 | 不改模板/逐页处理语义 |
| R3-S6 | `dictation` reducer slice 与生产接线 | 不改模板/逐页处理语义 |
| R3-S7 | `context + draft + prepare` reducer slice；消除剩余同字段 `useState` | 保持现有 reset 行为 |
| R3-H1 | 生命周期纠偏：原子 reset、同步防重入、scopeRevision/stale completion、StrictMode | 明确行为变更，需专项测试 |
| R3-C1 | 请求/effect 迁入 `useFixedIntakeController`；Tab 只组合展示 | API 参数与顺序不变 |
| R3-C2 | 原始 setter 改为语义动作 | 面板文案/按钮/老师门禁不变 |
| R3-W1 | `Objective/Dictation/Subjective` 工作台 controller，先各自后共享 | 不强造万能 workbench hook |
| R3-A | 阶段收口审计 | 不改生产 |

每批必须独立合同、改前基线、机械或语义对照、专项测试和同批回执。不得把 S1～C2 合并成一次大重写。

## 13. R3-E1 首个执行单元

下一批唯一允许动作是增强 `scripts/test_exam_fixed_intake_shared_shell_ui.py`，不修改生产代码。至少新增：

1. class/assessment 变化后的当前 draft/session 保留矩阵；
2. expected pages、答案文本、答案文件清除后的 result/requestKey 行为；
3. 选择第二批学生文件后，普通卷确认、模板结果、逐页失败和处理占用不串批；
4. parent options 以等值新数组刷新时不重置合法选择；
5. prepare 首次失败重试继续复用同一 key（保留既有断言）；
6. 各关键失败继续保留老师已填输入和明确重试入口；
7. 当前完整 reload 行为明确为 fail-closed，不宣称恢复草稿；
8. 权威写入黑名单继续为 0。

同一时刻双击、StrictMode 重放和旧请求晚到属于 H1 的目标行为；E1 可以先建立可控 mock/计数能力，但不得提交必然失败的断言冒充完成。

## 14. 每批测试门禁

R3-E1 及后续至少执行：

1. `npx tsc --noEmit`；
2. `node --test tests/frontend/examPure.test.ts`；
3. 新增 reducer 后执行对应 `node --test` 直接测试；
4. `npm run build`；
5. 十三个 UI 脚本 `py_compile`；
6. 十三组既有浏览器回归；
7. 新增 R3 专项浏览器场景；
8. `git diff --check`；
9. 完整 porcelain、HEAD、Git index SHA-256、staged 和非目标文件保护。

涉及生产接线时，还须证明：

- 当前 23 个唯一固定上传命令的名称、参数、幂等键和相对顺序未漂移；
- 权威写入黑名单仍为 0；
- 每个失败场景保留的 UI 状态与改前一致；
- 浏览器可见文本和主动作不变；
- 没有新增依赖或第二状态源。

## 15. R3 完成标准

只有同时满足以下条件，R3 才可收口：

1. `FixedIntakeTab` 不再直接持有 41 个平铺 state、5 个 request effect 和 23 个唯一业务命令；
2. reducer 是瞬时 UI 投影，后端仍是批次/run/终审/发布事实唯一权威；
3. raw setter 已从展示面板接口移除；
4. prepare 重试 key、答案版本、归组、质量、三材料逐页部分成功和题库旁路语义保持；
5. 同键请求同步防重入，过期 completion 不写入新 scope，StrictMode 不复制处理链；
6. 父级等值刷新保持合法 session，完整 reload 明确 fail-closed；
7. Objective/Dictation/Subjective 三个工作台的异步编排职责完成独立收口或有证据证明无需迁移；
8. 全量测试和工作树保护通过；
9. 有独立 R3 阶段收口回执。

行数下降、编译通过或把代码搬入 hook 均不能单独证明完成。

## 16. 明确不证明

R3 不证明真实 OCR/OMR/手写识别准确率、学生与页面归属、provider 稳定性、调用成本、真实老师减负、独立 `.app`、数据合规、集成或发布。R3 不修改 Rust 事务和数据库恢复 API；这些属于 R4 或独立功能合同。
