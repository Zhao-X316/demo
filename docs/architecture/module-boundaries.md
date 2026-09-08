# 模块职责与本地验证

本项目采用 React 页面、Tauri 命令和 Rust 业务模块三层结构。重构保留原有页面与 API 公共入口；数据库迁移、老师终审、追加式版本、幂等与发布边界沿用原契约。

## 前端

| 入口 | 内部职责 |
| --- | --- |
| `src/App.tsx`、`src/pages/workspace/AppShell.tsx` | 工作台 / 资料 / 学生三项主导航，设置在底部；共享班级筛选，保留已访问页面中的草稿。 |
| `src/pages/workspace/` | 已保存任务列表、当前任务核对队列和结果；仅保存领域定位，评分与发布分别调用现有命令。 |
| `src/pages/Exam.tsx` | 在本次批次或答卷范围内组合资料、处理、老师核对、结果；复用 `exam/` 的 controller、材料入站状态与生命周期边界。 |
| `src/pages/QuestionBank.tsx` | 保留组卷入口；`question-bank/` 分别管理来源导入、答案、关联、检索、候选复核、表现与版本影响、题卷编辑。 |
| `src/pages/LearningInsights.tsx` | 保留班级/学生筛选；`learning-insights/` 分别管理个人快照、错题报告、单题干预、排期策略。 |
| `src/pages/ClassDashboard.tsx` | 负责班级视图；`class-dashboard/useClassDashboardController.tsx` 管理读取、快照与课堂事件操作，其余文件负责教学输入、教学行动与节点详情。 |

页面内部模块只向入口提供需要组合的组件或状态。共享格式化与标签放在各自功能目录，不新建跨业务的全局工具层。组件的 hook 仍在原组件生命周期内；班级 controller 在每次页面渲染时无条件调用。

`src/api/{exam,knowledge,learning,classDashboard}.ts` 是兼容导出入口。对应同名目录按业务域保存类型与命令，内部跨域依赖使用具体文件；类型依赖不引入运行时调用。`src/api/client.ts` 仍是统一 IPC 调用入口。新增字段或命令应修改所属域，避免在兼容入口复制定义。

## Rust 与桌面外壳

| 位置 | 职责与约束 |
| --- | --- |
| `crates/core` | 通用领域、仓储和端口；现有分层沿用。 |
| `crates/module-recitation` | 背诵流水线与评分；评分场景测试在 `service/scoring_tests.rs`。 |
| `crates/module-exam/src/service/subjective.rs` | 转写、评分建议、老师复核和工作台；`subjective/accepted_answers.rs` 与 `rubric_evidence.rs` 分别创建未来答案和评分规则版本。 |
| `crates/module-exam/src/service/question_performance/` | 沿用已有表现、影响计划、复核与发布职责划分。 |
| `crates/module-knowledge` | 沿用 taxonomy、题库版本、答案、关联、来源等模块；不改变版本与去重规则。 |
| `crates/module-wrongbook` | 错题读模型与干预；读模型测试在 `read_model_tests.rs`。 |
| `crates/module-profile/src/profile/` | `contracts.rs` 定义快照协议；`metrics.rs` 只计算聚合，不访问数据库；父模块管理证据读取、快照持久化与查询。 |
| `src-tauri/src/workspace.rs`、`src-tauri/src/exam_intake/resume.rs` | 只读聚合原领域任务、当前处理记录、分组和批改范围；不建通用任务表。恢复页面不调用识别、评分或发布命令。 |
| `src-tauri/src/exam_intake/` | `contracts.rs` 定义 UI 协议；`files.rs` 管理本地文件准备与归档，不访问数据库或 provider；父模块负责入站事实、分组与事务协调。 |
| `src-tauri/src/diagnostics.rs` | 诊断导出逻辑；隔离测试在 `diagnostics_tests.rs`，保留脱敏与导出审计。 |

大型内联测试移到相邻 `*_tests.rs`，通过 `#[cfg(test)]` 和 `#[path = ...] mod tests;` 加载，测试模块路径和访问范围保持。生产调用方继续使用原 public 路径。既有写命令与事务边界沿用。工作台新增只读 DTO；三个批改读模型在 SQL 限额前按答卷 ID 筛选，空范围返回空结果。

## 验证入口

在仓库根目录执行：

```sh
npm run check
```

它顺序运行前端单测、4 项现有 Exam 边界审计、生产构建，以及 Rust workspace 和独立 Tauri 包的测试与 Clippy。依赖为现有 Node、npm、Cargo 和 macOS Tauri 构建环境；命令不会安装依赖。日志与逐项退出结果保存在临时目录，也可指定目录：

```sh
npm run check -- local /tmp/jiaofu-check-local
npm run check:frontend
npm run check:backend
```

浏览器验收另开一个终端启动临时预览，再执行全部已有 UI 脚本：

```sh
npm run dev -- --host 127.0.0.1 --port 4173 --strictPort
npm run check:ui
```

UI 脚本需要 Python 3 与现有 Playwright/Chromium，使用合成 Tauri mock。它们验证页面交互和命令参数，不构成真实老师使用验收或生产环境证明。React StrictMode 可以重复初始读取，老师操作的写命令仍按每次一次核验。

生产构建仍有既存的主 bundle 超过 500 kB 提示；本次职责拆分没有引入新的加载策略。全项目检查不会执行部署、真实数据写入或 PEW 正式状态库操作。
