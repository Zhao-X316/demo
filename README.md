# 教辅系统（本地版）

macOS 本地教辅平台。模块化单体：共享内核 `core` + 业务模块（M1 背诵批改、M2 题目批改…）。

> 设计文档：
> - 平台架构：[`教辅系统_平台总体架构_v1.md`](./教辅系统_平台总体架构_v1.md)
> - M1 背诵批改详规：[`背诵批改系统_技术规格与实施方案_v1.md`](./背诵批改系统_技术规格与实施方案_v1.md)

## 当前进度

> 2026-07-16：独立分支 `codex/t2-artifacts` 已完成 T2～T6、B3a0、B3a1 普通卷纵切、B3a2 固定答题卡首建/复用与多页模板集、M2-C0 固定格式默写正式终审，以及答案图片/TXT/PDF/DOCX/XLSX 结构化、冲突预检和“采用上传答案与评分点，另存新版本”。答题卡现在要求每个实际页码的模板全部确认且题目逐项覆盖后才允许处理；客观格进入本地 OMR，填空/简答主观区先形成答案隔离、追加式手写 OCR 转写，再进入独立主观题工作台。填空题只按老师已确认答案做确定性精确匹配；简答题按确认 rubric 逐点输出覆盖、部分覆盖、遗漏、矛盾或不确定，得分建议必须引用学生答案原文。老师接受机器建议或逐槽/逐评分点人工修正后形成 grade decision，逐项人工修正必须完整覆盖当前答案槽位或评分点并由系统自动汇总总分；整题总分兼容入口不伪造逐项证据。成绩仍须整份显式发布。老师可把人工确认的填空写法沉淀为未来答案版本，并按填空槽位/简答评分点确认未来知识与能力链接；当前成绩、发布和既有证据不被重绑。默写继续走独立 policy/rubric 语义。普通试卷、答题卡和默写共用按文件自然顺序、页面重复周期、起始学号与缺交名单的归组外壳，但使用独立识别算法。三类材料已具备合成黄金集、hash-only 单会话报告、老师人工基线/AI 辅助复核耗时合同，以及把真实数据闸门、数据权利证据、机器影子结果和老师报告绑定到同一会话的 hash-only 总验收包；真实脱敏材料/provider 阈值与真实老师影子试点仍未完成，合成结果不得宣称生产准确率或减负比例。未 tag、合并、推送或发布。

> 2026-07-19：M1.1 已完成结构化 rubric/评分点、ASR 与评分 run、老师逐点评审、评分点首用、疑点区间跳播、风险快捷终审、历史证据回看及隔离 `.app` G01～G05/重启核验。M1.2-1（`9f1f530`）已把老师确认的总体、流畅度和逐点评审投影为共享 `learning_evidence`；M1.2-2（`9f4b4d8`）新增不可变终审日期上下文和跨日期 retention 窗口；M1.2-3（`05779fa`）新增严格脱敏 manifest、真实数据授权 hash/有效期门禁、19 类历史背诵风险合同和固定 JSON runner；M1.2-4（`4222dd6`）新增无正文的逐条人工/AI 成对观察、每日积压快照、P50/P80/P95 与风险分账固定报告；M1.2-5（`f8286cd`）新增设置页一键默认脱敏诊断包，内容证据必须“逐项选择 → 精确预览 → 二次确认”；M1.2-6（`1754455`）为 M6 增加显式证据适配白名单：只有 active 且由老师接受/修正的背诵评分点可投知识节点，绝不投高阶能力；总体、流畅度和保持只进入内容级历史，并随不可变快照冻结。M6-3（`9f3d134`）再把 M3 当前错题、订正与巩固事实冻结进个人快照解释层，接入低权重订正证据、完全同口径的个人纵向趋势和追加式老师补充判断；错题恢复和老师说明都不能反写成绩、证据或系统掌握值。机器未确认、未知契约、未映射和非 active 证据全部 fail closed。合成合同只能验证 schema/计算/门禁，不得宣称真实准确率或减负；至少 50 对获批真实独立样本和两种顺序均覆盖后才允许描述实测耗时改善，所有报告仍永不授权发布。真实 provider、学生纵向录音和老师成对耗时仍未验证；M6-3 只完成数据库/服务自动化与模拟 Tauri 浏览器桌面/窄屏冒烟，尚未做独立 `.app` GUI、正式库副本迁移或真实课堂验收。未 tag、合并、推送或发布。

