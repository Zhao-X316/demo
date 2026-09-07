---
status: "authorized_local_implementation"
code_authorized: true
authorization: "goal_2026-08-02_continuous_refactor"
source_commit: "6072360049b09c3155726773039da139834294fe"
source_exam_sha256: "b21834ec2fed489749b77971d0f9096ec974ea04bb7da98234ea37438f828823"
source_question_test_sha256: "725a568c9f1f4f8d7773a7e6fa73dd0366819562138b187e875236ba6227de7e"
scope: "R2-S1-shared-question-type-label-extraction"
question_tab_extraction_authorized: false
integration_authorized: false
---

# 教辅系统 R2-S1 共享题型标签外移合同

> 本合同执行 `QuestionTab` 生产外移前的唯一共享依赖整理：将 `GradeTab` 与 `QuestionTab` 共用的展示映射 `TYPE_LABEL` 原样外移到独立模块。本批不移动任何组件，不修改题型值、文案、表单、状态或业务命令。

## 一、只读边界结论

当前 `TYPE_LABEL` 位于 `src/pages/Exam.tsx` 第 101–107 行，仅有两个读取消费者：

```text
GradeTab
  └── TYPE_LABEL[question.qtype]

QuestionTab
  └── TYPE_LABEL[question.qtype]
```

它与 `OBJECTIVE_TYPE_LABEL`、`MATERIAL_TYPE_LABEL` 的键集合和领域语义不同；本批不得顺手合并三份映射，也不得把未知题型回退、题型模型或 API 枚举纳入改动。

## 二、本批唯一允许改动

| 文件 | 动作 |
|---|---|
| `src/pages/exam/questionTypes.ts` | 新建；原样保存 `TYPE_LABEL`，只增加 `export` |
| `src/pages/Exam.tsx` | 增加唯一 `TYPE_LABEL` import，删除改前第 101–108 行（常量块及其尾随单个分隔空行） |

明确禁止修改：

- `QuestionTab`、`GradeTab` 的 props、JSX、状态、调用点和文案；
- `OBJECTIVE_TYPE_LABEL`、`MATERIAL_TYPE_LABEL` 或其他常量；
- `scripts/test_exam_question_tab_ui.py` 及其他测试实现；
- API、CSS、依赖、Rust、Tauri command、IPC/DTO、SQL、迁移和业务数据。

## 三、机械不变量

1. 新模块去掉 `export ` 后，必须与改前 `TYPE_LABEL` 常量块逐字节一致；
2. 改后 `Exam.tsx` 必须等于改前文件只增加一条固定 import、删除改前第 101–108 行的机械变换；
3. `TYPE_LABEL` 只能有一个定义，`GradeTab` 与 `QuestionTab` 两个读取点保持原样；
4. 五个键值必须保持 `single/单选`、`multi/多选`、`judge/判断`、`fill/填空`、`subjective/主观`；
5. `QuestionTab` 仍内联，文件行数或 bundle modules 变化不代表老师减负。

## 四、施工保护与验收

施工前校验 frontmatter 的 HEAD、`Exam.tsx` 和题库 Tab 测试哈希；确认目标模块不存在；保存完整 porcelain、改前 cached diff 和全部既有脏普通文件哈希。

改后依次运行：

1. 新模块与原常量块、父文件与固定机械变换逐字节比较；
2. `node --test tests/frontend/examPure.test.ts`；
3. `npm run build`；
4. preview 下运行 `python3 scripts/test_exam_knowledge_tab_ui.py` 与 `python3 scripts/test_exam_question_tab_ui.py`；
5. 校验测试脚本及所有非目标既有脏文件哈希不变；
6. 排除 `Exam.tsx` 和唯一新模块后，porcelain 与改前逐字节一致；cached diff 前后均为空；
7. whitespace 无诊断，当前无 staged change。

任一项失败即停止，不继续 `QuestionTab` 生产外移。通过后只允许标记“共享题型标签行为保持式外移自动化通过”；不得在同批移动组件、改状态或宣称真实老师减负。

### 施工中边界纠正

首次父文件机械比较在任何功能测试前停止：`apply_patch` 删除常量时同时归一化了其后的单个分隔空行。该空行没有运行时语义，但初始机械预期只删第 101–107 行，因此 `cmp` 未通过。合同随即将固定删除范围纠正为改前第 101–108 行；没有扩大到下一个常量或任何业务实现。纠正后必须重新生成预期并比较，未通过不得继续。

## 五、下一批边界

本批通过后，`QuestionTab` 才具备“UI 特征测试已固定 + 共享标签依赖已独立”的生产外移前置。下一批仍须单独冻结组件 imports、精确删除范围、机械比较和 UI 回归合同。
