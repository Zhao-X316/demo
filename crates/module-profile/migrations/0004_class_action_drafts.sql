-- M6.1-4：老师确认的班级教学行动草稿。
-- 预览不落库；确认后冻结来源班级快照、节点、目标学生和内容版本。
-- 练习作业的正式建立另走显式 materialization，并与 M2 作业原子提交。

CREATE TABLE class_action_drafts (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  request_key TEXT NOT NULL UNIQUE,
  class_profile_snapshot_id INTEGER NOT NULL
    REFERENCES class_profile_snapshots(id) ON DELETE RESTRICT,
  class_profile_node_metric_id INTEGER NOT NULL
    REFERENCES class_profile_node_metrics(id) ON DELETE RESTRICT,
  class_id INTEGER NOT NULL REFERENCES classes(id) ON DELETE RESTRICT,
  action_kind TEXT NOT NULL CHECK(action_kind IN (
    'reteach','practice','recitation','temporary_group'
  )),
  title TEXT NOT NULL CHECK(length(trim(title)) > 0),
  rationale TEXT NOT NULL CHECK(length(trim(rationale)) > 0),
  estimated_minutes INTEGER NOT NULL CHECK(estimated_minutes BETWEEN 1 AND 240),
  destination_module TEXT NOT NULL CHECK(destination_module IN (
    'class_dashboard','exam','recitation'
  )),
  destination_view TEXT NOT NULL,
  target_count INTEGER NOT NULL CHECK(target_count > 0),
  candidate_count INTEGER NOT NULL CHECK(candidate_count >= 0),
  snapshot_payload_sha256 TEXT NOT NULL CHECK(length(snapshot_payload_sha256)=64),
  payload_sha256 TEXT NOT NULL CHECK(length(payload_sha256)=64),
  state TEXT NOT NULL CHECK(state='teacher_confirmed'),
  confirmed_by TEXT NOT NULL,
  confirmed_at TEXT NOT NULL
);
CREATE INDEX idx_class_action_drafts_class
  ON class_action_drafts(class_id,confirmed_at DESC,id DESC);

CREATE TABLE class_action_draft_targets (
  draft_id INTEGER NOT NULL REFERENCES class_action_drafts(id) ON DELETE RESTRICT,
  student_id INTEGER NOT NULL REFERENCES students(id) ON DELETE RESTRICT,
  source_cell_status TEXT NOT NULL CHECK(source_cell_status IN (
    'needs_support','developing','stable'
  )),
  PRIMARY KEY(draft_id,student_id)
);

CREATE TABLE class_action_draft_candidates (
  draft_id INTEGER NOT NULL REFERENCES class_action_drafts(id) ON DELETE RESTRICT,
  order_index INTEGER NOT NULL CHECK(order_index >= 0),
  candidate_type TEXT NOT NULL CHECK(candidate_type='question_version'),
  question_version_public_id TEXT NOT NULL
    REFERENCES k1_question_versions(public_id) ON DELETE RESTRICT,
  answer_key_version_public_id TEXT NOT NULL
    REFERENCES k1_answer_key_versions(public_id) ON DELETE RESTRICT,
  rubric_version_public_id TEXT NOT NULL
    REFERENCES k1_rubric_versions(public_id) ON DELETE RESTRICT,
  link_set_public_id TEXT NOT NULL
    REFERENCES k1_link_sets(public_id) ON DELETE RESTRICT,
  title TEXT NOT NULL,
  question_type TEXT NOT NULL,
  max_score REAL NOT NULL CHECK(max_score > 0),
  active_assignment_count INTEGER NOT NULL CHECK(active_assignment_count >= 0),
  PRIMARY KEY(draft_id,question_version_public_id)
);

CREATE TABLE class_action_materializations (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  request_key TEXT NOT NULL UNIQUE,
  draft_id INTEGER NOT NULL UNIQUE REFERENCES class_action_drafts(id) ON DELETE RESTRICT,
  destination_type TEXT NOT NULL CHECK(destination_type='exam_assessment'),
  destination_public_id TEXT NOT NULL,
  destination_version_public_id TEXT NOT NULL,
  payload_sha256 TEXT NOT NULL CHECK(length(payload_sha256)=64),
  created_by TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TRIGGER class_action_draft_scope_guard
BEFORE INSERT ON class_action_drafts
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM class_profile_snapshots snapshot
    JOIN class_profile_node_metrics metric
      ON metric.id=NEW.class_profile_node_metric_id
     AND metric.snapshot_id=snapshot.id
    WHERE snapshot.id=NEW.class_profile_snapshot_id
      AND snapshot.class_id=NEW.class_id
      AND snapshot.state='teacher_confirmed'
  ) THEN RAISE(ABORT, 'class action draft source scope mismatch') END;
