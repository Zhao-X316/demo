# 教辅系统（本地版）

macOS 本地教辅平台。模块化单体：共享内核 `core` + 业务模块（M1 背诵批改、M2 题目批改…）。

> 设计文档：
> - 平台架构：[`教辅系统_平台总体架构_v1.md`](./教辅系统_平台总体架构_v1.md)
> - M1 背诵批改详规：[`背诵批改系统_技术规格与实施方案_v1.md`](./背诵批改系统_技术规格与实施方案_v1.md)

## 当前进度

> 2026-07-15：独立分支 `codex/t2-artifacts` 已完成 T2～T6、B3a0、B3a1 普通卷纵切、B3a2 固定答题卡首建/复用、M2-C0 固定格式默写正式终审，以及答案图片/TXT/PDF/DOCX/XLSX 结构化、冲突预检和“采用上传答案与评分点，另存新版本”。简答题新版本会保留稳定评分点身份并按顺序沿用现有知识/能力链接，机器建议与老师确认级别保持不变；评分点数量或总分变化时整笔拒绝，不猜测映射。默写 OCR 仍只产生追加式转写和评分点建议；老师可逐条接受、按原图补录/记分，或严格批量确认置信度不低于 0.95 的精确结果。整份默写必须显式发布，发布后才按 rubric point 生成正式学习证据；OCR 原文、老师校正、评分 revision 和发布 revision 均可追溯。普通试卷、答题卡和默写现已具备分开的合成黄金集合同和 hash-only 离线评估报告；真实材料评估新增试点数据闸门，必须冻结用途/范围/期限/授权/导出删除演练/诊断默认/云端 TTL 与策略 hash，合同夹具、草稿、过期或内容漂移一律拒绝。真实照片/provider/GUI、实际导出删除能力和三类脱敏真图黄金集仍未完成，合成结果不得宣称生产准确率。未 tag、合并、推送或发布。