| 部分 | 状态 | 说明 |
|------|------|------|
| `crates/core` 共享内核 | ✅ 实现 + 单测 | 错误/实体/能力抽象(ports)/纯算法(归一化·拼音容错·相似度·正确率门控·间隔重复·hash) |
| `crates/core` DB 层 | ✅ T2 共享契约 | 迁移框架、通用仓储、`artifacts/ai_runs/background_jobs/learning_evidence/outbox/audit`；任务、提交、判定、复习卡与 effect 账本支持事务/改判/漂移保护 |
| `crates/core` 复习引擎 | ✅ 实现 + 单测 | `services/review`：pass/lapse 分账；补做通过从 stage 1 重启且保留 lapse |
| `crates/module-recitation` M1 域 | ✅ M1.1 + M1.2-6 | 文件名解析、结构化 rubric/评分点、不可变 transcript/AI run、熟练度与评分编排、老师逐点评审、共享总体/逐点/保持学习证据、脱敏黄金集、老师成对试点指标合同、默认脱敏诊断出口和 M6 显式只读适配 |
| `crates/module-recitation` M1 服务 | ✅ 实现 + 单测 | import 去重/归档、ASR 可恢复状态机、机器建议、老师总体/逐点终审、补背/到期复习/日切、跨日期保持窗口、同日去重、双向改判，以及 effect/review/evidence/retention 同事务回滚 |
| `crates/module-knowledge` K1 | 🟡 T3 兼容底座 | 教材/知识/考点/能力稳定版本，题目/答案/rubric/link 不可变版本，C0～L4 质量闸门与旧表显式映射；导入、搜索和 UI 尚未接入 |
| `crates/module-exam` M2 | 🟡 A1 + B0/B1 + M2.5 + B2 + B3a1/B3a2/B3a5/B3c3 + M2-C0 | 普通卷、固定答题卡多页模板集/客观主观分流、答题卡主观区追加式 OCR、填空确定性建议、简答逐评分点建议与老师主观题工作台、逐槽/逐评分点人工终审账本、固定默写正式链、五类答案版本链、评分点结构变化映射、未来答案/知识/能力版本均已落；三材料黄金合同、真实闸门、受限数据权利演练、hash-only 影子会话、老师耗时观察合同和统一试点证据总包已落。真实脱敏样本/provider 阈值和真实老师影子试点仍缺 |
| `crates/module-profile` M6 | 🟡 M6-1～3/M6.1 安全子集 | 个人/班级不可变快照、热力图、完全同口径个人/班级趋势、课堂事件、老师确认行动和脱敏摘要；M1/M2 正式证据显式适配，M3 错题恢复事实冻结，追加式老师补充判断与系统结论并列且不反写 |
| Tauri 应用外壳 `src-tauri` | ✅ 实现 + 独立检查 | DB+迁移、M1 终审，以及 T6 工作台/接受建议/人工记分/严格批量/显式发布、T6.1b 固定卷上传归档/PDF 真拆页、Ark 题区识别 run 和答题卡主观区答案隔离手写 OCR；在线备份恢复、按 hash 归档、凭据掩码、最小 asset scope 和默认脱敏诊断包 |
| 前端 `src` | 🟡 M1/M2 已接入 | 方向 B 模块切换；M1 六区、M2-A1 手工兜底，以及“上传批改”单入口、T6 标准卷按题终审、异常处理、严格批量和整卷发布页面 |
| 跨平台 CI | ✅ 后端三平台 + 应用 Win/Mac 编译 | `.github/workflows/ci.yml` |
| 打包 CI（安装包） | ✅ 工作流就绪 | `release.yml`：手动/tag 触发出 `.msi/.exe/.dmg` |
| 火山 ASR 真接口 | ✅ 标准版 submit+query | 境内端点绕系统代理；processing/ok/failed 可恢复，失败可重试、同 hash 重定位或作废 |
| 页面与老师终审 | ✅ A6b 真机通过 | 机器只给建议；老师核对录音/ASR/答案版本/评分/备注后终审，副作用才生效 |
| M2 页面证据链 / OCR | 🟡 三材料固定版式纵切已接 | 普通卷按 `structure run → teacher confirmation → alignment/region/crop → observation`；答题卡按 `逐页 blank image → 完整 template set → four-anchor alignment → 客观 OMR / 主观 crop → answer-free OCR → suggestion → teacher review`；默写按 `blank image → template/policy confirmation → aligned derivative/region crop → answer-free OCR → exception review`。简答题分项建议已有答案原文证据门禁；真实照片/provider 准确率仍未验证 |
| M2.5 题目自动沉淀 | 🟡 T5 核心合同完成 | 精确复用当前作业已固定的 K1 版本；否则创建老师私有 C0 候选；冲突/歧义进入待确认，学生卷只允许脱敏文本；相似题语义检索、候选整理 UI 和晋级尚未接入 |
| M2 拍照批改 | 🟡 普通卷/固定答题卡/固定默写纵切 | 三类材料共用“学号升序拍摄、文件自然排序、重复页面周期、起始学生/缺交一次确认”的学生归组，缺页不得平移后续学生；普通卷、答题卡和默写识别方式独立。默写已分开未写、无法辨认、识别失败、涂改终态和真实不匹配，并支持失败重试、追加式校正、人工补录/记分、严格批量终审与显式发布 |
| M2 傻瓜式固定卷预检 | 🟡 三材料固定版式 + 图片/TXT/PDF/Word/Excel 答案核对已贯通 | “上传批改”仍是唯一主入口；普通卷确认结构，答题卡确认一次空白模板，默写确认一次空白区域/评分策略，之后整批自动处理并只展示异常。答案全一致一次确认；冲突/缺题阻断；老师可沿用当前答案，或把客观题/固定填空冲突答案另存为新 K1/作业版本且不改写本批绑定 |
| 音频回放 | ✅ app-managed archive | 按 hash 归档；原文件改名、重启和数据库恢复后仍可回放，原路径只兜底 |
| ffmpeg 转码/时长探测 | ✅ 可选集成 | 装了 ffmpeg 则转 16k 单声道 wav + 探测时长，否则降级 |

