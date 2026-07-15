# 教辅系统（本地版）

macOS 本地教辅平台。模块化单体：共享内核 `core` + 业务模块（M1 背诵批改、M2 题目批改…）。

> 设计文档：
> - 平台架构：[`教辅系统_平台总体架构_v1.md`](./教辅系统_平台总体架构_v1.md)
> - M1 背诵批改详规：[`背诵批改系统_技术规格与实施方案_v1.md`](./背诵批改系统_技术规格与实施方案_v1.md)

## 当前进度

> 2026-07-14：M1 A6b 隔离真机 G01-G16 与 M2-A1 手工客观题纵切均已通过。独立分支 `codex/t2-artifacts` 已完成 T2～T5；T6 已完成服务核心、桌面工作台、独立固定夹具真 `.app` 终审/发布/重启持久化，以及 provider 无关的 OMR 输入输出合同和 `ai_run → observation` 绑定门禁。T6.1a/T6.1b 已补固定试卷输入、答案权威层级、连续分组、三路预检、一站式上传入口、本地归档和 PDF 真拆页。真实扫描/拍照 OCR/OMR provider、答案资料自动结构化与真实黄金集仍未接入；所有范围仍未 tag、合并、推送或发布。

| 部分 | 状态 | 说明 |
|------|------|------|
| `crates/core` 共享内核 | ✅ 实现 + 单测 | 错误/实体/能力抽象(ports)/纯算法(归一化·拼音容错·相似度·正确率门控·间隔重复·hash) |
| `crates/core` DB 层 | ✅ T2 共享契约 | 迁移框架、通用仓储、`artifacts/ai_runs/background_jobs/learning_evidence/outbox/audit`；任务、提交、判定、复习卡与 effect 账本支持事务/改判/漂移保护 |
| `crates/core` 复习引擎 | ✅ 实现 + 单测 | `services/review`：pass/lapse 分账；补做通过从 stage 1 重启且保留 lapse |
| `crates/module-recitation` M1 域 | ✅ 实现 + 单测 | 文件名解析、熟练度 A/B/C、评分编排、识别 ports、rec_contents 仓储 |
| `crates/module-recitation` M1 服务 | ✅ 实现 + 单测 | import 去重/归档、ASR 可恢复状态机、机器建议、老师终审、补背/到期复习/日切、双向改判 |
| `crates/module-knowledge` K1 | 🟡 T3 兼容底座 | 教材/知识/考点/能力稳定版本，题目/答案/rubric/link 不可变版本，C0～L4 质量闸门与旧表显式映射；导入、搜索和 UI 尚未接入 |
| `crates/module-exam` M2 | 🟡 A1 + B0/B1 + M2.5 + B2 + B3a 后端底座 | 手工纵切已真机通过；assessment/attempt、页面证据链、私有候选、客观题终审/发布/evidence，以及 OMR port、固定试卷输入/答案权威/连续分组预检与 run/题区/artifact 绑定门禁已落地 |
| Tauri 应用外壳 `src-tauri` | ✅ 实现 + 独立检查 | DB+迁移、M1 终审，以及 T6 工作台/接受建议/人工记分/严格批量/显式发布、T6.1b 固定卷上传归档与 PDF 真拆页命令；在线备份恢复、按 hash 归档、凭据掩码与最小 asset scope |
| 前端 `src` | 🟡 M1/M2 已接入 | 方向 B 模块切换；M1 六区、M2-A1 手工兜底，以及“上传批改”单入口、T6 标准卷按题终审、异常处理、严格批量和整卷发布页面 |
| 跨平台 CI | ✅ 后端三平台 + 应用 Win/Mac 编译 | `.github/workflows/ci.yml` |
| 打包 CI（安装包） | ✅ 工作流就绪 | `release.yml`：手动/tag 触发出 `.msi/.exe/.dmg` |
| 火山 ASR 真接口 | ✅ 标准版 submit+query | 境内端点绕系统代理；processing/ok/failed 可恢复，失败可重试、同 hash 重定位或作废 |
| 页面与老师终审 | ✅ A6b 真机通过 | 机器只给建议；老师核对录音/ASR/答案版本/评分/备注后终审，副作用才生效 |
| M2 页面证据链 / OCR | 🟡 T4 契约 + OMR port 完成 | 固定夹具已覆盖原图 artifact、页面质量、学生/页码匹配、模板配准、题区裁剪、异常池、重启恢复和完整追溯；真实 OCR/OMR provider 与真实样本 UI 尚未接入 |
| M2.5 题目自动沉淀 | 🟡 T5 核心合同完成 | 精确复用当前作业已固定的 K1 版本；否则创建老师私有 C0 候选；冲突/歧义进入待确认，学生卷只允许脱敏文本；相似题语义检索、候选整理 UI 和晋级尚未接入 |
| M2 标准卷客观题 | 🟡 T6 真 `.app` + OMR port 完成·待真实 provider | 固定隔离夹具已验证选择/判断观察、2 条严格批量、4 条异常逐条人工记分、6 份显式发布与 12 条正式学习证据；OMR run 不能跨题区/裁剪绑定，失败元数据必须脱敏；真实扫描/拍照识别仍未接入 |
| M2 傻瓜式固定卷预检 | 🟡 T6.1a/T6.1b 入站闭环完成 | “上传批改”以班级和已确认作业为唯一入口；JPEG/PDF 学生卷及图片/PDF/TXT/粘贴答案可按隐私级别归档，PDF 拆成真实单页 artifact；连续分组与 `可批量确认/需复核/阻断` 三路预检复用既有事务状态机。答案文件当前只是候选资料；真实 OCR/OMR、学生/页码匹配、题区识别和答案自动结构化尚未接入 |
| 音频回放 | ✅ app-managed archive | 按 hash 归档；原文件改名、重启和数据库恢复后仍可回放，原路径只兜底 |
| ffmpeg 转码/时长探测 | ✅ 可选集成 | 装了 ffmpeg 则转 16k 单声道 wav + 探测时长，否则降级 |