| 部分 | 状态 | 说明 |
|------|------|------|
| `crates/core` 共享内核 | ✅ 实现 + 单测 | 错误/实体/能力抽象(ports)/纯算法(归一化·拼音容错·相似度·正确率门控·间隔重复·hash) |
| `crates/core` DB 层 | ✅ T2 共享契约 | 迁移框架、通用仓储、`artifacts/ai_runs/background_jobs/learning_evidence/outbox/audit`；任务、提交、判定、复习卡与 effect 账本支持事务/改判/漂移保护 |
| `crates/core` 复习引擎 | ✅ 实现 + 单测 | `services/review`：pass/lapse 分账；补做通过从 stage 1 重启且保留 lapse |
| `crates/module-recitation` M1 域 | ✅ 实现 + 单测 | 文件名解析、熟练度 A/B/C、评分编排、识别 ports、rec_contents 仓储 |
| `crates/module-recitation` M1 服务 | ✅ 实现 + 单测 | import 去重/归档、ASR 可恢复状态机、机器建议、老师终审、补背/到期复习/日切、双向改判 |
| `crates/module-knowledge` K1 | 🟡 T3 兼容底座 | 教材/知识/考点/能力稳定版本，题目/答案/rubric/link 不可变版本，C0～L4 质量闸门与旧表显式映射；导入、搜索和 UI 尚未接入 |
| `crates/module-exam` M2 | 🟡 A1 + B0/B1 + M2.5 + B2 + B3a1/B3a2/B3c3 + M2-C0 | 普通卷、固定答题卡、固定默写首建/复用与正式终审链、五类答案源结构化/冲突预检及客观题、固定填空、固定简答答案另存新版本已落；三材料合成黄金合同、只读评估器和真实材料试点闸门合同已落，真实脱敏样本与实际数据权利流程仍缺；默写机器结果仍只是建议，简答评分点结构变化仍须老师逐点补录映射 |
| Tauri 应用外壳 `src-tauri` | ✅ 实现 + 独立检查 | DB+迁移、M1 终审，以及 T6 工作台/接受建议/人工记分/严格批量/显式发布、T6.1b 固定卷上传归档/PDF 真拆页和 Ark 题区识别 run；在线备份恢复、按 hash 归档、凭据掩码与最小 asset scope |
| 前端 `src` | 🟡 M1/M2 已接入 | 方向 B 模块切换；M1 六区、M2-A1 手工兜底，以及“上传批改”单入口、T6 标准卷按题终审、异常处理、严格批量和整卷发布页面 |
| 跨平台 CI | ✅ 后端三平台 + 应用 Win/Mac 编译 | `.github/workflows/ci.yml` |
| 打包 CI（安装包） | ✅ 工作流就绪 | `release.yml`：手动/tag 触发出 `.msi/.exe/.dmg` |
| 火山 ASR 真接口 | ✅ 标准版 submit+query | 境内端点绕系统代理；processing/ok/failed 可恢复，失败可重试、同 hash 重定位或作废 |
| 页面与老师终审 | ✅ A6b 真机通过 | 机器只给建议；老师核对录音/ASR/答案版本/评分/备注后终审，副作用才生效 |
| M2 页面证据链 / OCR | 🟡 三材料固定版式纵切已接 | 普通卷按 `structure run → teacher confirmation → alignment/region/crop → observation`；答题卡按 `blank image → template run → one-time teacher confirmation → four-anchor alignment → deterministic crop/OMR → observation`；默写按 `blank image → template/policy confirmation → aligned derivative/region crop → answer-free OCR → exception review`。真实照片准确率未验证 |
| M2.5 题目自动沉淀 | 🟡 T5 核心合同完成 | 精确复用当前作业已固定的 K1 版本；否则创建老师私有 C0 候选；冲突/歧义进入待确认，学生卷只允许脱敏文本；相似题语义检索、候选整理 UI 和晋级尚未接入 |
| M2 拍照批改 | 🟡 普通卷/固定答题卡/固定默写纵切 | 三类材料共用自然排序和学生归组，识别方式独立；默写已分开未写、无法辨认、识别失败、涂改终态和真实不匹配，并支持失败重试、追加式校正、人工补录/记分、严格批量终审与显式发布 |
| M2 傻瓜式固定卷预检 | 🟡 三材料固定版式 + 图片/TXT/PDF/Word/Excel 答案核对已贯通 | “上传批改”仍是唯一主入口；普通卷确认结构，答题卡确认一次空白模板，默写确认一次空白区域/评分策略，之后整批自动处理并只展示异常。答案全一致一次确认；冲突/缺题阻断；老师可沿用当前答案，或把客观题/固定填空冲突答案另存为新 K1/作业版本且不改写本批绑定 |
| 音频回放 | ✅ app-managed archive | 按 hash 归档；原文件改名、重启和数据库恢复后仍可回放，原路径只兜底 |
| ffmpeg 转码/时长探测 | ✅ 可选集成 | 装了 ffmpeg 则转 16k 单声道 wav + 探测时长，否则降级 |

当前已验证：module-exam **109 tests**、workspace **238 tests**、Tauri 应用 **47 tests**（另有隔离夹具 2 tests）、两套 Clippy `-D warnings`、Tauri check、前端 build 和 `git diff --check` 全通过；exam 迁移已到 `exam_0020`。新增回归证明：老师校正默写后仍须单独终审，发布前不产生正式学习证据；人工补录不会覆盖机器 OCR；严格批量只确认当前高置信度精确结果、重复请求幂等并保存排除原因；显式发布后证据精确引用 rubric point，且老师校正级别和不可变来源可追溯；简答答案候选只有在参考答案、评分点和总分结构完整时才能进入 ready，采用后会建立新答案/rubric/link/作业版本，当前批次和既有成绩证据不变；三材料黄金合同会拒绝仓库内真实学生材料、禁止合成集声明生产准确率，并分别统计危险批量放行与识别单元误接受；真实数据评估只有在批准闸门的身份、策略 hash、有效期、图像/建议/老师标注范围和治理条件全部匹配时才能运行，未来审批或导出删除演练不能倒签解锁先前日期。真实答题卡/默写/答案文件、真实 provider 凭据、阈值、GUI 和实际导出删除演练尚未验证；正式库未写入。

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
