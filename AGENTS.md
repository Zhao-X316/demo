# AGENTS — 教辅系统 demo 本地入口

本目录是教辅系统 demo 的本地调试副本。项目上下文的权威来源在：

- 全局协作公约：`/Users/zzx/agent-shared/笔记/AGENTS.md`
- 项目 Lean Context：`/Users/zzx/agent-shared/笔记/任务地图/教辅系统demo.md`
- 项目笔记：`/Users/zzx/agent-shared/笔记/教辅系统/`
- 修改记录队列：`/Users/zzx/agent-shared/笔记/修改记录/教辅系统demo/未审查修改记录.md`

接手顺序：

1. 先读全局协作公约。
2. 再读任务地图 `教辅系统demo.md` 的 `🧠 Lean Context`。
3. 优先读任务地图锚点里的 `踩坑与经验`、总体架构、M1 规格、README 和当前分支状态。

硬约束：

- 云凭据只存在本机安全文件，绝不入库或打包。
- `cargo test --workspace` 不覆盖 `src-tauri`，涉及 Tauri/Rust 外壳时还要跑对应检查。
- 保持老师终审权，AI 判分只作为辅助。
