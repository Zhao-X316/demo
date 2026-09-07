# R3-C2-S1 固定卷语义动作生产迁移合同（2026-08-02）

## 目标

在不改变固定卷入口行为、状态机事件、异步完成身份和老师终审边界的前提下，将上传表单与根控制器公开的原始状态 setter 收口为面向教师操作的语义动作。

## 源基线

- Git HEAD：`6072360049b09c3155726773039da139834294fe`
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`
- 扩展 porcelain 条目数：`154`
- 语义动作审计：`2/10`（目标红，生产迁移前预期）
- 语义动作审计脚本 SHA-256：`a7764fe7c9fd9220b30b972772d635a72e1b7d3644ea2d61b6df1354ab0a9993`

### 生产源文件 SHA-256

- `src/pages/exam/FixedIntakeTab.tsx`：`551aa65cc4157b86778e51a7edd5c0434ffcbc12a005e422e25bfa3df21b22be`
- `src/pages/exam/FixedIntakeUploadForm.tsx`：`8a6c070d490556c81f51ad36bc3e0ef7038cc1d675785a3b60d3a447d24384b9`
- `src/pages/exam/useFixedIntakeController.ts`：`86855098e5869f7574b5f2518625a0d638aa4d858b8d9f43fb09dac3286c82d2`
- `src/pages/exam/fixedIntakeUploadCommands.ts`：`92bca583a5b0cca48abda162bbe24524283dabe9be694e6faae0b27d5bd0e01c`

## 本批唯一授权范围

仅允许修改以下四个生产文件：

1. `src/pages/exam/FixedIntakeTab.tsx`
2. `src/pages/exam/FixedIntakeUploadForm.tsx`
3. `src/pages/exam/useFixedIntakeController.ts`
4. `src/pages/exam/fixedIntakeUploadCommands.ts`

允许新增本合同。除此之外，不授权修改测试、审计脚本、API、依赖、Rust、数据库、配置、验收文档或其他生产文件。

## 机械迁移映射

### 对外公开动作

- `setClassId` → `selectClass`
- `setAssessmentVersionId` → `selectAssessment`
- `setExpectedPages` → `changeExpectedPages`
- `setAnswerText` → `changeAnswerText`
- `clearAnswer` → `clearAnswerSource`

### 控制器内部文件选择动作

- `setStudentPaths` → `selectStudentFiles`
- `setAnswerPath` → `selectAnswerFile`

内部文件选择动作只注入上传命令工厂，不得从根控制器 `actions` 暴露给页面。

## 必须保持不变

- reducer 事件名、事件载荷和作用域修订语义；
- 班级与作业选项失效后的自动协调行为；
- 文件类型限制、Tauri 文件选择器、自然顺序与页面周期推断；
- 请求幂等键、完成身份校验、并发占用和迟到结果隔离；
- 答案图片/文件/粘贴文本的互斥与清除行为；
- 普通试卷、答题卡、默写分流及其后续老师确认入口；
- API 调用参数、错误文案、DOM 标识、按钮顺序和老师终审权；
- C1、H1 与全部既有 UI 验收合同。

## 验收

1. `node scripts/audit_exam_fixed_intake_semantic_actions.mjs` 必须由 `2/10` 转为 `10/10`。
2. C1、H1 审计和 65 项 Rust 测试通过；前端构建、Python 编译、`git diff --check` 通过。
3. 27 项 UI 验收全部通过。
4. 语义动作审计、C1/H1 审计、Exam API、依赖清单和直接测试文件哈希不变。
5. HEAD、index 和 staged diff 不变；不提交、不暂存、不发布。
6. 扩展 porcelain 只因本合同从 `154` 增为 `155`；四个既有未跟踪生产文件的修改不得引入额外路径。
