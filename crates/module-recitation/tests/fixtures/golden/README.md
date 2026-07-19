# 背诵黄金集合成合同夹具

本目录只固定 M1.2-3 的离线评估合同，不包含真实学生录音、ASR 正文、学生姓名、
文件名或本机路径。

- `synthetic_contract_manifest_v1.json` 覆盖规格要求的 19 类历史背诵风险。
- `synthetic_contract_predictions_v1.json` 是与冻结标注完全一致的合同输出，用来验证
  runner、计数和报告 schema，不代表任何 ASR 或评分模型的真实准确率。
- 合成样本不得填写 `audio_artifact_sha256`，且
  `production_accuracy_claim_allowed` 必须为 `false`。
- 真实去标识音频只能留在仓库外受限目录，清单只登记 artifact hash、版本 hash、
  不透明标注者/授权引用；评估时还必须提供有效的独立授权清单。
- 报告的 `release_authorized` 永远为 `false`，黄金集结果不能替代老师终审、真机验收、
  异模型审查或发布批准。

离线运行合成合同：

```bash
cargo run -p module-recitation --example evaluate_recitation_golden -- \
  --manifest crates/module-recitation/tests/fixtures/golden/synthetic_contract_manifest_v1.json \
  --predictions crates/module-recitation/tests/fixtures/golden/synthetic_contract_predictions_v1.json
```

真实去标识音频还需追加：

```bash
--authorization /受限目录/recitation_golden_authorization_v1.json \
--as-of 2026-07-18
```

授权清单必须与 manifest 的 dataset/gate 身份一致、仍在有效期内，并且其完整 JSON
SHA-256 等于 manifest 冻结的 `pilot_gate_policy_sha256`。任何内容漂移、过期、撤销、
未披露云处理或未声明原始音频仅本机保存，runner 都会失败关闭。
