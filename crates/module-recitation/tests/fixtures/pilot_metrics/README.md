# 背诵试点指标合成合同

本目录只保存无学生身份、无录音、无转写、无答案正文和无本机路径的合成命令合同。
它用于固定 M1.2-4 的成对观察、每日积压和指标计算，不是实际课堂试点，也不能证明
ASR/评分准确率或老师真实节省时间。

运行：

```bash
mkdir -p /tmp/jiaofu-recitation-pilot
cargo run -p module-recitation --example evaluate_recitation_pilot -- \
  --observations crates/module-recitation/tests/fixtures/pilot_metrics/synthetic_contract_observations_v1.json \
  --generated-at 2026-07-18T09:00:00Z \
  --evaluated-on 2026-07-18 \
  --output /tmp/jiaofu-recitation-pilot/report.json
```

固定合成结果包括：

- 4 组同老师、同去标识样本的纯人工/AI 辅助配对；
- 人工与辅助每 50 条主动耗时分别为 1250 秒和 625 秒；
- 人工 P50/P80/P95 为 20/40/40 秒，辅助为 10/20/20 秒；
- AI 辅助完整回听 2 条、只听疑点 1 条、未播放 1 条；
- 机器 pass/老师 fail 1 条，机器 fail/老师 pass 1 条；
- 8 个已评审评分点中修正 2 个；
- 初始 ASR 失败 2 条，其中恢复 1 条、未恢复 1 条；
- 实际 ASR 调用 3 条、缓存命中 1 条，初始失败率为 2/3（6666 bp）；
- 关联异常 1 条，一次完成 3 条；
- 一组人工/辅助每日积压快照；
- `minimum_pair_count_met=false`、`time_saving_claim_allowed=false`、
  `release_authorized=false`。
- 固定报告 hash：
  `ce7163281aa3397678c1f277b6787d772b4b6559ea01bc63bfbd1981ac780dfb`。

真实观察必须放在仓库外受限目录，并额外提供用途为
`recitation_pilot_metrics` 的有效授权 JSON。每条人工/辅助观察必须绑定同一老师 hash、
同一 case hash 和同一顺序分组；同一 case hash 不能重复扩充样本量。只有获批真实数据、
至少 50 对独立样本、人工优先和辅助优先两种顺序均有覆盖，且实测辅助主动耗时更低时，
报告才会把 `time_saving_claim_allowed` 设为 `true`。即使如此，
`release_authorized` 仍固定为 `false`，发布仍需独立闸门。
