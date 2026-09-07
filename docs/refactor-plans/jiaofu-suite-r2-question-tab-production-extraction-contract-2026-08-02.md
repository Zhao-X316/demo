---
status: "authorized_local_implementation"
code_authorized: true
authorization: "goal_2026-08-02_continuous_refactor"
source_commit: "6072360049b09c3155726773039da139834294fe"
source_exam_sha256: "c49483c739683986eb42fbd469573b70142b6c5524b2bb89e78e23b340d81d79"
source_question_types_sha256: "e07e893bce00b5b11dbd8f545762f547c077a02ceea47174be88f89629432095"
source_question_test_sha256: "725a568c9f1f4f8d7773a7e6fa73dd0366819562138b187e875236ba6227de7e"
scope: "R2-QuestionTab-production-extraction"
state_rewrite_authorized: false
integration_authorized: false
---

# 教辅系统 R2 `QuestionTab` 生产外移合同

> 本合同在 R2-E2 浏览器特征测试和 R2-S1 共享题型标签外移通过后，执行 `QuestionTab` 的行为保持式机械外移。本批只改变组件文件所有权，不修改表单、题型、选项解析、命令参数或父级刷新编排。

## 一、冻结边界

改前 `QuestionTab` 位于 `src/pages/Exam.tsx` 第 3587–3690 行，输入和外部依赖为：

```text
props
  questions: Question[]
  knowledge: KnowledgePoint[]
  onDone(message)
  onError(message)

dependencies
  React useState
  Question / QuestionInput / KnowledgePoint
  questionCreate
  TYPE_LABEL

parent orchestration
  <QuestionTab ... onDone={done} onError={...} />
  done(message) -> load() -> questionsList()/kpList() 回读
```

局部状态、必填门禁、`correct_answer` 保存方式、选项 label/`is_correct`/知识点/ord 构造、成功重置和失败保留都已由 `scripts/test_exam_question_tab_ui.py` 固定。

## 二、本批唯一允许改动

| 文件 | 动作 |
|---|---|
| `src/pages/exam/QuestionTab.tsx` | 新建；改前组件原样移动，只增加 imports 与 `export` |
| `src/pages/Exam.tsx` | 增加唯一 `QuestionTab` import；删除父级不再使用的 `QuestionInput`、`questionCreate` import；删除改前第 3586–3690 行（组件及前置分隔空行） |

明确禁止修改：

- `QuestionTab` 的 props、JSX、文案、状态初值、校验、参数和重置逻辑；
- 父级 `done → load()`、Tab 路由或错误提示；
- `questionTypes.ts`、`KnowledgeTab.tsx`、测试脚本、CSS 和依赖；
- API、Rust、Tauri command、IPC/DTO、SQL、迁移和业务数据。

## 三、机械不变量

1. 新组件移除固定 imports、将 `export function` 还原为 `function` 后，必须与改前第 3587–3690 行逐字节一致；
2. 改后父文件必须等于改前文件只执行以下固定变换：
   - 在 `KnowledgeTab` import 后增加 `QuestionTab` import；
   - 删除 API import 中的 `QuestionInput` 与 `questionCreate` 两行；
   - 删除改前第 3586–3690 行；
3. `QuestionTab` 定义只能有 1 处，父级调用保持 1 处；`questionCreate` 只由新组件导入；
4. `TYPE_LABEL` 继续来自 `questionTypes.ts`，`GradeTab` 保持父级读取，新组件保持自己的读取；
5. 行数下降只记录结构事实，不作为老师减负或发布证据。

## 四、施工保护与验收

施工前校验 frontmatter 的 HEAD 和三个文件哈希，确认目标组件不存在；保存改前 `Exam.tsx`、组件原文、完整 porcelain、改前 cached diff 和全部既有脏普通文件哈希。

改后依次运行：

1. 新组件和父文件两项机械逐字节比较；
2. `node --test tests/frontend/examPure.test.ts`；
3. `npm run build`；
4. preview 下运行知识点和题库 Tab 两个 UI 特征测试；
5. 校验 `questionTypes.ts`、测试脚本及所有非目标既有脏文件哈希不变；
6. 排除 `Exam.tsx` 和唯一新组件后，porcelain 与改前逐字节一致；cached diff 前后均为空；
7. whitespace 无诊断，当前无 staged change。

任一项失败即停止，不进入下一 Tab。通过后只允许标记“`QuestionTab` 行为保持式生产外移自动化通过”；不得宣称独立 `.app`、真实老师减负、真实题库/provider 或集成发布完成。

## 五、下一批边界

本批通过后重新盘点剩余内联 Tab。持有评分/发布副作用的 `GradeTab`、主观/客观/默写终审和三材料上传不得自动继承放行；必须重新评估 UI 特征测试、事务/发布语义和可逆边界。
