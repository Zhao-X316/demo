---
title: jiaofu-suite R3-W1-V1 review workbench closeout audit
date: 2026-08-02
status: authorized_read_only_closeout
authorization_source: goal_2026-08-02_continuous_refactor
source_branch: codex/t2-artifacts
source_commit: 6072360049b09c3155726773039da139834294fe
source_index_sha256: b54ac3d57d26f1df76dd633cd53fdb00400ca9ed4be0167a597aab2e7f547d4b
source_porcelain_count: 166
scope: R3-W1-V1-review-workbench-closeout-audit
production_change_authorized: false
test_change_authorized: false
audit_change_authorized: false
shared_hook_change_authorized: false
integration_authorized: false
release_authorized: false
---

# R3-W1-V1 终审工作台 Controller 独立收口审计合同

## 1. 唯一目标

在不修改生产、测试、审计器、API、依赖或既有合同的前提下，从零复跑并确认：

1. Objective、Dictation、Subjective 三个 Tab 仍各自恰好组合一个独立 controller；
2. 三个 Tab 不直接持有 provider、`useState/useEffect`、UUID 或确认框；provider 调用点全部由对应 controller 独占；
3. controller 与 `SubjectiveComponentEditor` 只暴露语义动作，不暴露 React raw setter；
4. 20 个 provider 命令、22 个调用点、参数/顺序/幂等键、阈值、老师终审与发布边界无漂移；
5. W1 27/27、C2 10/10、C1 19/19、H1 17/17/26/26、65/65、build、Python、whitespace 与 27/27 UI 可复现；
6. 冻结哈希、HEAD、Git index 与 staged 不变。

## 2. 冻结边界

| 范围 | SHA-256 |
|---|---|
| Objective Tab / controller | `61b7e555...4def` / `32e002ea...c3a1` |
| Dictation Tab / controller | `36ed7f3c...5d64` / `876f2582...70bc` |
| Subjective Tab / controller / editor | `f9a3d296...1f84` / `c50c1ae...bb71` / `f64e8a08...9f54e` |
| Subjective toolbar / publish panel | `9bcba435...a44e` / `cc34532d...c47` |
| W1/C2/C1/H1 审计器 | `f454b7b5...c4a3` / `a7764fe7...a9993` / `b7f2d20c...a389` / `3e9d6496...b2e7` |
| `Exam.tsx` / Exam API | `ed4132ca...e9bc` / `1a796762...c164` |
| package / lock | `a3f0afa7...69b0` / `8f627a00...adf4` |
| 四个直接测试 | `8d74ebd3...5b21` / `8f25a0df...6790` / `9942359c...7da` / `71a750d0...b1fb` |
| 27 个 UI 脚本哈希清单 | `12c9e0ba...baf5` |

## 3. 共享 hook 复核结论

三个 controller 只有作业/题目 options 协调和简单 keyed draft 写入形式相似；它们的行模型、草稿 key、校验、provider 命令、严格批量条件、OCR/OMR 续接、答案/rubric 晋级和发布流程不同。当前重复不足以形成稳定的共同状态/事件合同，抽取共享 hook 会增加泛型和跨工作台耦合。因此 V1 应记录“无需共享 hook”，不得为消除少量重复修改生产。

## 4. 允许范围

仓库内只允许新增本合同。审计过程只读；仓库外可新增 V1 回执并更新权威状态、索引和修改记录。若任一冻结哈希或门禁失败，必须保持 W1 未关闭并另立修复合同，不得在 V1 中修改生产或测试。

## 5. 通过与边界

通过要求所有门禁退出 0，七个目标生产文件、保护组件、测试、审计器、API、依赖、HEAD/index/staged 不变；展开 porcelain 只能 `166→167`。通过只代表 R3-W1 本地自动化收口，不等于 R3 阶段整体收口，也不证明真实 OCR/AI/provider、老师减负、独立 `.app`、R4～R5、集成或发布。
