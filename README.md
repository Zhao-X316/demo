# 教辅系统（本地版）

macOS 本地教辅平台。模块化单体：共享内核 `core` + 业务模块（M1 背诵批改、M2 题目批改…）。

> 设计文档：
> - 平台架构：[`教辅系统_平台总体架构_v1.md`](./教辅系统_平台总体架构_v1.md)
> - M1 背诵批改详规：[`背诵批改系统_技术规格与实施方案_v1.md`](./背诵批改系统_技术规格与实施方案_v1.md)

## 当前进度

> 2026-07-15：独立分支 `codex/t2-artifacts@f8061b5` 已完成 T2～T6、B3a0 有序归组/评分前重拍、B3a1 普通卷结构确认纵切，以及 B3a2 固定答题卡的核心处理纵切。普通卷由老师一次确认结构后进入既有客观题机器建议；固定答题卡则绑定不可变空白卡模板和题号格位映射，使用四锚点透视校正、本地可复现 OMR、原子配准/题区/裁剪账本，再复用同一机器观察与老师终审链。两条链均不能自动计分、发布或生成学习证据。答题卡首次建模 UI、无稳定锚点答题卡的候选路径、真实照片阈值、默写 OCR/评分策略、答案结构化与三类黄金集仍未完成。未 tag、合并、推送或发布。

| 部分 | 状态 | 说明 |
|------|------|------|
| `crates/core` 共享内核 | ✅ 实现 + 单测 | 错误/实体/能力抽象(ports)/纯算法(归一化·拼音容错·相似度·正确率门控·间隔重复·hash) |
| `crates/core` DB 层 | ✅ T2 共享契约 | 迁移框架、通用仓储、`artifacts/ai_runs/background_jobs/learning_evidence/outbox/audit`；任务、提交、判定、复习卡与 effect 账本支持事务/改判/漂移保护 |
| `crates/core` 复习引擎 | ✅ 实现 + 单测 | `services/review`：pass/lapse 分账；补做通过从 stage 1 重启且保留 lapse |
| `crates/module-recitation` M1 域 | ✅ 实现 + 单测 | 文件名解析、熟练度 A/B/C、评分编排、识别 ports、rec_contents 仓储 |
| `crates/module-recitation` M1 服务 | ✅ 实现 + 单测 | import 去重/归档、ASR 可恢复状态机、机器建议、老师终审、补背/到期复习/日切、双向改判 |
| `crates/module-knowledge` K1 | 🟡 T3 兼容底座 | 教材/知识/考点/能力稳定版本，题目/答案/rubric/link 不可变版本，C0～L4 质量闸门与旧表显式映射；导入、搜索和 UI 尚未接入 |
| `crates/module-exam` M2 | 🟡 A1 + B0/B1 + M2.5 + B2 + B3a1/B3a2 核心纵切 | 手工纵切已真机通过；assessment/attempt、页面证据链、私有候选、客观题终审/发布/evidence、普通卷结构确认链，以及固定答题卡模板/格位映射/本地 OMR 合同已落地 |
| Tauri 应用外壳 `src-tauri` | ✅ 实现 + 独立检查 | DB+迁移、M1 终审，以及 T6 工作台/接受建议/人工记分/严格批量/显式发布、T6.1b 固定卷上传归档/PDF 真拆页和 Ark 题区识别 run；在线备份恢复、按 hash 归档、凭据掩码与最小 asset scope |
| 前端 `src` | 🟡 M1/M2 已接入 | 方向 B 模块切换；M1 六区、M2-A1 手工兜底，以及“上传批改”单入口、T6 标准卷按题终审、异常处理、严格批量和整卷发布页面 |
| 跨平台 CI | ✅ 后端三平台 + 应用 Win/Mac 编译 | `.github/workflows/ci.yml` |
| 打包 CI（安装包） | ✅ 工作流就绪 | `release.yml`：手动/tag 触发出 `.msi/.exe/.dmg` |
| 火山 ASR 真接口 | ✅ 标准版 submit+query | 境内端点绕系统代理；processing/ok/failed 可恢复，失败可重试、同 hash 重定位或作废 |
| 页面与老师终审 | ✅ A6b 真机通过 | 机器只给建议；老师核对录音/ASR/答案版本/评分/备注后终审，副作用才生效 |
| M2 页面证据链 / OCR | 🟡 普通卷和固定答题卡核心链已接 | 普通卷按 `structure run → teacher confirmation → alignment/region/crop → observation`；固定答题卡按 `blank template → four-anchor alignment → deterministic crop/OMR → observation`。结果仍须老师逐题终审；答题卡建模 UI、默写 OCR 和真实照片准确率未验证 |
| M2.5 题目自动沉淀 | 🟡 T5 核心合同完成 | 精确复用当前作业已固定的 K1 版本；否则创建老师私有 C0 候选；冲突/歧义进入待确认，学生卷只允许脱敏文本；相似题语义检索、候选整理 UI 和晋级尚未接入 |
| M2 拍照批改 | 🟡 普通卷与固定答题卡核心纵切完成 | 自然排序/时间核验、页数建议、起始学号/缺交、缩略图质量和完整组正式归属已落。普通卷由老师确认结构后识别；固定答题卡以空白卡、四锚点、题号格位映射和本地 OMR 识别。首次答题卡模板确认、任意答题卡候选路径、默写 OCR/评分策略尚未完成 |
| M2 傻瓜式固定卷预检 | 🟡 两类固定卷核心后台已贯通 | “上传批改”仍是唯一主入口；普通卷走视觉结构候选，固定答题卡走几何配准与 OMR，最终共用异常终审。答案自动结构化、真实照片 GUI/阈值验收、答题卡首次建模和默写专用链仍待施工 |
| 音频回放 | ✅ app-managed archive | 按 hash 归档；原文件改名、重启和数据库恢复后仍可回放，原路径只兜底 |
| ffmpeg 转码/时长探测 | ✅ 可选集成 | 装了 ffmpeg 则转 16k 单声道 wav + 探测时长，否则降级 |

当前已验证：module-exam **73 tests**、workspace **202 tests**、Tauri 应用 **37 tests**、两套 Clippy `-D warnings`、Tauri check、前端 build 和 `git diff --check` 全通过；exam 迁移已到 `exam_0015`。新增回归覆盖四锚点旋转/透视/越界拒绝、固定答题卡模板与空白图 hash 绑定、页面物化事务回滚/幂等，以及“OMR 只产生机器观察，不产生评分、发布或学习证据”。真实答题卡照片、阈值、首次模板建立 GUI、真实 provider 凭据、上传页 GUI 和三类黄金集尚未验证；正式库未写入。

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