当前已验证：module-exam **182 tests**、module-knowledge **6 tests**、module-profile **32 tests**、module-recitation **101 tests** + e2e **1 test**、module-wrongbook **22 tests**、suite-core **74 tests**，workspace 合计 **418 tests**；Tauri 应用 **60 tests**（含 9 项诊断包专项，另有客观/主观/M1 独立夹具 2/3/2 项），两套 Clippy `-D warnings`、Tauri check、前端 build、M6-3 模拟 Tauri 浏览器桌面/760px 窄屏冒烟、定向 Rust 格式化和 `git diff --check` 全通过。背诵合成黄金合同覆盖 19 类场景、13 个逐点状态、11 个疑点时间段和 7 个需完整回听样本；合成试点合同固定 4 对逐条观察、两种顺序、每日积压、ASR 调用/缓存和风险分账，固定报告 hash 为 `ce716328...80dfb`。设置页真实导出的默认诊断 ZIP 为 Unix `0600`，只含 manifest、诊断 JSON 和说明文件；扫描未发现姓名、绝对路径、音频、ASR、答案正文或凭据。两类试点报告仍 `release_authorized=false`，合成结果也分别禁止生产准确率或真实减负声明。此前 M1.2 隔离 `.app` 与旧已终审验收库副本在 54 条迁移时完成完整性/FK 与 16 条正式 evidence/outbox/audit 回读；第 55、56 个 profile 迁移只随全量自动化和 Tauri 启动路径执行，尚未做本批独立 `.app` GUI 或正式库副本迁移验收。真实答题卡/默写/答案文件、真实 provider、跨日期学生纵向录音、真实黄金音频、阈值、真实老师并行耗时、真实敏感内容诊断演练和真实课堂 M6-3 使用仍未验证，正式库未写入。

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