END;

CREATE TRIGGER class_action_target_scope_guard
BEFORE INSERT ON class_action_draft_targets
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM class_action_drafts draft
    JOIN class_profile_student_cells cell
      ON cell.snapshot_id=draft.class_profile_snapshot_id
     AND cell.node_metric_id=draft.class_profile_node_metric_id
     AND cell.student_id=NEW.student_id
     AND cell.status=NEW.source_cell_status
    JOIN students student
      ON student.id=NEW.student_id
     AND student.class_id=draft.class_id
     AND student.enabled=1
    WHERE draft.id=NEW.draft_id
  ) THEN RAISE(ABORT, 'class action target scope mismatch') END;
END;

CREATE TRIGGER class_action_candidate_scope_guard
BEFORE INSERT ON class_action_draft_candidates
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM class_action_drafts draft
    JOIN class_profile_node_metrics metric
      ON metric.id=draft.class_profile_node_metric_id
    JOIN k1_question_versions question
      ON question.public_id=NEW.question_version_public_id
     AND question.state='published'
     AND question.quality_level IN ('L2','L3','L4')
    JOIN k1_answer_key_versions answer
      ON answer.public_id=NEW.answer_key_version_public_id
     AND answer.question_version_id=question.id
     AND answer.state='confirmed'
    JOIN k1_rubric_versions rubric
      ON rubric.public_id=NEW.rubric_version_public_id
     AND rubric.question_version_id=question.id
     AND rubric.state='confirmed'
    JOIN k1_link_sets links
      ON links.public_id=NEW.link_set_public_id
     AND links.question_version_id=question.id
     AND links.state='confirmed'
    WHERE draft.id=NEW.draft_id
      AND draft.action_kind='practice'
      AND (
        (
          metric.target_type='knowledge_node'
          AND EXISTS (
            SELECT 1
            FROM k1_knowledge_links knowledge_link
            JOIN k1_knowledge_nodes knowledge
              ON knowledge.id=knowledge_link.knowledge_node_id
             AND knowledge.public_id=metric.target_public_id
            WHERE knowledge_link.link_set_id=links.id
              AND knowledge_link.confirmation_level='teacher_confirmed'
              AND knowledge_link.relation_type IN ('direct_assessment','rubric_basis')
          )
        )
        OR
        (
          metric.target_type='ability_dimension'
          AND EXISTS (
            SELECT 1
            FROM k1_ability_links ability_link
            JOIN k1_ability_dimensions ability
              ON ability.id=ability_link.ability_dimension_id
             AND ability.public_id=metric.target_public_id
            WHERE ability_link.link_set_id=links.id
              AND ability_link.confirmation_level='teacher_confirmed'
              AND ability_link.evidence_strength>=0.5
          )
        )
      )
  ) THEN RAISE(ABORT, 'class action candidate version mismatch') END;
END;

CREATE TRIGGER class_action_materialization_scope_guard
BEFORE INSERT ON class_action_materializations
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1 FROM class_action_drafts
    WHERE id=NEW.draft_id AND action_kind='practice' AND state='teacher_confirmed'
  ) THEN RAISE(ABORT, 'only confirmed practice drafts can be materialized') END;
END;

CREATE TRIGGER class_action_drafts_no_update
BEFORE UPDATE ON class_action_drafts
BEGIN SELECT RAISE(ABORT, 'class action drafts are immutable'); END;
CREATE TRIGGER class_action_drafts_no_delete
BEFORE DELETE ON class_action_drafts
BEGIN SELECT RAISE(ABORT, 'class action drafts are retained'); END;
CREATE TRIGGER class_action_draft_targets_no_update
BEFORE UPDATE ON class_action_draft_targets
BEGIN SELECT RAISE(ABORT, 'class action targets are immutable'); END;
CREATE TRIGGER class_action_draft_targets_no_delete
BEFORE DELETE ON class_action_draft_targets
BEGIN SELECT RAISE(ABORT, 'class action targets are retained'); END;
CREATE TRIGGER class_action_draft_candidates_no_update
BEFORE UPDATE ON class_action_draft_candidates
BEGIN SELECT RAISE(ABORT, 'class action candidates are immutable'); END;
CREATE TRIGGER class_action_draft_candidates_no_delete
BEFORE DELETE ON class_action_draft_candidates
BEGIN SELECT RAISE(ABORT, 'class action candidates are retained'); END;
CREATE TRIGGER class_action_materializations_no_update
BEFORE UPDATE ON class_action_materializations
BEGIN SELECT RAISE(ABORT, 'class action materializations are immutable'); END;
CREATE TRIGGER class_action_materializations_no_delete
BEFORE DELETE ON class_action_materializations
BEGIN SELECT RAISE(ABORT, 'class action materializations are retained'); END;
