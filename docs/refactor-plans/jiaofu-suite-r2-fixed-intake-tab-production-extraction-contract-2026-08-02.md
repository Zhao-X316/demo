---
title: jiaofu-suite R2 FixedIntakeTab production extraction contract
date: 2026-08-02
status: authorized_local_production_extraction
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_exam_sha256: e29e08d699c4371d833727adc15a5bcfdb87168940aa5049b6ea946b77560cb2
private_candidate_sha256: 07249c7b1fda453b48066d17966c821b5926425ffc65d9a3b2c96e179f3b2652
component_candidate_sha256: 97091ec49d9c7405f888201a169dc6403ba4178de215f7a54f6cb8fa091e31f6
scope: R2-complete-fixed-intake-tab
integration_authorized: false
release_authorized: false
---

# R2 完整 `FixedIntakeTab` 生产外移合同

## 目标

在 R2-E12 固定上传共享壳十三组浏览器特征保护，以及八个封闭展示面板已经独立外移的基础上，只把完整 `FixedIntakeTab` 作为一个封闭 feature 单元机械迁到 `src/pages/exam/FixedIntakeTab.tsx`。父级 `Exam` 继续负责七个 Tab 的顶层编排、基础数据/workbench 加载、错误与成功提示、统一刷新和终审 Tab 切换。本批不重写固定上传状态、effect、十一类业务调用、三材料处理或老师终审语义。

## 边界取证

1. 完整组件候选是改前 `src/pages/Exam.tsx` 第 277～1287 行，共 1,011 行，SHA-256 `97091ec4...31f6`；定义后直到文件末尾没有其他顶层对象。
2. 组件只接收四项外部契约：`options`、`onOpenReview`、`onDone`、`onError`。其余班级/作业选择、学生文件、答案资料、页周期、幂等请求、批次、答案核对、归组、页面质量、普通卷、答题卡和默写状态均由组件内部持有。
3. 改前第 79～134 行共 56 行私有候选，SHA-256 `07249c7b...2652`，包含材料类型标签、文件扩展名、答案来源原因集合、新评分点哨兵，以及评分点映射初始化/完备性校验；全文消费者均属于 `FixedIntakeTab`。
4. `open`、`hasExtension`、两个私有 pure helper、八个 `FixedIntake*Panel` 和固定上传专属 API/type 只服务完整组件，应随组件迁移；`FixedIntakeOption` 同时仍是父级加载 state 的类型，因此父子分别从 API 模块导入。
5. 父级继续持有 `studentsList / questionsList / kpList / examAnswersList / examObjectiveWorkbench / examAnswerSheetSubjectiveWorkbench / examDictationWorkbench / examFixedIntakeOptions`，以及 `done → load()` 和 `onOpenReview → load() + setTab()`。
6. R2-E12 已覆盖保守空态、文件类型/顺序/页周期、准备失败幂等重试、粘贴与文件答案、答案一致/冲突处置、材料类型、起始学号/缺交、联系表、页面质量/重拍和普通卷/答题卡客观/答题卡主观/默写四路终审。

## 施工快照

- `Exam.tsx`：1,287 行，SHA-256 `e29e08d6...0cb2`。
- 私有候选：56 行，SHA-256 `07249c7b...2652`。
- 完整组件候选：1,011 行，SHA-256 `97091ec4...31f6`。
- 批前展开 porcelain：91 个路径；状态快照 SHA-256 `b1e2a887...2949`。
- Git index SHA-256 `b54ac3d5...d4b`；staged diff 为空。
- 分支 `codex/t2-artifacts`，HEAD `6072360049b09c3155726773039da139834294fe`。

## 唯一允许修改

1. 新建 `src/pages/exam/FixedIntakeTab.tsx`，原样承接 56 行私有候选和 1,011 行完整组件候选，仅增加所需 import，并给组件定义增加 `export`。
2. `src/pages/Exam.tsx` 只删除两个冻结候选、删除因此失效的固定上传专属 import，并增加唯一 `FixedIntakeTab` import。
3. 父级调用的四项 props、`load`、`done`、错误提示、成功提示、七个 Tab 状态与其余六个独立 Tab 均保持不变。
4. 完成后只回写对应验收、任务地图、跨模块实施标准和修改记录。

## 必须冻结的当前行为

1. 学生照片/PDF 继续按现有文件选择、扩展名校验、自然顺序、时间交叉核对和重复版式页周期推断处理；不新增或修改自动对应算法。
2. 答案图片/PDF/DOCX/XLSX/TXT/粘贴文本继续按当前分析、一致确认、保留已绑定版本或采纳新版本路径运行，冲突评分点仍由老师显式映射。
3. 材料类型、起始学号、缺交学生、页面质量与重拍仍须按现有门禁确认；失败或受阻状态不得伪装成功。
4. 普通卷页面分析/结构确认/题库同步/客观区识别、答题卡模板/OMR/OCR、默写模板/手写 OCR 的参数、顺序、并发状态、失败保留和消息不变。
5. 机器识别和评分仍只是建议；进入标准卷、主观题或默写终审仍由老师处理，任何上传结果不得自动计分或发布。
6. 成功回调仍触发父级 `done → load()`；打开终审仍先请求父级刷新再切换 Tab；错误继续进入父级统一错误栏。
7. 本批不得改变 API DTO、Rust/SQL/迁移、CSS、测试、业务状态或学习证据。

## 机械变换证明

1. 新文件去除 import 区，并把 `export function FixedIntakeTab` 还原为 `function FixedIntakeTab` 后，56 行私有候选和 1,011 行组件候选必须分别与快照逐字节一致。
2. 改后父文件移除唯一新 import，恢复原固定上传专属 import，并在原锚点插回两个冻结候选后，必须重建改前 `Exam.tsx` 同一 SHA-256。
3. 改后父文件不再出现 `FixedIntakeTab` 定义、固定上传专属 state/effect/handler/API 或八个子面板 import；父级调用仍恰好一处。
4. 新组件只能持有改前已有固定上传调用，不得出现父级列表/workbench 加载、其他六个 Tab 的写调用或顶层 Tab state。

## 禁止范围

- 不重命名、重排、合并、简化或重写候选内部代码。
- 不抽取 hook/service，不改变 state/effect 依赖、API 时序、并发、错误处理、提示文案或终审门禁。
- 不修改现有测试、夹具、样式、Rust、SQL、迁移或依赖。
- 不暂存、不提交、不 tag、不合并、不推送、不发布。

## 必跑验收

1. 两个候选逐字同源证明与父文件固定逆变换证明。
2. `npx tsc --noEmit`、`node --test tests/frontend/examPure.test.ts`、`npm run build`。
3. 既有十一组 R2 UI、普通卷题库旁路 UI 和固定上传共享壳 UI，共十三组浏览器回归。
4. `python3 -m py_compile` 覆盖十三个 UI 脚本。
5. `git diff --check`、staged diff、展开 porcelain、Git index、HEAD 和批前非目标路径保护。

## 停止条件

- 需要改写 state、effect、handler、API/DTO、现有测试或 CSS 才能完成；
- 机械同源或父文件逆重建失败；
- 父级统一加载/刷新或四项 props 语义变化；
- 任一测试失败，或非目标文件、暂存区、HEAD 发生未解释变化。

## 完成口径

只有所有验收通过，才允许记为“R2 完整 `FixedIntakeTab` 行为保持式本地生产外移自动化通过，未集成、未发布”。届时可据实把 `Exam.tsx` 记为纯顶层页面编排，并另行审计 R2 是否满足结构收口；不得写成 OCR/评分准确、真实材料可靠、老师已减负、R3 自动放行或系统已发布。
