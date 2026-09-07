---
title: jiaofu-suite R3-S7 context draft prepare state reducer and production connection contract
date: 2026-08-02
status: authorized_local_behavior_preserving_refactor
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_fixed_intake_sha256: 4cf5166a10bc44f6736ef27aa7d9e0a9c69eec1087e87bb90e758b0e32eaceec
source_upload_form_sha256: 3f709773828a2424f07263cee1bc97cc7a5d136f4947ecf833ff9182d6f5fc09
source_state_sha256: 67a012710fc73c6c14fefdac51e5e83636082de479fb41f999ae14352bd928c9
source_state_test_sha256: 939f48ca63a6f5d69a70a30c8185089283a7574ea0bab31ada6a7491e5d23c3c
source_shared_shell_sha256: dcff9b4dc01f5a158129a40992f89fa972703f4abbe1d9e54b105a2c0d00e4a7
scope: R3-S7-context-draft-prepare-state-production-connection
behavior_change_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-S7 context、draft 与 prepare 状态 reducer 接线合同

## 目标

把 `FixedIntakeTab` 中最后七个同字段 `useState`——班级、作业版本、学生文件、答案文件、答案文本、固定页数、页周期建议——并入现有 workflow reducer。prepare 的 `requestKey/busy/result/confirmingType` 已在 `batch` slice，本批只补齐其输入所有权并机械接线，不移动任何请求函数，不改变参数、调用顺序、提示、老师确认门禁或 reset 范围。

完成后 `FixedIntakeTab` 不再持有业务字段 `useState`；但这不代表 R3-H1 生命周期纠偏、同步防重入、scope revision、旧响应隔离、StrictMode 去重或 R3-C1 controller 已完成。

## 改前状态与事实边界

| 域 | 字段 | 初始值 | 所有权 |
|---|---|---|---|
| `context` | `classId` | 首个班级或 `0` | 当前 WebView 选择投影 |
| `context` | `assessmentVersionId` | 当前班级首个作业或 `0` | 当前 WebView 选择投影 |
| `draft` | `studentPaths` | `[]` | 本次选择的本机路径顺序 |
| `draft` | `answerPath` | `null` | 可选答案文件路径 |
| `draft` | `answerText` | `""` | 可选粘贴答案 |
| `draft` | `expectedPages` | `"1"` | 老师可编辑的固定页数字符串 |
| `draft` | `pageCycle` | `null` | 页周期推断建议 |
| `batch` | `requestKey/busy/result/confirmingType` | 已有 reducer 初值 | prepare 与材料确认的瞬时投影 |

以上均不是学生归属、正式答案、成绩或发布事实。`studentPaths` 必须保持系统文件选择器返回顺序；本批不二次排序、不从文件名猜学生、不把答案草稿升级为权威答案。

## 当前行为矩阵

1. 初次挂载时，班级取 `classOptions` 第一项；作业取该班级在原始 `options` 顺序中的第一项。
2. `options` 刷新时，仍合法的班级和作业选择保持不变；非法班级切到新列表第一项或 `0`，随后非法作业切到新班级第一项或 `0`。自动校准当前不调用 `resetRequest()`，本批保持。
3. 老师切换班级或作业时，上传表单先写选择，再调用现有 `resetRequest()`；已选学生文件、答案草稿、固定页数和页周期建议保留，已有 session 投影按原 reset 范围清理。
4. 选择学生文件取消或扩展名不支持时不改任何草稿；合法路径先原顺序写入，再执行现有 session/答案/普通卷/答题卡/默写 reset，然后发起页周期推断。
5. 页周期推断成功才同时写 `pageCycle` 与 `expectedPages`；推断抛错时，新 `studentPaths` 保留，旧 `pageCycle/expectedPages` 也保留。旧请求晚到覆盖新草稿的风险留给 R3-H1，不在本批纠偏。
6. 老师修改固定页数时只写字符串草稿并使旧 prepare `requestKey/result` 失效；不自动提交。
7. 合法答案文件选择后写 `answerPath`、清空 `answerText`，再调用现有 `resetRequest()`；答案文本修改写原始字符串并 reset；清除答案同时把文件设为 `null`、文本设为空并 reset。
8. prepare 参数继续读取当前 `assessmentVersionId/studentPaths/answerPath/answerText/expectedPages`；文本只在请求边界 `trim()`，空值传 `null`。
9. prepare 失败只显示错误并在 finally 解除 busy，输入与 `requestKey` 保留供重试；成功保存 result、清 requestKey，并按现状在答案文档存在时启动答案结构分析。
10. 当前同一事件循环内双击仍可能绕过 React busy；当前 effect 自动校准、页周期晚到响应与 StrictMode 风险都必须原样留给 R3-H1。

## 纯状态合同

`FixedIntakeBatchWorkflowState` 新增：

```text
context
  classId
  assessmentVersionId

draft
  studentPaths
  answerPath
  answerText
  expectedPages
  pageCycle
```

初始工厂接受可选初始 context；未传参数时仍返回 `{ classId: 0, assessmentVersionId: 0 }`。每个新初始状态必须独占 `studentPaths` 数组。

| action | 结果 |
|---|---|
| `CONTEXT_CLASS_CHANGED` | 只写 classId；相同值返回原 state |
| `CONTEXT_ASSESSMENT_CHANGED` | 只写 assessmentVersionId；相同值返回原 state |
| `DRAFT_STUDENT_PATHS_CHANGED` | 只替换路径数组；相同引用返回原 state |
| `DRAFT_ANSWER_PATH_CHANGED` | 只写答案文件；相同值返回原 state |
| `DRAFT_ANSWER_TEXT_CHANGED` | 只写答案文本；相同值返回原 state |
| `DRAFT_EXPECTED_PAGES_CHANGED` | 只写页数字符串；相同值返回原 state |
| `DRAFT_PAGE_CYCLE_RESOLVED` | 只写页周期建议；相同引用返回原 state |

