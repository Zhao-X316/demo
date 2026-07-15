# 教辅系统（本地版）

macOS 本地教辅平台。模块化单体：共享内核 `core` + 业务模块（M1 背诵批改、M2 题目批改…）。

> 设计文档：
> - 平台架构：[`教辅系统_平台总体架构_v1.md`](./教辅系统_平台总体架构_v1.md)
> - M1 背诵批改详规：[`背诵批改系统_技术规格与实施方案_v1.md`](./背诵批改系统_技术规格与实施方案_v1.md)

## 当前进度

> 2026-07-15：独立分支 `codex/t2-artifacts@815bff0` 已完成 T2～T6、B3a0 有序归组/评分前重拍，以及 B3a1 普通卷结构分析第一段。普通卷质量通过并正式匹配后，系统按页调用一个 provider-neutral 严格合同，生成不可变 `ordinary_paper_structure` AI run，并在上传页汇总“可继续/需复核/受阻/待处理”；模型不能新增题目、确认配准/题区、计分、发布或生成学习证据。当前仍须把成功候选落为既有 alignment/region revision 并接客观题 observation；真实照片/凭据校准、答题卡 OMR、默写 OCR、答案结构化与三类黄金集未完成。未 tag、合并、推送或发布。

| 部分 | 状态 | 说明 |
|------|------|------|
| `crates/core` 共享内核 | ✅ 实现 + 单测 | 错误/实体/能力抽象(ports)/纯算法(归一化·拼音容错·相似度·正确率门控·间隔重复·hash) |
| `crates/core` DB 层 | ✅ T2 共享契约 | 迁移框架、通用仓储、`artifacts/ai_runs/background_jobs/learning_evidence/outbox/audit`；任务、提交、判定、复习卡与 effect 账本支持事务/改判/漂移保护 |
| `crates/core` 复习引擎 | ✅ 实现 + 单测 | `services/review`：pass/lapse 分账；补做通过从 stage 1 重启且保留 lapse |
| `crates/module-recitation` M1 域 | ✅ 实现 + 单测 | 文件名解析、熟练度 A/B/C、评分编排、识别 ports、rec_contents 仓储 |
| `crates/module-recitation` M1 服务 | ✅ 实现 + 单测 | import 去重/归档、ASR 可恢复状态机、机器建议、老师终审、补背/到期复习/日切、双向改判 |
| `crates/module-knowledge` K1 | 🟡 T3 兼容底座 | 教材/知识/考点/能力稳定版本，题目/答案/rubric/link 不可变版本，C0～L4 质量闸门与旧表显式映射；导入、搜索和 UI 尚未接入 |
| `crates/module-exam` M2 | 🟡 A1 + B0/B1 + M2.5 + B2 + B3a 后端底座 | 手工纵切已真机通过；assessment/attempt、页面证据链、私有候选、客观题终审/发布/evidence，以及 OMR port、固定试卷输入/答案权威/连续分组预检与 run/题区/artifact 绑定门禁已落地 |
| Tauri 应用外壳 `src-tauri` | ✅ 实现 + 独立检查 | DB+迁移、M1 终审，以及 T6 工作台/接受建议/人工记分/严格批量/显式发布、T6.1b 固定卷上传归档/PDF 真拆页和 Ark 题区识别 run；在线备份恢复、按 hash 归档、凭据掩码与最小 asset scope |
| 前端 `src` | 🟡 M1/M2 已接入 | 方向 B 模块切换；M1 六区、M2-A1 手工兜底，以及“上传批改”单入口、T6 标准卷按题终审、异常处理、严格批量和整卷发布页面 |
| 跨平台 CI | ✅ 后端三平台 + 应用 Win/Mac 编译 | `.github/workflows/ci.yml` |
| 打包 CI（安装包） | ✅ 工作流就绪 | `release.yml`：手动/tag 触发出 `.msi/.exe/.dmg` |
| 火山 ASR 真接口 | ✅ 标准版 submit+query | 境内端点绕系统代理；processing/ok/failed 可恢复，失败可重试、同 hash 重定位或作废 |
| 页面与老师终审 | ✅ A6b 真机通过 | 机器只给建议；老师核对录音/ASR/答案版本/评分/备注后终审，副作用才生效 |
| M2 页面证据链 / OCR | 🟡 首个真实题区 provider 已接 | 固定夹具覆盖完整页面证据链；Ark 视觉适配器已按 `ObjectiveRecognizer → ai_run → observation` 接入，并拒绝模板外标签、脱敏持久化失败。当前仅处理老师确认且含 `mark_cells` 的图片裁剪，上传照片自动建立身份/题区仍待下一纵切 |
| M2.5 题目自动沉淀 | 🟡 T5 核心合同完成 | 精确复用当前作业已固定的 K1 版本；否则创建老师私有 C0 候选；冲突/歧义进入待确认，学生卷只允许脱敏文本；相似题语义检索、候选整理 UI 和晋级尚未接入 |
| M2 拍照批改 | 🟡 B3a0 评分前重拍闭环完成·三类识别器待接 | 自然排序/时间核验、页数建议、起始学号/缺交、缩略图质量和完整组正式归属已落。模糊页只扣住本学生，可逐页重拍替换；旧页保留、后续学生不顺移，整组补齐才建立正式归属。固定普通卷题区 Ark 已接，但整页机器质量/版面/题区、答题卡 OMR 与默写 OCR 尚未完成 |
| M2 傻瓜式固定卷预检 | 🟡 入站完成·真实题区识别可用 | “上传批改”以班级和已确认作业为唯一入口；JPEG/PDF 学生卷及答案候选分级归档，PDF 拆成真实单页 artifact；已确认题区可调用 Ark 视觉识别，失败可在工作台用新 run 重试。上传后自动学生/页码匹配、题区/答题格生成和答案自动结构化尚未接入 |
| 音频回放 | ✅ app-managed archive | 按 hash 归档；原文件改名、重启和数据库恢复后仍可回放，原路径只兜底 |
| ffmpeg 转码/时长探测 | ✅ 可选集成 | 装了 ffmpeg 则转 16k 单声道 wav + 探测时长，否则降级 |

当前已验证：module-exam **56 tests**、workspace **185 tests**、Tauri `--all-targets` **33 tests**（外壳 31 + 隔离夹具 2）、两套 Clippy `-D warnings`、Tauri check、前端 build 和 `git diff --check` 全通过；当前 22 个迁移。新增回归覆盖同一学生两页待重拍时“补第一页不激活、补齐第二页才整组建立 2 个正式 match”，旧页资产/历史 revision 不删除且全程 0 条评分。真实照片、机器质量/语义页型和三类黄金集尚未验证；上传页 GUI 冒烟仍未完成，正式库未写入。

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
