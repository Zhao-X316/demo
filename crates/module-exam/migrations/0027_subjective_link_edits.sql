-- M2-B3c6：答题卡填空槽位/简答评分点的老师确认链接另存新版本。
-- 当前作业、评分、发布与学习证据不原地重绑；新链接只供未来作业版本使用。

CREATE TABLE exam_subjective_link_edits_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  source_assessment_item_id INTEGER NOT NULL REFERENCES exam_assessment_items_v2(id),
  assessment_id INTEGER NOT NULL REFERENCES exam_assessments_v2(id),
  question_version_id INTEGER NOT NULL REFERENCES k1_question_versions(id),
  question_type TEXT NOT NULL CHECK(question_type IN ('fill_blank','short_answer')),
  base_assessment_version_id INTEGER NOT NULL REFERENCES exam_assessment_versions_v2(id),
  base_assessment_item_id INTEGER NOT NULL REFERENCES exam_assessment_items_v2(id),
  base_link_set_id INTEGER NOT NULL REFERENCES k1_link_sets(id),
  adopted_assessment_version_id INTEGER NOT NULL UNIQUE REFERENCES exam_assessment_versions_v2(id),
  adopted_assessment_item_id INTEGER NOT NULL UNIQUE REFERENCES exam_assessment_items_v2(id),
  adopted_link_set_id INTEGER NOT NULL UNIQUE REFERENCES k1_link_sets(id),
  input_hash TEXT NOT NULL CHECK(length(input_hash)=64),
  knowledge_link_count INTEGER NOT NULL CHECK(knowledge_link_count >= 0),
  ability_link_count INTEGER NOT NULL CHECK(ability_link_count >= 0),
  confirmed_by TEXT NOT NULL CHECK(length(trim(confirmed_by)) > 0),
  created_at TEXT NOT NULL,
  UNIQUE(base_link_set_id,input_hash)
);

CREATE TRIGGER trg_exam_subjective_link_edit_scope_insert_v2
BEFORE INSERT ON exam_subjective_link_edits_v2
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_assessment_items_v2 source_item
    JOIN exam_assessment_versions_v2 source_version
      ON source_version.id=source_item.assessment_version_id
    JOIN k1_question_versions question ON question.id=source_item.question_version_id
    JOIN exam_assessment_items_v2 base_item ON base_item.id=NEW.base_assessment_item_id
    JOIN exam_assessment_versions_v2 base_version
      ON base_version.id=NEW.base_assessment_version_id
    JOIN exam_assessment_items_v2 adopted_item ON adopted_item.id=NEW.adopted_assessment_item_id
    JOIN exam_assessment_versions_v2 adopted_version
      ON adopted_version.id=NEW.adopted_assessment_version_id
    JOIN k1_link_sets base_links ON base_links.id=NEW.base_link_set_id
    JOIN k1_link_sets adopted_links ON adopted_links.id=NEW.adopted_link_set_id
    WHERE source_item.id=NEW.source_assessment_item_id
      AND source_version.assessment_id=NEW.assessment_id
      AND source_item.question_version_id=NEW.question_version_id
      AND question.question_type=NEW.question_type
      AND base_item.assessment_version_id=base_version.id
      AND base_version.assessment_id=NEW.assessment_id
      AND base_version.state='confirmed'
      AND base_item.question_version_id=NEW.question_version_id
      AND base_item.link_set_id=NEW.base_link_set_id
      AND adopted_item.assessment_version_id=adopted_version.id
      AND adopted_version.assessment_id=NEW.assessment_id
      AND adopted_version.state='confirmed'
      AND adopted_version.supersedes_version_id=base_version.id
      AND adopted_item.question_version_id=NEW.question_version_id
      AND adopted_item.link_set_id=NEW.adopted_link_set_id
      AND base_links.question_version_id=NEW.question_version_id
      AND base_links.state='confirmed'
      AND adopted_links.question_version_id=NEW.question_version_id
      AND adopted_links.state='confirmed'
      AND adopted_links.supersedes_link_set_id=base_links.id
  ) THEN RAISE(ABORT,'EXAM_SUBJECTIVE_LINK_EDIT_SCOPE_MISMATCH') END;

  SELECT CASE WHEN EXISTS (
    SELECT 1 FROM k1_knowledge_links
    WHERE link_set_id=NEW.adopted_link_set_id
      AND source_type IN ('answer_slot','rubric_point')
      AND confirmation_level<>'teacher_confirmed'
  ) THEN RAISE(ABORT,'EXAM_SUBJECTIVE_LINK_EDIT_UNCONFIRMED_KNOWLEDGE') END;

  SELECT CASE WHEN EXISTS (
    SELECT 1 FROM k1_ability_links
    WHERE link_set_id=NEW.adopted_link_set_id
      AND source_type IN ('answer_slot','rubric_point')
      AND confirmation_level<>'teacher_confirmed'
  ) THEN RAISE(ABORT,'EXAM_SUBJECTIVE_LINK_EDIT_UNCONFIRMED_ABILITY') END;
END;

CREATE TRIGGER trg_exam_subjective_link_edit_immutable_update_v2
BEFORE UPDATE ON exam_subjective_link_edits_v2
BEGIN SELECT RAISE(ABORT,'EXAM_SUBJECTIVE_LINK_EDIT_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_subjective_link_edit_immutable_delete_v2
BEFORE DELETE ON exam_subjective_link_edits_v2
BEGIN SELECT RAISE(ABORT,'EXAM_SUBJECTIVE_LINK_EDIT_IMMUTABLE'); END;