当前已验证：module-exam **46 tests**、workspace **175 tests**、Tauri `--all-targets` **21 tests**（外壳 19 + 隔离夹具 2）、workspace/Tauri 两套 Clippy `-D warnings`、Tauri check、前端 `tsc+vite` 和 `git diff --check` 全通过；`npm run acceptance:t6:build` 生成 bundle id 为 `com.jiaofu.suite.t6fixture` 的独立 `.app`。固定夹具完成 18 个迁移、6 份作答、2 条严格批量、4 条异常人工终审、6 个发布 revision 和 12 条正式学习证据，`integrity_check=ok`、外键违规 0、归档缺失 0；重启同一隔离 HOME 后 GUI 仍显示 6/6 已终审和 6/6 已发布，证据裁剪可见。T6.1a 有 7 条后端专项测试；T6.1b 新增 7 条 Tauri 专项测试，覆盖上传选项、JPEG/答案归档隐私、幂等重试、PDF 真拆页、重复内容和非法格式的写入前拒绝。真实 OCR/AI provider、真实扫描/拍照黄金集和阈值校准仍待完成；上传页 GUI 冒烟因当前环境缺少 Playwright 尚未验证，正式库未写入。完整 debug DMG 的既有本机打包问题仍未冒充发布完成。

> 这些结果不等于授权发布。异模型复核、tag、合并和安装包发布仍需单独放行。

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
│  ├─ module-recitation/       # M1
│  │  ├─ src/{lib,grader,asr_volcano, domain/{filename,fluency}}.rs
│  │  └─ migrations/0001_recitation.sql
│  ├─ module-knowledge/         # K1：教材/知识/题目/答案/rubric/link 版本底座
│  └─ module-exam/             # M2：页面证据链 + OMR port + assessment/grade/publication + 私有候选 + 客观题终审/正式证据
├─ src-tauri/                  # 桌面外壳与 IPC 命令
└─ src/                        # React 前端
```

> ⚠️ 内核 crate 名为 `suite-core`（不可叫 `core`，会与 Rust 标准库 `core` 冲突）。模块内 `use suite_core::...`。

## 开发命令

```bash
# workspace 纯逻辑（不包含 src-tauri）
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

# Tauri 外壳必须单独检查
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo check --manifest-path src-tauri/Cargo.toml

# 前端与本地 App
npm run build
npm run tauri build

# T6 独立验收包（独立 bundle id，不读取正式应用目录）
npm run acceptance:t6:build
cargo test --manifest-path src-tauri/Cargo.toml --example t6_objective_fixture
```

## 安全红线

- 云凭据（火山 App ID / Access Token）只存本机 `secrets.json`（0600），**绝不入库、不打进包**。
- `secrets.json`、`*.db` 已在 `.gitignore`。

## 加新模块（M2/M3…）

见平台架构 §8：新建 `crates/module-xxx` 依赖 `suite-core` → 实现 `Module` 契约 + 模块专属表 → 在 `src-tauri` 注册 → 前端 `moduleRegistry` 加一项。core 与其他模块零改动。
