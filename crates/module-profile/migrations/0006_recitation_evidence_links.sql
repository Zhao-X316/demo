-- M1.2-6：M6 冻结背诵内容级证据引用。
--
-- 逐知识点证据仍写入 profile_evidence_links；总体结论、流畅度和跨日保持度
-- 只作为内容级历史引用，不得绑定知识节点或能力维度。

CREATE TABLE profile_recitation_evidence_links (
  snapshot_id INTEGER NOT NULL REFERENCES profile_snapshots(id) ON DELETE RESTRICT,
  learning_evidence_id INTEGER NOT NULL REFERENCES learning_evidence(id) ON DELETE RESTRICT,
  PRIMARY KEY(snapshot_id, learning_evidence_id)
);
CREATE INDEX idx_profile_recitation_evidence_links_evidence
  ON profile_recitation_evidence_links(learning_evidence_id, snapshot_id);

CREATE TRIGGER profile_recitation_evidence_links_no_update
BEFORE UPDATE ON profile_recitation_evidence_links
BEGIN SELECT RAISE(ABORT, 'profile recitation evidence links are immutable'); END;
CREATE TRIGGER profile_recitation_evidence_links_no_delete
BEFORE DELETE ON profile_recitation_evidence_links
BEGIN SELECT RAISE(ABORT, 'profile recitation evidence links are retained'); END;
