# 教辅系统（本地版）

macOS 本地教辅平台。模块化单体：共享内核 `core` + 业务模块（M1 背诵批改…）。

> 设计文档：
> - 平台架构：[`教辅系统_平台总体架构_v1.md`](./教辅系统_平台总体架构_v1.md)
> - M1 背诵批改详规：[`背诵批改系统_技术规格与实施方案_v1.md`](./背诵批改系统_技术规格与实施方案_v1.md)

## 当前进度（P0 平台骨架）

| 部分 | 状态 | 说明 |
|------|------|------|
| `crates/core` 共享内核 | ✅ 已实现 + 单测通过 | 错误/实体/能力抽象(ports)/纯算法(归一化·拼音容错·相似度·正确率门控·间隔重复) |
| `crates/module-recitation` M1 | ✅ 域逻辑 + 单测通过 | 文件名解析·熟练度·评分编排·火山ASR占位 |
| Tauri 应用外壳 `src-tauri` | ⏳ 待接线 | 在 macOS 上构建；DB(rusqlite)/commands/迁移执行 |
| 前端 `src` | ⏳ 待接线 | React shell + 模块注册表 + 各模块页面 |
| 火山 ASR 真接口 | ⏳ 占位 | `asr_volcano.rs` 待接 reqwest |

已验证：`cargo test`（28 项通过）、`cargo clippy`（0 警告），跨平台可在 Linux/macOS 运行。

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
