---
status: "authorized_local_implementation"
code_authorized: true
authorization: "goal_2026-08-02_continuous_refactor"
source_commit: "6072360049b09c3155726773039da139834294fe"
source_exam_sha256: "9d7acd3ba3c63acfa9e494b86a8f955f81930cc82209886ec49fa2be2e42220b"
source_ui_test_sha256: "e345bd84b97e20f122e8dc0557954c8a42630ec77ea7bd2c09f10f0ebdd0ff94"
scope: "R2-KnowledgeTab-production-extraction"
integration_authorized: false
---

# 教辅系统 R2 `KnowledgeTab` 生产外移执行合同

> 用户当前目标是“持续推进，完成重构”。本合同据此授权一个本地、可回滚的行为保持批次：只把已经有浏览器特征测试保护的 `KnowledgeTab` 从 `Exam.tsx` 机械外移。合同不授权行为重写、其他 Tab 拆分、暂存、提交、tag、合并、推送或发布。

## 一、目标与唯一结构因素

本批只改变 `KnowledgeTab` 的文件归属，不改变其输入、状态、命令或界面行为。

唯一允许修改：

| 文件 | 动作 |
|---|---|
| `src/pages/exam/KnowledgeTab.tsx` | 新建；保存当前内联组件及其最小依赖 |
| `src/pages/Exam.tsx` | 导入新组件，删除内联组件，删除不再由本文件使用的 `kpCreate` import |

禁止修改：

- `scripts/test_exam_knowledge_tab_ui.py`、`src/pages/exam/examPure.ts` 和既有测试；
- `src/api/exam.ts`、CSS、package/lockfile、tsconfig、Vite 配置或依赖；
- 其他 Tab、helper、React 状态或父级 `load/done` 编排；
- Rust、Tauri command、IPC/DTO、SQL、迁移、audit/outbox 或业务文案。

## 二、固定机械变换

### 新组件文件

`src/pages/exam/KnowledgeTab.tsx` 必须只包含：

1. 从 React 导入 `useMemo`、`useState`；
2. 从 `../../api/exam` 导入 `kpCreate`，并以 type-only 方式导入 `KnowledgePoint`；
3. 导出 `KnowledgeTab`；
4. 组件 props、局部状态、`roots`、`save` 和 JSX 与改前内联组件逐字节等价，只允许增加 `export` 和因独立文件需要的 imports。

### `Exam.tsx`

只允许：

1. 在现有 `examPure` import 后增加 `import { KnowledgeTab } from "./exam/KnowledgeTab";`；
2. 从 `../api/exam` named import 删除 `kpCreate`；
3. 删除改前第 3698–3755 行：内联 `KnowledgeTab` 定义及其紧邻的单个分隔空行。

父级调用点必须保持原样；`KnowledgePoint` 仍由 `Exam.tsx` 使用，不得删除。

## 三、行为不变量

- props 仍为 `knowledge / onDone / onError`；
- 名称为空仍只调用 `onError("知识点名称不能为空")`，不调用后端；
- `kpCreate` 参数和顺序仍为 `name.trim(), code.trim() || null, parentId || null`；
- 成功后仍只清空名称和编码，再调用 `onDone("知识点已创建")`；父级 `done → load()` 刷新语义不变；
- 失败仍将 `String(err)` 交给 `onError`，保留表单输入，不伪造本地节点；
- 父级候选仍只显示根节点，树仍只显示根节点及其直接子节点；
- 老师终审、成绩发布和学习证据边界零变化。

## 四、施工前保护

施工前必须：

1. 校验 HEAD、`Exam.tsx` 和 UI 特征测试 SHA-256 与 frontmatter 一致；
2. 确认目标组件文件不存在；
3. 运行 `node --test tests/frontend/examPure.test.ts`、`npm run build` 和 `python3 scripts/test_exam_knowledge_tab_ui.py`；
4. 保存完整 porcelain、Git index、全部既有脏普通文件 SHA-256，以及 `Exam.tsx` 改前副本。

建立快照后，只允许修改本合同列出的两个生产文件；验收回执写入仓库外权威笔记目录。

## 五、改后验收

按顺序执行：

1. 机械提取比较：新组件去除独立 imports 和 `export` 后，与改前内联组件逐字节一致；
2. `Exam.tsx` 与固定的“增加唯一 import、删除 `kpCreate` import、删除固定内联段”机械变换逐字节一致；
3. `rg` 确认组件定义只在新文件、父级调用点仍唯一、`kpCreate` 只由新组件导入；
4. UI 特征测试脚本和 `examPure` 模块/测试哈希不变；
5. `node --test tests/frontend/examPure.test.ts` 必须 6/6 通过；
6. `npm run build`；
7. 启动 preview 后运行 `python3 scripts/test_exam_knowledge_tab_ui.py`；
8. 两个目标文件 whitespace 检查；
9. 施工前全部既有脏文件（排除 `Exam.tsx`）哈希不变，过滤两个目标文件后的 porcelain 与完整 Git index 前后逐字节一致。

任一项失败即停止，不扩大范围修补。

## 六、完成口径与后续

通过后只允许表述：

> R2 首个完整 `KnowledgeTab` 已完成行为保持式生产外移，浏览器特征测试、前端构建、机械比较和工作树保护通过。

不得表述为 `Exam.tsx` 已完成整体重构、其他 Tab 可无测试直接拆分、状态编排已优化、老师减负已验证或系统可以发布。下一候选必须重新盘点依赖与测试缺口，另立合同。
