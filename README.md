# 教辅系统（本地版）

macOS 本地教辅平台。模块化单体：共享内核 `core` + 业务模块（M1 背诵批改、M2 题目批改…）。

> 设计文档：
> - 平台架构：[`教辅系统_平台总体架构_v1.md`](./教辅系统_平台总体架构_v1.md)
> - M1 背诵批改详规：[`背诵批改系统_技术规格与实施方案_v1.md`](./背诵批改系统_技术规格与实施方案_v1.md)

## 当前进度

> 2026-07-16：独立分支 `codex/t2-artifacts` 已完成 T2～T6、B3a0、B3a1 普通卷纵切、B3a2 固定答题卡首建/复用与多页模板集、M2-C0 固定格式默写正式终审，以及答案图片/TXT/PDF/DOCX/XLSX 结构化、冲突预检和“采用上传答案与评分点，另存新版本”。答题卡现在要求每个实际页码的模板全部确认且题目逐项覆盖后才允许处理；客观格进入本地 OMR，填空/简答主观区先形成答案隔离、追加式手写 OCR 转写，再进入独立主观题工作台。填空题只按老师已确认答案做确定性精确匹配；简答题按确认 rubric 逐点输出覆盖、部分覆盖、遗漏、矛盾或不确定，得分建议必须引用学生答案原文。老师接受机器建议或逐槽/逐评分点人工修正后形成 grade decision，逐项人工修正必须完整覆盖当前答案槽位或评分点并由系统自动汇总总分；整题总分兼容入口不伪造逐项证据。成绩仍须整份显式发布。老师可把人工确认的填空写法沉淀为未来答案版本，并按填空槽位/简答评分点确认未来知识与能力链接；当前成绩、发布和既有证据不被重绑。默写继续走独立 policy/rubric 语义。普通试卷、答题卡和默写共用按文件自然顺序、页面重复周期、起始学号与缺交名单的归组外壳，但使用独立识别算法。三类材料已具备合成黄金集、hash-only 单会话报告、老师人工基线/AI 辅助复核耗时合同，以及把真实数据闸门、数据权利证据、机器影子结果和老师报告绑定到同一会话的 hash-only 总验收包；真实脱敏材料/provider 阈值与真实老师影子试点仍未完成，合成结果不得宣称生产准确率或减负比例。未 tag、合并、推送或发布。

> 2026-07-19：M1.1 已完成结构化 rubric/评分点、ASR 与评分 run、老师逐点评审、评分点首用、疑点区间跳播、风险快捷终审、历史证据回看及隔离 `.app` G01～G05/重启核验。M1.2-1（`9f1f530`）已把老师确认的总体、流畅度和逐点评审投影为共享 `learning_evidence`；M1.2-2（`9f4b4d8`）新增不可变终审日期上下文和跨日期 retention 窗口；M1.2-3（`05779fa`）新增严格脱敏 manifest、真实数据授权 hash/有效期门禁、19 类历史背诵风险合同和固定 JSON runner；M1.2-4（`4222dd6`）新增无正文的逐条人工/AI 成对观察、每日积压快照、P50/P80/P95 与风险分账固定报告；M1.2-5（`f8286cd`）新增设置页一键默认脱敏诊断包，内容证据必须“逐项选择 → 精确预览 → 二次确认”；M1.2-6（`1754455`）为 M6 增加显式证据适配白名单：只有 active 且由老师接受/修正的背诵评分点可投知识节点，绝不投高阶能力；总体、流畅度和保持只进入内容级历史，并随不可变快照冻结。M6-3（`9f3d134`）再把 M3 当前错题、订正与巩固事实冻结进个人快照解释层，接入低权重订正证据、完全同口径的个人纵向趋势和追加式老师补充判断；错题恢复和老师说明都不能反写成绩、证据或系统掌握值。M6-4 教材范围第一批（`3a784bf`）增加“按现有证据自动确定、整册知识地图、教材单元/课”三类选择，个人与班级只消费范围内证据并把 selector、水位和 payload 随快照冻结；范围外记录单列排除，不会变成薄弱。个人学习报告（`2f215d4`）只允许本机老师从当前未过期个人快照生成不可变、可打印的私有 HTML，冻结节点、背诵/错题恢复/同口径趋势与老师补充判断，拒绝覆盖且 Unix 权限为 `0600`；报告不含原始录音、图片、转写、学生答案或题干，也不会自动发送，家长授权和在线分享继续 fail closed。班级共性教学输入（`4517d42`）只读取当前最新、未过期且老师确认的班级快照，将同时达到样本门槛和共同支持比例门槛的知识/能力节点整理为可删减、可改写的一站式课堂重点；老师确认后冻结不可变输入并写 audit/outbox，但不自动布置作业、不修改成绩、学习证据、学生标签或背诵排程。机器未确认、未知契约、未映射和非 active 证据全部 fail closed。合成合同只能验证 schema/计算/门禁，不得宣称真实准确率或减负；至少 50 对获批真实独立样本和两种顺序均覆盖后才允许描述实测耗时改善。真实 provider、学生纵向录音、老师成对耗时和真实课堂报告/教学重点内容仍未验证；个人报告尚未做独立 `.app` GUI/打印，班级教学重点仅完成模拟 Tauri 浏览器交互。未 tag、合并、推送或发布。

