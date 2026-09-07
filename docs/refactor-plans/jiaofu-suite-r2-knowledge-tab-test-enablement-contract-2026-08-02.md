---
status: "authorized_local_test_enablement"
code_authorized: true
authorization: "user_2026-08-02_next_after_r1-1"
source_commit: "6072360049b0"
source_exam_sha256: "9d7acd3ba3c63acfa9e494b86a8f955f81930cc82209886ec49fa2be2e42220b"
scope: "R2-E1-knowledge-tab-characterization"
production_extraction_authorized: false
integration_authorized: false
---

# 教辅系统 R2-E1 KnowledgeTab 特征测试前置合同

> 本合同执行用户要求的“下一步”：先为 R2 首批完整 Tab 建立可运行特征测试。本批不移动 `KnowledgeTab`，不修改生产代码、状态编排或 API；通过后才另立生产外移合同。

## 一、候选选择

`src/pages/Exam.tsx` 当前顶层 feature/Tab 的实测规模：

| 候选 | 当前行段 | 约行数 | 首批判断 |
|---|---:|---:|---|
| `FixedIntakeTab` | 443–1974 | 1532 | 同时承载普通卷、答题卡、默写，不作为首批 |
| `SubjectiveReviewTab` | 2209–2811 | 603 | 状态和评分 revision 边界较多，后置 |
| `DictationReviewTab` | 2822–3134 | 313 | 有发布/批量终审副作用，后置 |
| `ObjectiveReviewTab` | 3166–3426 | 261 | 有批量确认和发布副作用，后置 |
| `GradeTab` | 3427–3593 | 167 | 依赖旧式补录/终审流程和共享 helper，后置 |
| `QuestionTab` | 3594–3698 | 105 | 依赖题型常量、题目输入构造和创建命令，后置 |
| `KnowledgeTab` | 3699–3755 | 57 | 最小完整 Tab，依赖封闭，选为 R2 首批 |

`KnowledgeTab` 的生产边界固定为：

```text
输入 props
  knowledge: KnowledgePoint[]
  onDone(message)
  onError(message)

本地状态
  name / code / parentId / busy

外部依赖
  React useMemo / useState
  KnowledgePoint type
  kpCreate command
```

它不持有 `Exam` 总加载、tab 路由、学生/题目/作答状态，也不直接发布成绩或写学习证据。

## 二、本批唯一允许改动

| 文件 | 状态 | 唯一动作 |
|---|---|---|
| `scripts/test_exam_knowledge_tab_ui.py` | 新建 | 使用 Playwright + 浏览器端 Tauri invoke mock，固定当前内联 `KnowledgeTab` 行为 |

明确禁止修改：

- `src/pages/Exam.tsx`、`src/pages/exam/examPure.ts`；
- `src/api/exam.ts`、其他前端组件、CSS；
- package/lockfile/tsconfig/Vite 配置和依赖；
- Rust、Tauri command、SQL、迁移、IPC/DTO；
- 现有 UI 测试和任何业务文案。

## 三、固定特征场景

新脚本必须在当前内联组件上覆盖：

1. 进入“改作业 → 题目批改 → 知识点”，显示一个根节点、一个子节点和总数；
2. “上级板块”只列根节点，不把子节点作为可选父节点；
3. 名称为空时显示“知识点名称不能为空”，不得调用 `kp_create`；
4. 填写名称、编码并选择根节点后，`kp_create` 参数必须精确为 `{subjectId: null, parentId, code, name}`；
5. 创建成功显示“知识点已创建”，清空名称/编码，并在 `onDone → load()` 后回读新增子节点；
6. 模拟 `kp_create` 失败时显示错误，不显示成功提示，也不向本地列表伪造节点。

测试只固定现有行为，不在本批改善交互或改变知识点模型。

## 四、施工保护与验收

施工前必须校验：

- HEAD 仍为 `6072360049b09c3155726773039da139834294fe`；
- `Exam.tsx` SHA-256 为 frontmatter 固定值；
- 新测试文件不存在；
- `node --test tests/frontend/examPure.test.ts` 6/6 通过；
- `npm run build` 通过；
- 保存完整 porcelain、Git index、全部既有脏普通文件 SHA-256。

改后按顺序运行：

1. `npm run build`；
2. 启动 `npm run preview -- --host 127.0.0.1 --port 4173`；
3. `python3 scripts/test_exam_knowledge_tab_ui.py`；
4. 新文件 no-index whitespace 检查；
5. `Exam.tsx` 与所有施工前脏文件哈希不变；
6. 排除唯一新测试文件后，porcelain 与 Git index 前后逐字节一致。

任一项失败即停止。通过后只允许标记“R2 KnowledgeTab 特征测试前置通过”；不得在同批移动组件。

## 五、下一批边界

只有本测试通过后，才可另立 `R2 KnowledgeTab 生产外移合同`，精确冻结：

- 新组件文件路径和 import；
- `Exam.tsx` 删除的固定行段；
- `KnowledgePoint` 类型与 `kpCreate` 依赖来源；
- UI 特征测试、build、机械 diff 和工作树保护；
- 不修改 props、文案、命令参数、加载/刷新语义或老师终审边界。

本合同不授权该生产外移。