reducer 不负责触发 reset、请求、错误提示、答案互斥或页周期推断；这些编排继续按现有顺序留在组件。所有既有 action 必须保留 `context` 与 `draft` 引用，除非该 action 明确修改对应 slice。

## 唯一允许修改

1. 本合同。
2. `src/pages/exam/fixedIntakeState.ts`：新增 context/draft slice、初始参数、action 与纯 reducer 分支。
3. `tests/frontend/fixedIntakeState.test.ts`：先写 context/draft/prepare 直接测试，再实现模型。
4. `src/pages/exam/FixedIntakeTab.tsx`：删除七个 `useState`，从 reducer 解构同名字段，并将 setter 机械替换为 dispatch 包装。
5. `src/pages/exam/FixedIntakeUploadForm.tsx`：把 setter prop 类型从 React `Dispatch<SetStateAction<T>>` 收窄为只接受当前调用所需具体值的函数；prop 名、调用顺序、文案和交互不变。
6. 完成后的验收总览、重构标准、文档索引、任务地图与修改记录。

不得修改共享壳脚本、三材料面板、API、Rust/SQL/DTO、样式、依赖、请求函数位置或其他生产组件。语义 action 重命名属于 R3-C2，本批不提前实施。

## 生产接线映射

- `useReducer` 的 lazy init 参数由首个班级与该班级首个作业构成，保持原 `useState` 首次值。
- 两个 options 校准 effect 的依赖、校准条件和无 reset 行为原样保留，只将 setter 换为 context dispatch。
- 上传表单仍按“写字段 → reset/invalidate”顺序调用；仅将 setter 类型改成具体值 callback。
- 学生文件选择仍先写 paths，再执行五类 reset，再 await 页周期；成功后先写 pageCycle，再写 expectedPages。
- 答案文件仍先写 path、再清 text、再 reset。
- `resetRequest()`、`invalidatePreparedBatch()`、`submit()`、全部请求函数及其参数不移动。
- `FIXED_INTAKE_SESSION_RESET` 和已有 material slice reset 继续保持 R3-E1 已冻结的不完整 reset 语义；本批不清 draft。

## 改前基线与保护

- 分支/HEAD：`codex/t2-artifacts@6072360049b09c3155726773039da139834294fe`。
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`；staged=0。
- 展开 porcelain：105 条。
- `FixedIntakeTab.tsx`：1,052 行，SHA-256 `4cf5166a...aceec`。
- `FixedIntakeUploadForm.tsx`：171 行，SHA-256 `3f709773...fc09`。
- `fixedIntakeState.ts`：1,014 行，SHA-256 `67a01271...8c9`。
- `fixedIntakeState.test.ts`：1,500 行，SHA-256 `939f48ca...d23c3c`。
- 答题卡/默写面板：`8b294bbb...bd5` / `5b8b26bc...e61`。
- 共享壳/普通卷题库同步脚本：`dcff9b4d...e4a7` / `4006d90b...2aa`。
- `src/api/exam.ts` / `package.json` / `tsconfig.json`：`1a796762...164` / `a3f0afa7...69b0` / `e09b1d73...f906`。
- 23 个业务命令清单 SHA-256：`375a66bfade8b24a2d2ffb180bb934494754e4d5cff25401bd04b8e7e4358ba4`。
- baseline：状态直接测试 33/33、`examPure` 6/6、共享壳全场景 PASS。

## 必跑验收

1. 测试先行：新增测试在 context/draft slice 与 action 尚不存在时稳定失败，再实现转绿。
2. `node --test tests/frontend/fixedIntakeState.test.ts` 与 `node --test tests/frontend/examPure.test.ts`。
3. `npx tsc --noEmit`、`npm run build`。
4. 十三个 UI 脚本全部 `py_compile` 并逐组运行；共享壳的 options 刷新、context 切换、prepare 重试/输入失效、答案文件清除与三材料路由必须 PASS。
5. 静态审计：`FixedIntakeTab.tsx` 的业务字段 `useState` 与 `setState` 类型 prop 均为 0；workflow reducer 生产消费者仍恰为 1。
6. 23 个业务命令名称清单与基线逐字一致；权威写入黑名单为 0。
7. 共享壳、三材料面板、普通卷题库同步脚本、API/Rust/DTO、依赖配置哈希不变。
8. `git diff --check`、目标空白扫描、完整 porcelain、HEAD、Git index 和 staged 保护。

## 停止条件

- 需要改变 class/assessment 自动校准或老师切换后的 draft/session 保留矩阵；
- 需要改变学生文件顺序、页周期失败/晚到处理、答案互斥、prepare 参数、幂等重试或请求顺序；
- 顺带实施同步防双击、scopeRevision、StrictMode 去重、controller 抽取或语义 action 重命名；
- 共享壳、面板、API、Rust/DTO、依赖或任一非目标生产文件漂移；
- 新增依赖、第二状态源或任一直接/浏览器/保护测试失败；
- 暂存区、HEAD 或 Git index 变化。

## 完成口径

全部验收通过后，只能记为“R3-S7 context/draft 七字段 UI 投影已受测并机械接入现有 workflow reducer，prepare 输入、reset、重试和请求语义保持不变”。不得写成固定上传生命周期安全、controller、真实 OCR/AI、老师减负、独立 `.app`、集成或发布已经完成。
