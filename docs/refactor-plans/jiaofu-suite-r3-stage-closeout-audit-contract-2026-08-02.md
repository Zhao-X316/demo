# Jiaofu Suite R3 阶段收口审计合同（2026-08-02）

## 1. 审计目标

本批只读审计 R3 前端状态与请求编排是否满足 `jiaofu-suite-r3-frontend-orchestration-design-contract-2026-08-02.md` 的九项完成标准。通过时，只允许把 R3 记为“前端状态/请求编排重构完成（本地自动化，未集成、未发布）”；不得扩大为真实 OCR/AI/provider 已验证、老师已减负、独立 `.app` 已验收、R4/R5 自动完成或系统已发布。

## 2. 冻结基线

- 分支：`codex/t2-artifacts`
- HEAD：`6072360049b09c3155726773039da139834294fe`
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`
- 审计开始时展开 porcelain：167 条；staged：0
- 固定上传 13 个目标生产文件哈希清单：`e101ec44...3be3`
- 三个终审工作台 7 个目标生产文件哈希清单：`fe9821ff...177c`
- W1/C2/C1/H1 四个审计器哈希清单：`1cb7417c...e7fe`
- 四个前端直接测试哈希清单：`4ee8faa0...1689`
- 27 个 UI 脚本哈希清单：`12c9e0ba...baf5`
- `Exam.tsx` / Exam API：`ed4132ca...e9bc` / `1a796762...c164`
- package / lock：`a3f0afa7...69b0` / `8f627a00...adf4`

## 3. 允许范围

仓库内只允许新增本合同。审计过程只读；仓库外可新增 R3-A 回执并更新实施标准、文档索引、任务地图和修改记录索引。禁止修改生产、测试、审计器、API、Rust/SQL/DTO、依赖、CSS、构建配置或历史回执。

## 4. 必须成立的九项完成标准

1. `FixedIntakeTab` 不直接持有旧 41 个平铺 state、5 个 request effect 或 23 个唯一业务命令，只组合 root controller 和展示面板；
2. reducer/runtime 只保存瞬时 UI 投影，后端批次/run/终审/发布仍为唯一事实权威；
3. 展示表单/Tab/controller 公共边界没有 raw setter，文件选择动作保持内部依赖；
4. prepare 重试 key、答案版本、归组、质量、三材料逐页部分成功和题库旁路语义有直接/UI 回归保护；
5. 23 个固定上传业务命令实现同步防重入、过期 completion 静默与 StrictMode 去重，H1 为 17/17 操作族、26/26 调用点；
6. 父级等值 options 刷新保持合法 session，完整 reload 明确 fail-closed；
7. Objective/Dictation/Subjective 三个工作台各自完成 controller/语义动作收口，W1 为 27/27；
8. C2 10/10、C1 19/19、H1 17/17/26/26、W1 27/27、65/65、build、Python、whitespace 和 27/27 UI 全绿，工作树保护通过；
9. R3-D 至 W1-V1 共 43 份分批合同及对应 43 个 `00-*` 验收回执存在，并新增独立 R3-A 阶段收口回执。

## 5. 结构和行为冻结

- 固定上传 provider 库存必须保持 23 个唯一命令、26 个调用点与规范 hash `375a66bf...8ba4`；终审工作台保持 20 个唯一命令、22 个调用点与规范 hash `ef7b4508...772f`。
- `FixedIntakeTab`、root controller、runtime、view model 与六个命令模块继续满足既有行预算、无循环依赖和 provider 所有权；不以行数下降单独判完成。
- 老师终审、发布、答案/rubric 未来版本、原始证据、失败保留与题库旁路边界不得变化。
- R3-A 不解决完整刷新草稿恢复、真实 provider/材料准确率、老师耗时、独立 `.app` 或 Rust use-case；这些不是 R3 完成标准。

## 6. 工程门禁

必须从零执行并通过：

1. W1/C2/C1/H1 四个审计器；
2. 4 个前端直接测试，共 65 项；
3. `npm run build`；
4. 27 个 UI 脚本 `py_compile` 与浏览器回归；
5. `git diff --check`；
6. 冻结哈希、HEAD、Git index、staged 与展开 porcelain 保护。

## 7. 判定规则与边界

- 任一完成标准或门禁失败：R3 保持“阶段收口待完成”，另立修复合同；不得在本批改生产。
- 全部成立：R3 可标记为“前端状态/请求编排重构完成（本地自动化，未集成、未发布）”。展开 porcelain 只能 `167→168`，唯一新增仓库路径为本合同。
- R3 通过后只能开始 R4 Rust use-case 的只读盘点/设计；R4 生产拆分、R5 统一任务壳、真实老师/provider、tag、合并、推送和发布仍须独立授权与证据。
