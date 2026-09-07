---
status: "authorized_local_implementation"
code_authorized: true
authorization: "user_2026-08-02_skip_review_continue"
source_commit: "6072360049b0"
source_exam_sha256: "2cdae5e967db845d864e36b9646a25bfd829bd98bcda6bea1d3162f68351b65d"
source_module_sha256: "014537aeecbaec0db54169314e69f88bd882b5f3106df0079dabf0ed81c28cb3"
source_test_sha256: "8d74ebd33501175fcdcef8a59876173ca84164246adfa0a9f267b14670e85b21"
scope: "R1.1-production-switch"
integration_authorized: false
---

# 教辅系统 R1.1 纯函数生产接线执行合同

> zzx 已明确要求“不用等审查，直接继续”。本合同据此授权一个本地、可回滚的行为保持批次；不豁免测试、工作树保护和老师终审边界，也不授权暂存、提交、tag、合并、推送或发布。

## 一、目标与唯一改动

只修改 `src/pages/Exam.tsx`：

1. 从 `./exam/examPure` 导入 `answerJsonLabel`、`displayTime`、`fileName`、`fillAnswerSlots`、`hasExtension`、`shortAnswerRubricPoints`；
2. 删除改前文件第 174–257 行的五个同名内联函数；
3. 删除改前文件第 377–379 行的 `displayTime` 内联函数。

`src/pages/exam/examPure.ts` 和 `tests/frontend/examPure.test.ts` 必须保持 E1 回执中的 SHA-256 不变。不得顺带移动其他 helper、常量、React 状态、组件、API、DTO、Rust、SQL、迁移或文案。

## 二、结构不变量

接线后的 `Exam.tsx` 必须等于以下机械变换，不能人工扩大范围：

```text
改前固定内容
  + 在改前第 89 行之前插入唯一的 6 项 named import
  - 改前第 174–257 行
  - 改前第 377–379 行
```

通过条件：

- `Exam.tsx` 中不存在 6 个目标函数的内联定义；
- `src/` 内只有 `Exam.tsx` 引用 `./exam/examPure`；
- 新模块和测试哈希不变；
- 施工前全部既有脏文件、Git index 和过滤目标文件后的 porcelain 状态不变；
- 生产行为只发生符号来源切换，不修改函数实现、调用点或业务逻辑。

## 三、施工前保护

施工前必须：

1. 校验固定提交、`Exam.tsx`、E1 模块和测试的 SHA-256；
2. 运行 `node --test tests/frontend/examPure.test.ts` 和 `npm run build`；
3. 保存完整 porcelain、Git index、全部既有脏普通文件 SHA-256；
4. 单独保存 `Exam.tsx` 改前副本。

快照建立后，除 `src/pages/Exam.tsx` 外，不得再修改仓库文件；验收回执写入仓库外权威笔记目录。

## 四、改后必跑验收

按顺序执行：

1. 用机械 `awk` 变换改前副本，与接线后的 `Exam.tsx` 做 `cmp`；
2. 校验导入唯一性、6 个内联定义全部消失、E1 模块与测试哈希不变；
3. `node --test tests/frontend/examPure.test.ts`，必须 6/6 通过；
4. `npm run build`；
5. 启动 `npm run preview -- --host 127.0.0.1`，运行 `python3 scripts/test_ordinary_question_sync_ui.py`；
6. 检查目标 diff whitespace；
7. 校验全部既有脏文件 SHA-256、Git index和排除 `src/pages/Exam.tsx` 后的 porcelain 状态与施工前一致。

任一项失败即停止，不进入 R2，不以单个构建或 UI 结果代替整套验收。

## 五、完成后的证据边界

本批通过后只允许表述：

> R1.1 首批 6 个纯函数已完成行为保持式生产接线，直接测试、前端构建和普通试卷批改台 UI 冒烟通过。

不得表述为 `Exam.tsx` 已完成整体拆分、批改台全面验收、真实老师减负已验证，或系统已具备发布条件。
