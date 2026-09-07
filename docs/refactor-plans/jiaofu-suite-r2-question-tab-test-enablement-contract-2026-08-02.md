---
status: "authorized_local_test_enablement"
code_authorized: true
authorization: "goal_2026-08-02_continuous_refactor"
source_commit: "6072360049b09c3155726773039da139834294fe"
source_exam_sha256: "b21834ec2fed489749b77971d0f9096ec974ea04bb7da98234ea37438f828823"
source_knowledge_tab_sha256: "26219646a562ae8e8d16fc9ead4f28465c8814ba8b1fd5741ae77c2089068c69"
scope: "R2-E2-QuestionTab-characterization"
production_extraction_authorized: false
integration_authorized: false
---

# 教辅系统 R2-E2 `QuestionTab` 特征测试前置合同

> 本合同执行持续重构目标中的下一测试前置：为当前内联 `QuestionTab` 建立可运行浏览器特征测试。本批不移动组件，不修改生产代码、共享题型标签、API 或状态编排。

## 一、只读边界结论

`QuestionTab` 当前位于 `src/pages/Exam.tsx` 第 3594–3697 行，约 104 行。边界为：

```text
输入 props
  questions: Question[]
  knowledge: KnowledgePoint[]
  onDone(message)
  onError(message)

局部状态
  questionNo / qtype / stem / correct / maxScore / kpId / optionsText / busy

外部依赖
  React useState
  Question / QuestionInput / KnowledgePoint
  questionCreate
  TYPE_LABEL（同时被 GradeTab 使用）
```

现有 `scripts/test_question_bank_ui.py` 等测试针对独立 K1“题目与题库”页面；`scripts/test_ordinary_question_sync_ui.py` 针对普通卷自动沉淀旁路，均不覆盖这个“改作业 → 题目批改 → 题库”的手工录题 Tab。

## 二、本批唯一允许改动

| 文件 | 动作 |
|---|---|
| `scripts/test_exam_question_tab_ui.py` | 新建；使用 Playwright 和浏览器端 Tauri mock 固定当前内联行为 |

明确禁止修改：

- `src/pages/Exam.tsx`、`src/pages/exam/KnowledgeTab.tsx` 和 `examPure.ts`；
- `src/api/exam.ts`、其他前端组件、CSS；
- package/lockfile、tsconfig、Vite 配置和依赖；
- Rust、Tauri command、IPC/DTO、SQL、迁移和任何业务文案。

## 三、固定特征场景

新测试至少覆盖：

1. 进入“改作业 → 题目批改 → 题库”，显示既有题目、题型标签、答案、分值和知识点；
2. 单选/多选显示选项输入，判断/填空隐藏选项输入；
3. 题干或标准答案任一为空时显示“题干和标准答案不能为空”，不得调用 `question_create`；
4. 多选答案规范化只用于选项 `is_correct` 判断，`correct_answer` 仍保存老师输入的 trim 值；
5. 选项解析覆盖显式英文标签、中文分隔符、无标签自动按顺序生成标签、空行过滤、ord 和知识点引用；
6. `question_create` 的 `{q}` 必须精确包含题号、题型、题干、答案、分值、知识点、默认 null 字段、enabled 和 options；
7. 成功后清空题号、题干、答案和选项，保留题型、分值和知识点，并经父级 `onDone → load()` 回读新增题目；
8. 模拟失败时显示错误，保留输入，不显示成功提示，也不伪造题目列表。

## 四、施工保护与验收

施工前必须校验 frontmatter 中的 HEAD 和两个生产文件哈希，确认目标测试文件不存在；运行 6/6 直接测试、前端 build、现有 `KnowledgeTab` UI 特征测试；保存完整 porcelain、Git index 和全部既有脏普通文件哈希。

改后依次运行：

1. Python 编译与新文件 whitespace 检查；
2. `node --test tests/frontend/examPure.test.ts`；
3. `npm run build`；
4. preview 下运行 `python3 scripts/test_exam_knowledge_tab_ui.py` 和 `python3 scripts/test_exam_question_tab_ui.py`；
5. 校验 `Exam.tsx`、`KnowledgeTab.tsx` 及所有既有脏文件哈希不变；
6. 排除唯一新测试文件后，porcelain 和 Git index 与施工前逐字节一致。

任一项失败即停止。通过后只允许标记“R2-E2 `QuestionTab` 特征测试前置通过”；不得在同批移动组件或重构 `TYPE_LABEL`。

## 五、下一批边界

测试通过后才能确定生产外移方式。由于 `TYPE_LABEL` 同时被 `GradeTab` 使用，下一合同必须明确选择并只执行一种结构方案，例如先外移共享标签，或让新组件接收稳定标签依赖；不得在组件外移时顺手重写题型模型或题库业务。