> 2026-07-19：`9271717` 完成 M2.5 固定普通卷客观题来源纵切。普通卷结构 provider 可返回清洗后的印刷题，老师确认页面结构后再明确“收入我的题库”；系统每个作业页只选择一张安全来源页，逐题建立来源声明并复用既有 enqueue/worker/C0/待整理区。学生整页、答案 crop、姓名、作答、批注和得分不进入 K1；题库同步失败只显示待重试，不阻断本次批改。该提交不等于任意照片自动建题：独立空白卷/电子题目卷、填空/简答拆题、真实照片/provider 和独立 `.app` 仍未完成。

> 2026-07-19：`4110bf8` 完成 K1/M2.5 独立干净题面来源纵切。老师可从 JPG/PDF 空白卷或 TXT/DOCX/XLSX 电子题目文件一站式导入；原文件先归档，AI run 版本化拆题，成功结果只进入来源收件箱。老师可批量确认无异常题面、逐题修正低置信结果或丢弃；确认后只执行唯一完全一致复用或创建私有 K1 L0，不伪造答案、rubric、assessment、成绩或学习证据。学生身份、手写作答、教师批注或分数任一存在时 fail closed；失败 run 可回读。下一步是把答案图片/文件与 L0 题面匹配，经老师确认后提升 L1/L2。

> 2026-07-19：`6cc604b` 完成 K1 独立答案资料纵切。老师选择已确认题面来源后，可上传 JPG/PDF/TXT/DOCX/XLSX 答案资料；系统归档原件、运行版本化 AI 匹配，并为每道题显式生成正常、待核对或缺失草稿。正常项可批量确认，冲突/缺失由老师补录；确认事务创建 confirmed answer/rubric 和 L1/L2 质量事件。失败可重试，更新 run 会使旧草稿失效；任何 AI 草稿都不能自行成为答案。本批不创建 assessment、成绩、发布或学习证据。下一步是对 L2 题目复核知识/能力链接和证据强度，满足后再晋级 L3。

> 2026-07-19：`2d0e527` 完成 K1 L2→L3 知识/能力链接复核纵切。题库“导入题目”入口自动列出 L2 待关联题，AI 只能从所选 confirmed 教材地图和当前学科能力目录整理不可变草稿；老师逐整题、答案槽位或评分点增删链接，全部必需来源覆盖后才可确认。确认事务创建 teacher-confirmed link set、复核账本和 L3 质量事件；客观题要求整题、填空要求每空、简答要求每个评分点都具备对应知识关系和能力证据。本批不创建 assessment、成绩、发布或学习证据，也不改绑历史作业。

> 2026-07-19：`866d17e` 完成 K1-5 题目实际表现与版本影响计划纵切。题库新增“表现与版本影响”入口；表现只读取每份答卷当前有效发布快照中的老师确认得分，并分开满分/部分/零分、首次/订正和作业场景，不把单班结果写回预计难度。若作业仍绑定旧 answer/rubric/link，系统先展示受影响作业、未发布/已发布答卷、active evidence 和图谱快照引用；老师确认后只冻结“仅用于未来/计划重算未发布/创建已发布复核任务”的不可变计划与任务清单。preview hash 漂移时拒绝；本批不会换绑作业、改分、重发成绩、改写学习证据或覆盖旧图谱。

