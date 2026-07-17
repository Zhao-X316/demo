-- M2-B3c4：简答评分点增删/重排时，由老师显式确认稳定身份映射。
-- 映射只随新的 answer-source adoption 追加；旧 rubric、作业、成绩和学习证据保持只读。

CREATE TABLE exam_rubric_point_mappings_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  answer_source_adoption_id INTEGER NOT NULL
    REFERENCES exam_answer_source_adoptions_v2(id),
  source_ai_run_id INTEGER NOT NULL REFERENCES ai_runs(id),
  source_assessment_item_id INTEGER NOT NULL REFERENCES exam_assessment_items_v2(id),
  adopted_assessment_item_id INTEGER NOT NULL REFERENCES exam_assessment_items_v2(id),
  mapping_action TEXT NOT NULL
    CHECK(mapping_action IN ('reuse_existing','new_point','retire_existing')),
  candidate_order_index INTEGER,
  previous_rubric_point_public_id TEXT
    REFERENCES k1_rubric_points(public_id),
  adopted_rubric_point_public_id TEXT
    REFERENCES k1_rubric_points(public_id),
  created_by TEXT NOT NULL CHECK(length(trim(created_by)) > 0),
  created_at TEXT NOT NULL,
  CHECK(
    (mapping_action='reuse_existing'
      AND candidate_order_index IS NOT NULL AND candidate_order_index >= 0
      AND previous_rubric_point_public_id IS NOT NULL
      AND adopted_rubric_point_public_id IS NOT NULL)
    OR
    (mapping_action='new_point'
      AND candidate_order_index IS NOT NULL AND candidate_order_index >= 0
      AND previous_rubric_point_public_id IS NULL
      AND adopted_rubric_point_public_id IS NOT NULL)
    OR
    (mapping_action='retire_existing'
      AND candidate_order_index IS NULL
      AND previous_rubric_point_public_id IS NOT NULL
      AND adopted_rubric_point_public_id IS NULL)
  )
);

CREATE UNIQUE INDEX idx_exam_rubric_mapping_candidate_v2
  ON exam_rubric_point_mappings_v2(
    answer_source_adoption_id,source_assessment_item_id,candidate_order_index
  ) WHERE candidate_order_index IS NOT NULL;

CREATE UNIQUE INDEX idx_exam_rubric_mapping_previous_v2
  ON exam_rubric_point_mappings_v2(
    answer_source_adoption_id,source_assessment_item_id,previous_rubric_point_public_id
  ) WHERE previous_rubric_point_public_id IS NOT NULL;

CREATE UNIQUE INDEX idx_exam_rubric_mapping_adopted_v2
  ON exam_rubric_point_mappings_v2(
    answer_source_adoption_id,adopted_assessment_item_id,adopted_rubric_point_public_id
  ) WHERE adopted_rubric_point_public_id IS NOT NULL;

CREATE TRIGGER trg_exam_rubric_mapping_scope_insert_v2
BEFORE INSERT ON exam_rubric_point_mappings_v2
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_answer_source_adoptions_v2 adoption
    JOIN exam_assessment_items_v2 source_item
      ON source_item.id=NEW.source_assessment_item_id
     AND source_item.assessment_version_id=adoption.source_assessment_version_id
    JOIN exam_assessment_items_v2 adopted_item
      ON adopted_item.id=NEW.adopted_assessment_item_id
     AND adopted_item.assessment_version_id=adoption.adopted_assessment_version_id
     AND adopted_item.question_version_id=source_item.question_version_id
    WHERE adoption.id=NEW.answer_source_adoption_id
      AND adoption.source_ai_run_id=NEW.source_ai_run_id
  ) THEN RAISE(ABORT, 'M2_RUBRIC_MAPPING_SCOPE_MISMATCH') END;

  SELECT CASE WHEN NEW.previous_rubric_point_public_id IS NOT NULL AND NOT EXISTS (
    SELECT 1
    FROM exam_assessment_items_v2 source_item
    JOIN k1_rubric_points point
      ON point.rubric_version_id=source_item.rubric_version_id
    WHERE source_item.id=NEW.source_assessment_item_id
      AND point.public_id=NEW.previous_rubric_point_public_id
  ) THEN RAISE(ABORT, 'M2_RUBRIC_MAPPING_PREVIOUS_POINT_MISMATCH') END;

  SELECT CASE WHEN NEW.adopted_rubric_point_public_id IS NOT NULL AND NOT EXISTS (
    SELECT 1
    FROM exam_assessment_items_v2 adopted_item
    JOIN k1_rubric_points point
      ON point.rubric_version_id=adopted_item.rubric_version_id
    WHERE adopted_item.id=NEW.adopted_assessment_item_id
      AND point.public_id=NEW.adopted_rubric_point_public_id
  ) THEN RAISE(ABORT, 'M2_RUBRIC_MAPPING_ADOPTED_POINT_MISMATCH') END;
END;

CREATE TRIGGER trg_exam_rubric_mapping_update_v2
BEFORE UPDATE ON exam_rubric_point_mappings_v2
BEGIN SELECT RAISE(ABORT, 'M2_RUBRIC_MAPPING_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_rubric_mapping_delete_v2
BEFORE DELETE ON exam_rubric_point_mappings_v2
BEGIN SELECT RAISE(ABORT, 'M2_RUBRIC_MAPPING_IMMUTABLE'); END;