# macOS 安装盘：显式跳过易超时的 Finder 美化，并在挂载后验证
# bundle id / 版本 / 可执行文件 / Applications 链接 / 敏感文件边界
npm run bundle:macos

# 快速调试构建，或单独复核既有 DMG
npm run bundle:macos:debug
npm run verify:macos:dmg -- src-tauri/target/debug/bundle/dmg/JiaofuSuite_0.1.0_aarch64.dmg

# T6 独立验收包（独立 bundle id，不读取正式应用目录）
npm run acceptance:t6:build
cargo test --manifest-path src-tauri/Cargo.toml --example t6_objective_fixture

# 答题卡主观题独立验收包（填空 + 简答；独立 bundle id）
npm run acceptance:subjective:build
cargo test --manifest-path src-tauri/Cargo.toml --example t6_subjective_fixture

# M1.1 结构化背诵独立验收包（真实可播放 WAV + 风险队列；独立 bundle id）
npm run acceptance:recitation:build
cargo test --manifest-path src-tauri/Cargo.toml --example m1_structured_fixture

# M1.2 背诵合成黄金集只读评估（不调用 provider、不写成绩或学习证据）
cargo run -p module-recitation --example evaluate_recitation_golden -- \
  --manifest crates/module-recitation/tests/fixtures/golden/synthetic_contract_manifest_v1.json \
  --predictions crates/module-recitation/tests/fixtures/golden/synthetic_contract_predictions_v1.json

# M1.2 背诵人工/AI 成对试点指标合成合同（只写指定本机报告，不读业务库）
mkdir -p /tmp/jiaofu-recitation-pilot
cargo run -p module-recitation --example evaluate_recitation_pilot -- \
  --observations crates/module-recitation/tests/fixtures/pilot_metrics/synthetic_contract_observations_v1.json \
  --generated-at 2026-07-18T09:00:00Z \
  --evaluated-on 2026-07-18 \
  --output /tmp/jiaofu-recitation-pilot/report.json
```

当前主观题夹具 schema v2 覆盖 2 名学生 × 3 题（单槽填空、多槽填空、双评分点简答），
可由服务级生命周期测试验证逐项终审、成绩发布和学习证据落账。真 `.app` 验收仍需在隔离
`HOME` 下完成界面操作与重启回读，不能用上述命令通过替代 GUI 验收。

M1.1 夹具只允许写入 `jiaofu-recitation-fixture-*` 隔离根下、末级为
`com.jiaofu.suite.recitationfixture` 的全新目录。它准备“事实矛盾 → 评分点遗漏 →
机器通过”三条待终审记录，以及一篇尚未设置评分点的内容；录音是归档目录中的有效
16 kHz 单声道 WAV。夹具测试和 `seeded` 校验只证明数据、音频与状态合同成立，仍须
在隔离 `HOME` 下用真 `.app` 回归评分点设置、区间跳播、逐点评审、快捷键、自动下一条
和重启持久化。

## 安全红线

- 云凭据（火山 App ID / Access Token）只存本机 `secrets.json`（0600），**绝不入库、不打进包**。
- `secrets.json`、`*.db` 已在 `.gitignore`。

## 加新模块（M2/M3…）

见平台架构 §8：新建 `crates/module-xxx` 依赖 `suite-core` → 实现 `Module` 契约 + 模块专属表 → 在 `src-tauri` 注册 → 前端 `moduleRegistry` 加一项。core 与其他模块零改动。