> 2026-07-19：`291cec0` 完成 M2.5-3a 批改中简答 rubric 更新建议。老师只有在查看原图、逐评分点人工给分并引用学生原文后，才能再次明确把该表述加入未来评分规则；系统追加 confirmed rubric、重映射后的 link set 和新的 assessment version，当前/历史成绩、发布和学习证据保持不变。同一来源重复点击幂等，版本采用关系由 `exam_0035` 不可变账本保护。填空题可接受答案继续复用既有二次确认入口。下一步只消费已冻结影响任务：未发布作答生成可恢复重评准备，已发布作答生成老师逐条复核 case，仍不允许静默改历史。

> 2026-07-19：`3b9a54b` 完成 M2.5-3b 版本影响待处理 case。系统把 `exam_0034` 已冻结任务安全转换为不可变逐答卷 case：未发布答卷冻结当时的 active 老师评分（允许明确为空），已发布答卷冻结当前有效 publication 实际包含的评分 revision，而不是后来尚未发布的 active revision；同时冻结目标 answer/rubric/link、来源快照 hash、学生/作答/题目实例范围。重复执行幂等，上游作答或 publication 漂移时整笔拒绝；本批仍不改 assessment 绑定、grade、publication、学习证据或图谱快照。下一步由老师在 case 中查看原始答题证据，显式创建新 grade revision；已发布答卷还必须再次明确发布新 revision。

> 2026-07-19：`0cf73fb` 完成 M2.5-3c 版本影响评分确认与再发布。case 直接展示目标答案、学生 OCR/原始裁剪和逐槽/逐评分点评分表；老师先“确认新评分（暂不发布）”，系统只追加 `teacher_corrected` grade revision 和不可变 resolution，旧正式 publication 与 active evidence 保持有效。只有老师再次“明确发布整份新成绩”，才在发布事务中失效旧正式证据并激活引用目标 answer/rubric/link 的新证据。主观题正分必须引用学生原文，组件集合/分数总和、owner、来源快照和当前 active grade 全部 fail closed；本批不静默换绑 assessment item，也不改写旧图谱快照。

> 2026-07-19：`31d9047` 完成 M2.5-3d 未来作业默认版本升级。老师确认版本影响计划后，可为仍是当前默认的作业生成下一版；系统复制全部题目，只把计划中的唯一题目切换到目标 answer/rubric/link，再追加不可变默认版本选择。以后上传入口优先选择显式默认版本；历史 assessment version、attempt、grade、publication、learning evidence 和旧图谱快照均不重绑。当前默认或计划范围漂移时整笔拒绝。

