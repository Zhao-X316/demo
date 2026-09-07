---
status: "authorized_local_test_enablement"
code_authorized: true
authorization: "goal_2026-08-02_continuous_refactor"
source_commit: "6072360049b09c3155726773039da139834294fe"
source_exam_sha256: "ad1f1bb6fba6170954c73d42b061f9fa00c59456045191223a702a2335ccd8c9"
source_m25_ui_test_sha256: "51fceb6be88105834d43f1524cbc902da38c6fbfe09c42ea864ff1e06d1519d2"
source_question_tab_sha256: "9d7107dff0dc734fe1cb58480e5a0cfcabf6e847e81635f2ddb6cbc155a769f9"
scope: "R2-E3-SubjectiveLinkPanel-characterization"
production_extraction_authorized: false
integration_authorized: false
---

# 教辅系统 R2-E3 `SubjectiveLinkPanel` 特征测试前置合同

> 本合同为当前内联 `SubjectiveLinkPanel` 建立可运行浏览器特征测试。本批不移动组件，不修改知识/能力链接语义、版本另存命令或父级主观题终审。

## 一、只读边界结论

`SubjectiveLinkPanel` 当前位于 `src/pages/Exam.tsx` 第 1998–2200 行，约 203 行。边界为：

```text
输入 props
  assessmentItemId
  onDone(message)
  onError(message)

局部状态
  editor / sources / loading / saving

外部依赖
  React useEffect / useState
  SubjectiveLinkEditor / SubjectiveSourceLinkInput
  examSubjectiveLinkEditor
  examSubjectiveLinkSave
  KNOWLEDGE_RELATIONS / ABILITY_RESPONSE_MODES
```

现有 `scripts/test_m25_rubric_update_ui.py` 会让该面板以空 sources 渲染，但只验证“加入未来评分点示例”，没有操作链接面板，也不覆盖加载失败、链接增删、证据强度、确认取消、精确保存参数或保存失败。

## 二、本批唯一允许改动

| 文件 | 动作 |
|---|---|
| `scripts/test_exam_subjective_link_panel_ui.py` | 新建；基于既有 M2.5 浏览器 mock 扩展链接编辑器和保存命令，固定当前行为 |

明确禁止修改：

- `src/pages/Exam.tsx`、`src/pages/exam/*` 和 `src/api/exam.ts`；
- 既有 UI 测试、CSS、package/lockfile、tsconfig、Vite 配置和依赖；
- Rust、Tauri command、IPC/DTO、SQL、迁移、链接版本规则和业务文案。

## 三、固定特征场景

新测试至少覆盖：

1. 进入“改作业 → 题目批改 → 答题卡主观题”，按当前 assessment item 精确调用一次 `exam_subjective_link_editor`；
2. 成功加载答案槽位和评分点两类 source、已有知识/能力链接以及可选知识/能力项；
3. 新增知识点时，答案槽位默认 `answer_basis`，评分点默认 `rubric_basis`，且不重复选择当前 source 已有节点；
4. 新增能力时，答案槽位默认 `0.5 + recall`，评分点默认 `0.7 + structured_response`；支持修改 response mode、证据强度和移除链接；
5. 保存前必须出现确认框；取消时不得调用保存，当前编辑仍保留；
6. 确认后 `exam_subjective_link_save` 的 `assessmentItemId` 与完整 sources 必须精确匹配，成功文案带未来作业版本、知识/能力条数和“历史成绩保持不变”；随后重新读取 editor；
7. 加载失败时显示错误并隐藏面板；保存失败时显示错误、保留当前编辑、不显示成功文案；
8. 测试不得通过修改生产组件、放宽断言或复用真实数据库取得通过。

## 四、施工保护与验收

施工前校验 frontmatter 的 HEAD 和三个文件哈希，确认目标测试不存在；保存完整 porcelain、改前 cached diff 和全部既有脏普通文件哈希。

改后依次运行：

1. Python 编译与新文件 whitespace 检查；
2. `node --test tests/frontend/examPure.test.ts`；
3. `npm run build`；
4. preview 下运行现有知识点、题库 Tab UI 特征测试和新增链接面板测试；
5. 校验所有生产文件、既有测试及全部既有脏普通文件哈希不变；
6. 排除唯一新测试后，porcelain 与改前逐字节一致；cached diff 前后均为空；
7. 当前无 staged change。

任一项失败即停止。通过后只允许标记“R2-E3 `SubjectiveLinkPanel` 特征测试前置通过”；不得在同批移动组件、改版本规则或宣称真实老师减负。

## 五、下一批边界

测试通过后才能决定 `KNOWLEDGE_RELATIONS`、`ABILITY_RESPONSE_MODES` 与组件的共同外移边界，并另立生产合同。不得复制常量、重写 link DTO 或把 `SubjectiveReviewTab` 一并移动。
