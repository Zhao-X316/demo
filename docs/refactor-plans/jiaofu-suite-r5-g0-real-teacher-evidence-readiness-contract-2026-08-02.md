# Jiaofu Suite R5-G0 真实老师成对证据准备与核验合同（2026-08-02）

## 1. 批次性质

R5-G0 是 R5 前置证据就绪批，不是 R5 统一任务壳设计或生产施工。当前没有可核验的仓库外真实试点工作区，本批只允许：

1. 只读定位既有 `pilot_workspace_v1.json`；
2. 冻结并复验既有真实试点 runner、老师成对报告和总证据包；
3. 在共享笔记中生成不含学生正文、凭据或可识别路径的执行清单与回执；
4. 若找不到真实工作区，明确输出 `PILOT_WORKSPACE_NOT_FOUND`，不得自动创建虚假授权、虚假老师、虚假班级或虚假 provider 数据。

不得修改 Rust/TypeScript 生产、测试、Tauri、迁移、Cargo、前端、业务数据库或真实试点资产；不得调用外部 provider；不得暂存、提交、tag、合并、推送或发布。

## 2. 改前冻结

- 分支：`codex/t2-artifacts`
- HEAD：`6072360049b09c3155726773039da139834294fe`
- Git index SHA-256：`b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b`
- staged：0；展开 porcelain：189
- R5 放行状态：未放行
- 只读搜索范围：`/Users/zzx/Documents`、`/Users/zzx/Downloads`、`/Users/zzx/agent-shared`、`/Volumes/ai-zzx`、`/tmp` 与 Spotlight
- 改前搜索结果：未发现 `pilot_workspace_v1.json`

## 3. 受保护工具链

| 路径 | 行数 | SHA-256 |
|---|---:|---|
| `crates/module-exam/src/pilot_workspace.rs` | 879 | `becdf51e2385e41bce58864ed6fddba451378fb3cc5576868febf6084ad9e816` |
| `crates/module-exam/src/pilot_workspace_run.rs` | 720 | `b0e46a57c7c59539a65395ad18c9943142317d5080b81d70d329af71db8d14c4` |
| `crates/module-exam/src/teacher_shadow.rs` | 892 | `08e092b070d471b2ea64f3d4b85f0a2365423aacd471c76cb6be352bf2a101f8` |
| `crates/module-exam/src/pilot_evidence_bundle.rs` | 765 | `f4149cd74b5c57adef7db046c8c6423c0a6e75efd352a8223714c5f64e829880` |
| `examples/init_real_pilot_workspace.rs` | 42 | `5805fb3c9eee515b00711f20158d098b2f72098e1b72114bf40826c776afa60f` |
| `examples/check_real_pilot_workspace.rs` | 37 | `98366bb8ac4ff17e0d868ad422c696e53a43de0ce62af11b68f878bab9016ed4` |
| `examples/advance_real_pilot_workspace.rs` | 92 | `f2e6496f66cda17cd6c5e4f4a6a0d0ad9ef173993da9ef520b828a64c530e35b` |
| `tests/fixtures/material_golden/README.md` | — | `c289324429339d613e897bd2200e8a7f50db0345df6308ca1c96c0d0b1e9ce8d` |

上述文件本批必须保持逐字节不变。

## 4. 必须验证

```text
cargo test -p module-exam pilot_workspace -- --nocapture
cargo test -p module-exam teacher_shadow -- --nocapture
cargo test -p module-exam pilot_evidence_bundle -- --nocapture
cargo test -p module-exam
cargo clippy -p module-exam --all-targets -- -D warnings
git diff --check
```

还必须复核：

- 工作区生成器拒绝相对路径、仓库内路径和覆盖已有目录；
- 就绪检查只读、草稿退出码为 3、blocker 稳定；
- 未就绪推进零写结果；就绪后先冻结机器影子，再等待同批老师观察；
- manual baseline 与 assisted review 必须是同一老师、同一材料、同一 sample hash、同一工作量；
- provider/model/config 只冻结不透明 hash；
- 最终 evidence bundle 始终 `release_authorized=false`；
- 没有真实工作区时，禁止运行带虚构元数据的 `init` 或 `advance`。

## 5. 可交付文档

仓库外只允许新增：

1. `R5-G0真实老师成对证据执行清单_v1.md`；
2. R5-G0 就绪审计回执；
3. 权威方案、任务地图和修改记录的事实同步。

执行清单必须将“zzx/老师需要提供的真实决定”和“工具自动生成的 hash-only 结果”分开，且只写占位符和命令，不写真实学生信息、真实文件名、provider 凭据或个人身份。

## 6. 完成口径

若仍无真实工作区，本批唯一正确结论是：

> R5-G0 工具就绪，真实输入未到；blocker=`PILOT_WORKSPACE_NOT_FOUND`；R5 未放行。

该结论不等于整个重构完成，也不证明减负、零学习成本、真实 provider、成本、稳定性、独立 `.app`、集成或发布。