| 部分 | 状态 | 说明 |
|------|------|------|
| `crates/core` 共享内核 | ✅ 实现 + 单测 | 错误/实体/能力抽象(ports)/纯算法(归一化·拼音容错·相似度·正确率门控·间隔重复·hash) |
| `crates/core` DB 层 | ✅ T2 共享契约 | 迁移框架、通用仓储、`artifacts/ai_runs/background_jobs/learning_evidence/outbox/audit`；任务、提交、判定、复习卡与 effect 账本支持事务/改判/漂移保护 |
| `crates/core` 复习引擎 | ✅ 实现 + 单测 | `services/review`：pass/lapse 分账；补做通过从 stage 1 重启且保留 lapse |
| `crates/module-recitation` M1 域 | ✅ M1.1 + M1.2-6 | 文件名解析、结构化 rubric/评分点、不可变 transcript/AI run、熟练度与评分编排、老师逐点评审、共享总体/逐点/保持学习证据、脱敏黄金集、老师成对试点指标合同、默认脱敏诊断出口和 M6 显式只读适配 |
| `crates/module-recitation` M1 服务 | ✅ 实现 + 单测 | import 去重/归档、ASR 可恢复状态机、机器建议、老师总体/逐点终审、补背/到期复习/日切、跨日期保持窗口、同日去重、双向改判，以及 effect/review/evidence/retention 同事务回滚 |
| `crates/module-knowledge` K1 | 🟡 语义底座 + 找题查重 + 可解释组卷 + 题面/答案/链接/影响纵切 | 教材/知识/考点/能力稳定版本，题目/答案/rubric/link 不可变版本，C0～L4 质量闸门与旧表显式映射；当前个人/官方题目支持 FTS/结构化筛选、确定性疑似重复和老师追加式归类，题库组卷只从已确认 L3/L4 题目推荐并解释贡献；私有 C0 可由老师核对后追加 L1/confirmed answer；固定普通卷客观题印刷文本可在老师确认后进入候选；独立 JPG/PDF/TXT/DOCX/XLSX 题面可确认到精确复用或 L0，独立答案资料经老师确认后创建 answer/rubric 并晋级 L1/L2；L2 题目可由 AI 整理目录内链接草稿，老师逐整题/槽位/评分点确认后原子晋级 L3；当前有效发布表现和旧版本影响范围可查看、冻结计划、形成逐答卷 case，由老师确认新评分后明确再发布，并显式生成以后上传使用的新 assessment 默认版本。真正语义检索、题族正式编辑和完整共享 UI 尚未接入 |
| `crates/module-exam` M2 | 🟡 A1 + B0/B1 + M2.5 + B2 + B3a1/B3a2/B3a5/B3c3 + M2-C0 | 普通卷、固定答题卡多页模板集/客观主观分流、答题卡主观区追加式 OCR、填空确定性建议、简答逐评分点建议与老师主观题工作台、逐槽/逐评分点人工终审账本、固定默写正式链、五类答案版本链、评分点结构变化映射、未来答案/知识/能力版本均已落；三材料黄金合同、真实闸门、受限数据权利演练、hash-only 影子会话、老师耗时观察合同和统一试点证据总包已落。真实脱敏样本/provider 阈值和真实老师影子试点仍缺 |
| `crates/module-profile` M6 | 🟡 M6-1～4/M6.1 安全子集 | 个人/班级不可变快照、教材范围冻结、个人私有 HTML 报告、班级共性教学输入、热力图、完全同口径个人/班级趋势、课堂事件、老师确认行动和脱敏摘要；M1/M2 正式证据显式适配，M3 错题恢复事实冻结，追加式老师补充判断与系统结论并列且不反写 |
| Tauri 应用外壳 `src-tauri` | ✅ 实现 + 独立检查 | DB+迁移、M1 终审，以及 T6 工作台/接受建议/人工记分/严格批量/显式发布、T6.1b 固定卷上传归档/PDF 真拆页、Ark 题区识别 run 和答题卡主观区答案隔离手写 OCR；在线备份恢复、按 hash 归档、凭据掩码、最小 asset scope 和默认脱敏诊断包 |
| 前端 `src` | 🟡 M1/M2 已接入 | 方向 B 模块切换；M1 六区、M2-A1 手工兜底，以及“上传批改”单入口、T6 标准卷按题终审、异常处理、严格批量和整卷发布页面 |
| 跨平台 CI | ✅ 后端三平台 + 应用 Win/Mac 编译 | `.github/workflows/ci.yml` |
| 打包 CI（安装包） | ✅ 工作流就绪 | `release.yml`：手动/tag 触发出 `.msi/.exe/.dmg` |
| 火山 ASR 真接口 | ✅ 标准版 submit+query | 境内端点绕系统代理；processing/ok/failed 可恢复，失败可重试、同 hash 重定位或作废 |
| 页面与老师终审 | ✅ A6b 真机通过 | 机器只给建议；老师核对录音/ASR/答案版本/评分/备注后终审，副作用才生效 |
| M2 页面证据链 / OCR | 🟡 三材料固定版式纵切已接 | 普通卷按 `structure run → teacher confirmation → alignment/region/crop → observation`；答题卡按 `逐页 blank image → 完整 template set → four-anchor alignment → 客观 OMR / 主观 crop → answer-free OCR → suggestion → teacher review`；默写按 `blank image → template/policy confirmation → aligned derivative/region crop → answer-free OCR → exception review`。简答题分项建议已有答案原文证据门禁；真实照片/provider 准确率仍未验证 |
| M2.5 题目自动沉淀 | 🟡 T5 + 私有候选整理 + 固定普通卷/独立题面/答案/链接/版本影响闭环完成 | 精确复用当前作业已固定的 K1 版本；否则创建老师私有 C0 候选；老师可核对题干/选项/答案后晋级 L1 或丢弃，当前作业不自动换版。固定普通卷客观题印刷层在老师确认页面结构后可旁路入队；独立干净题面先到 L0，再由独立答案资料经老师确认后提升 L1/L2，最后逐来源确认知识/能力链接晋级 L3。学生图片/作答不进入 K1，题库失败不阻断批改；旧版本先生成影响计划和逐答卷 case，老师确认新 grade revision 后仍须明确再发布才切换正式 evidence；老师还可显式复制当前作业版本并将新版设为以后上传默认，历史作业不重绑。填空可接受写法和简答评分点原文均须老师二次明确确认才追加未来版本。相似题语义检索和共享仍未接入 |
| M2 拍照批改 | 🟡 普通卷/固定答题卡/固定默写纵切 | 三类材料共用“学号升序拍摄、文件自然排序、重复页面周期、起始学生/缺交一次确认”的学生归组，缺页不得平移后续学生；普通卷、答题卡和默写识别方式独立。默写已分开未写、无法辨认、识别失败、涂改终态和真实不匹配，并支持失败重试、追加式校正、人工补录/记分、严格批量终审与显式发布 |
| M2 傻瓜式固定卷预检 | 🟡 三材料固定版式 + 图片/TXT/PDF/Word/Excel 答案核对已贯通 | “上传批改”仍是唯一主入口；普通卷确认结构，答题卡确认一次空白模板，默写确认一次空白区域/评分策略，之后整批自动处理并只展示异常。答案全一致一次确认；冲突/缺题阻断；老师可沿用当前答案，或把客观题/固定填空冲突答案另存为新 K1/作业版本且不改写本批绑定 |
| 音频回放 | ✅ app-managed archive | 按 hash 归档；原文件改名、重启和数据库恢复后仍可回放，原路径只兜底 |
| ffmpeg 转码/时长探测 | ✅ 可选集成 | 装了 ffmpeg 则转 16k 单声道 wav + 探测时长，否则降级 |

