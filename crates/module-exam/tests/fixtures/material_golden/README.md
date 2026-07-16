# 三材料黄金集合同夹具

本目录固定普通试卷、答题卡和默写三条识别主链的最小离线评估合同。

- 仓库只保存合成合同清单和 hash，不保存学生姓名、文件路径、原图或作答正文。
- `repository_synthetic` 只能验证路由、状态、危险误放过和覆盖场景，不能宣称真实照片生产准确率。
- 真实扫描/照片必须保存在仓库外的本机受限目录；完成去标识、隐私审查与保存期限登记后，才能把 `production_accuracy_claim_allowed` 设为 `true`。
- 真实清单还必须冻结 `pilot_gate_id` 和对应闸门 JSON 的策略 hash；评估时同时提供 `--gate`、`--rights-evidence` 与 `--as-of`。闸门过期、撤销、内容漂移或四类导出/删除 dry-run 回执缺失时，评估器拒绝运行。
- 三类材料共用报告结构，但分别统计：普通试卷的页面/题区，答题卡的格位/涂改，默写的行栏/评分点。

离线生成报告：

```bash
cargo run -p module-exam --example evaluate_material_golden -- \
  --manifest crates/module-exam/tests/fixtures/material_golden/ordinary_paper_manifest_v1.json \
  --predictions crates/module-exam/tests/fixtures/material_golden/ordinary_paper_predictions_v1.json
```

报告只做只读比较，不调用识别 provider，不写成绩、发布或学习证据。

闸门本身可先独立检查：

```bash
cargo run -p module-exam --example evaluate_pilot_data_gate -- \
  --manifest /受限目录/pilot_data_gate_v1.json \
  --rights-evidence /受限目录/pilot_data_rights_evidence_v1.json \
  --as-of 2026-07-15
```

## 受限试点数据权利演练

先把 `pilot_dataset_contract_template_v1.json` 复制到仓库外受限目录，改成内部不透明 ID、真实文件 hash/大小与相对路径。真实文件、学生映射和导出包都不得进入仓库。仓库内 `pilot_dataset_synthetic_contract_v1.json` 及 `assets/pilot-contract-asset` 只用于无学生数据的命令合同检查。

每种作用域和动作各跑一次 dry-run，并分别保存回执：

```bash
cargo run -p module-exam --example manage_pilot_data_rights -- \
  --dataset-root /受限目录/pilot-dataset \
  --manifest /受限目录/pilot-dataset/pilot_dataset.json \
  --receipt /受限目录/receipts/student-export.json \
  --scope student \
  --action export \
  --mode dry-run \
  --gate-id <opaque-gate-id> \
  --scope-ref-sha256 <student-ref-sha256> \
  --idempotency-key <unique-key> \
  --requested-at 2026-07-15T08:00:00Z
```

将 `--scope` 改为 `batch`、`--action` 改为 `export/delete`，取得恰好四类成功回执后生成证据包：

```bash
cargo run -p module-exam --example build_pilot_data_rights_evidence -- \
  --gate-id <opaque-gate-id> \
  --dataset-id <opaque-dataset-id> \
  --generated-at 2026-07-15T09:00:00Z \
  --output /受限目录/pilot_data_rights_evidence_v1.json \
  --receipt /受限目录/receipts/student-export.json \
  --receipt /受限目录/receipts/batch-export.json \
  --receipt /受限目录/receipts/student-delete.json \
  --receipt /受限目录/receipts/batch-delete.json
```

把命令输出的 `evidence_sha256` 和同一生成时间冻结到已批准闸门的 `rights.dry_run_evidence_sha256` / `rights.dry_run_verified_at`。执行导出还需 `--mode execute --output-root /独立导出目录`；执行删除还需 `--mode execute --confirm-delete`。删除先保存 `deletion_pending`，中断后只允许同一请求恢复；回执不含正文、路径或学生身份。

真实黄金集评估示例：

```bash
cargo run -p module-exam --example evaluate_material_golden -- \
  --manifest /受限目录/ordinary_real_manifest.json \
  --predictions /受限目录/ordinary_real_predictions.json \
  --gate /受限目录/pilot_data_gate_v1.json \
  --rights-evidence /受限目录/pilot_data_rights_evidence_v1.json \
  --as-of 2026-07-15
```

这里的导出/删除范围只覆盖仓库外 `PilotDatasetManifest` 声明的影子试点资产，不代表正式应用数据库、备份、录音和所有学生数据已经具备统一生产级数据权利流程。

## 三材料单会话影子评估

拿到三类 provider 预测后，不再逐类手工汇总。一个会话必须同时提供普通试卷、答题卡和默写，合成合同与真实材料不能混用。输出只含输入 hash、聚合计数和安全发现，且 `release_authorized` 永远为 `false`：

```bash
mkdir -p /tmp/jiaofu-shadow-pilot
cargo run -p module-exam --example run_shadow_pilot -- \
  --session-id shadow-contract-001 \
  --as-of 2026-07-16 \
  --started-at 2026-07-16T08:00:00Z \
  --predictions-generated-at 2026-07-16T08:05:00Z \
  --completed-at 2026-07-16T08:10:00Z \
  --provider-ref contract-provider \
  --model-ref contract-model \
  --provider-config-version contract-config-v1 \
  --ordinary-manifest crates/module-exam/tests/fixtures/material_golden/ordinary_paper_manifest_v1.json \
  --ordinary-predictions crates/module-exam/tests/fixtures/material_golden/ordinary_paper_predictions_v1.json \
  --answer-sheet-manifest crates/module-exam/tests/fixtures/material_golden/answer_sheet_manifest_v1.json \
  --answer-sheet-predictions crates/module-exam/tests/fixtures/material_golden/answer_sheet_predictions_v1.json \
  --dictation-manifest crates/module-exam/tests/fixtures/material_golden/dictation_manifest_v1.json \
  --dictation-predictions crates/module-exam/tests/fixtures/material_golden/dictation_predictions_v1.json \
  --output /tmp/jiaofu-shadow-pilot/result.json
```

真实材料在同一命令追加 `--gate` 与 `--rights-evidence`。三类清单必须全部引用同一当前有效闸门；缺一类、重复材料类型、混用合成/真实、闸门或数据权利证据漂移都会拒绝。会话把危险批量放行、错误识别值、误接受、缺失/额外输出和覆盖缺口列为安全发现；保守转人工只进入报告指标，不被伪装成安全通过率。
