# 教辅系统（本地版）

macOS 本地教辅平台。模块化单体：共享内核 `core` + 业务模块（M1 背诵批改…）。

> 设计文档：
> - 平台架构：[`教辅系统_平台总体架构_v1.md`](./教辅系统_平台总体架构_v1.md)
> - M1 背诵批改详规：[`背诵批改系统_技术规格与实施方案_v1.md`](./背诵批改系统_技术规格与实施方案_v1.md)

## 当前进度

| 部分 | 状态 | 说明 |
|------|------|------|
| `crates/core` 共享内核 | ✅ 实现 + 单测 | 错误/实体/能力抽象(ports)/纯算法(归一化·拼音容错·相似度·正确率门控·间隔重复·hash) |
| `crates/core` DB 层 | ✅ 实现 + 单测 | 迁移框架 + 11张通用表 + 仓储(settings/students/memory_cards/file_ledger/submissions/tasks/verdicts) |
| `crates/core` 复习引擎 | ✅ 实现 + 单测 | `services/review`：record_pass/lapse（间隔重复编排） |
| `crates/module-recitation` M1 域 | ✅ 实现 + 单测 | 文件名解析·熟练度A/B/C·评分编排·火山ASR占位·rec_contents仓储 |
| `crates/module-recitation` M1 服务 | ✅ 实现 + 单测 | import(去重/解析/匹配/异常池)·scoring(评分→判定→复习/补背)·tasks(补背/到期复习/日切) |
| Tauri 应用外壳 `src-tauri` | ✅ 实现 + CI 编译通过 | state(DB+迁移)/commands(dashboard/human_decide/seed_demo)/main |
| 前端 `src`（今日看板） | ✅ 实现 + 构建通过 | React shell + 模块注册表 + 看板(三组卡片+人工判定) |
| 跨平台 CI | ✅ 后端三平台 + 应用 Win/Mac 编译 | `.github/workflows/ci.yml` |
| 打包 CI（安装包） | ✅ 工作流就绪 | `release.yml`：手动/tag 触发出 `.msi/.exe/.dmg` |
| 火山 ASR 真接口 | ⏳ 占位 | `asr_volcano.rs` 待接 reqwest |
| 其余页面（导入/异常/管理/设置） | ⏳ 待补 | 看板已先行 |

已验证（CI 实测）：后端 55 测试在 **Windows/macOS/Linux** 全通过；前端 `tsc+vite` 构建通过；**Tauri 应用在 macOS 上编译通过**（Windows 同步验证中）。

> 整个应用（后端 + Tauri 外壳 + React 看板）已在真实 macOS 上完整编译。`release.yml` 可一键产出未签名安装包。

## 构建安装包

- 在 GitHub 仓库 Actions 页手动运行 **Release Build**，或推一个 `v*` tag。
- 完成后在该次运行的 Artifacts 下载 `jiaofu-suite-windows-latest`（.msi/.exe）或 `jiaofu-suite-macos-latest`（.dmg）。
- 未签名：Windows 经 SmartScreen「更多信息→仍要运行」；macOS 右键打开/系统设置放行。

## 工作区结构

```
.
├─ Cargo.toml                  # workspace（macOS 构建时再加入 src-tauri）
├─ crates/
│  ├─ core/                    # 共享内核（包名 suite-core，导入名 suite_core）
│  │  └─ src/{error,models,ports, domain/{normalize,pinyin_util,similarity,accuracy,scheduler}}.rs
│  └─ module-recitation/       # M1
│     ├─ src/{lib,grader,asr_volcano, domain/{filename,fluency}}.rs
│     └─ migrations/0001_recitation.sql
└─ docs（即根目录的两份 .md）
```

> ⚠️ 内核 crate 名为 `suite-core`（不可叫 `core`，会与 Rust 标准库 `core` 冲突）。模块内 `use suite_core::...`。

## 开发命令

```bash
# 纯逻辑构建与测试（任意平台）
cargo test
cargo clippy --workspace --all-targets

# macOS 上构建 App（后续接入 src-tauri 后）
# npm install && npm run tauri build
```

## 安全红线

- 云凭据（火山 App ID / Access Token）只存本机 `secrets.json`（0600），**绝不入库、不打进包**。
- `secrets.json`、`*.db` 已在 `.gitignore`。

## 加新模块（M2/M3…）

见平台架构 §8：新建 `crates/module-xxx` 依赖 `suite-core` → 实现 `Module` 契约 + 模块专属表 → 在 `src-tauri` 注册 → 前端 `moduleRegistry` 加一项。core 与其他模块零改动。