当前已验证：module-exam **204 tests**、module-knowledge **29 tests**、module-profile **42 tests**、module-recitation **101 tests** + e2e **1 test**、module-wrongbook **22 tests**、suite-core **74 tests**，workspace 合计 **473 tests**；Tauri 应用 **69 tests**（另有客观/主观/M1 独立夹具 2/3/2 项）。workspace/K1/Tauri Clippy `-D warnings`、Tauri check、前端 build、模拟 Tauri 浏览器交互和 `git diff --check` 全通过。M2.5-3d 专项覆盖完整复制当前默认作业、只更新影响计划中的唯一题目、幂等、过期默认/计划范围拒绝、默认选择账本不可变，以及历史 attempt/grade/publication/evidence/profile 零变更；上传入口明确选中“默认 · 第 2 版”。正式库 Online Backup 副本从 22 条迁移到 **70 条**后完整性 `ok`、FK=0；`exam_0038` 的默认选择表、当前默认视图和 3 个保护/校验触发器存在且新业务行 0。正式源库 SHA-256 仍为 `be3c97e4...34ab0`，保持 22 条迁移且未写入。既有 K1 搜索/组卷/候选/题面/答案/链接、K1-5 影响计划、M2.5-3a～3c 评分与再发布、M1/M3/M6 合同继续由各自验收总览证明。真实答题卡/默写/答案文件、真实 provider、跨日期学生纵向录音、真实黄金音频、阈值、真实老师并行耗时、真实敏感内容诊断演练、独立 `.app` 找题/查重/组卷/候选整理/题面/答案/链接/表现/更新建议/case/评分/再发布/默认换版点击、真实题库规模、报告保存打印和真实课堂 M6-4 使用仍未验证。

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

# M2.5 未来作业默认版本升级独立验收包（真实 IPC + 历史不变式；独立 bundle id）
npm run acceptance:m25:build
cargo test --manifest-path src-tauri/Cargo.toml --example m25_default_fixture

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
