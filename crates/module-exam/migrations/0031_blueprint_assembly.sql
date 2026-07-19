-- K1-4：可解释组卷蓝图确认账本。
-- 预览不落库；老师确认后，蓝图、入选题目解释和已确认作业版本在同一事务中冻结。

CREATE TABLE exam_blueprint_assemblies_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  request_key TEXT NOT NULL UNIQUE CHECK(length(trim(request_key)) > 0),
  request_hash TEXT NOT NULL CHECK(length(request_hash) = 64),
  class_id INTEGER NOT NULL REFERENCES classes(id),
  knowledge_map_id INTEGER NOT NULL REFERENCES k1_knowledge_maps(id),
  curriculum_node_id INTEGER REFERENCES k1_curriculum_nodes(id),
  title TEXT NOT NULL CHECK(length(trim(title)) > 0),
  total_score REAL NOT NULL CHECK(total_score > 0),
  question_type_targets_json TEXT NOT NULL,
  required_knowledge_node_ids_json TEXT NOT NULL,
  preview_hash TEXT NOT NULL CHECK(length(preview_hash) = 64),
  selected_set_hash TEXT NOT NULL CHECK(length(selected_set_hash) = 64),
  assessment_id INTEGER NOT NULL UNIQUE REFERENCES exam_assessments_v2(id),
  assessment_version_id INTEGER NOT NULL UNIQUE REFERENCES exam_assessment_versions_v2(id),
  state TEXT NOT NULL DEFAULT 'confirmed' CHECK(state = 'confirmed'),
  confirmed_by TEXT NOT NULL CHECK(length(trim(confirmed_by)) > 0),
  confirmed_at TEXT NOT NULL,
  created_at TEXT NOT NULL,
  CHECK(COALESCE(json_valid(question_type_targets_json), 0) = 1),
  CHECK(COALESCE(json_type(question_type_targets_json) = 'array', 0)),
  CHECK(COALESCE(json_valid(required_knowledge_node_ids_json), 0) = 1),
  CHECK(COALESCE(json_type(required_knowledge_node_ids_json) = 'array', 0))
);

CREATE TABLE exam_blueprint_items_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  assembly_id INTEGER NOT NULL REFERENCES exam_blueprint_assemblies_v2(id),
  question_version_id INTEGER NOT NULL REFERENCES k1_question_versions(id),
  answer_key_version_id INTEGER NOT NULL REFERENCES k1_answer_key_versions(id),
  rubric_version_id INTEGER NOT NULL REFERENCES k1_rubric_versions(id),
  link_set_id INTEGER NOT NULL REFERENCES k1_link_sets(id),
  order_index INTEGER NOT NULL CHECK(order_index >= 0),
  score REAL NOT NULL CHECK(score > 0),
  explanation_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE(assembly_id, order_index),
  UNIQUE(assembly_id, question_version_id),
  CHECK(COALESCE(json_valid(explanation_json), 0) = 1),
  CHECK(COALESCE(json_type(explanation_json) = 'object', 0)),
  CHECK(COALESCE(json_type(explanation_json, '$.schema_version') = 'integer', 0))
);

CREATE INDEX idx_exam_blueprint_class_confirmed_v2
  ON exam_blueprint_assemblies_v2(class_id, confirmed_at DESC);

CREATE TRIGGER trg_exam_blueprint_scope_insert_v2
BEFORE INSERT ON exam_blueprint_assemblies_v2
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1 FROM k1_knowledge_maps map
    WHERE map.id=NEW.knowledge_map_id AND map.state='confirmed'
  ) THEN RAISE(ABORT,'K1_BLUEPRINT_MAP_NOT_CONFIRMED') END;
  SELECT CASE WHEN NEW.curriculum_node_id IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM k1_curriculum_nodes node
    WHERE node.id=NEW.curriculum_node_id
      AND node.knowledge_map_id=NEW.knowledge_map_id
      AND node.state='active'
  ) THEN RAISE(ABORT,'K1_BLUEPRINT_CURRICULUM_SCOPE_MISMATCH') END;
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_assessment_versions_v2 version
    JOIN exam_assessments_v2 assessment ON assessment.id=version.assessment_id
    WHERE version.id=NEW.assessment_version_id
      AND version.assessment_id=NEW.assessment_id
      AND version.state='confirmed'
      AND assessment.class_id=NEW.class_id
      AND assessment.audience_kind='class'
      AND assessment.state='active'
  ) THEN RAISE(ABORT,'K1_BLUEPRINT_ASSESSMENT_NOT_FROZEN') END;
END;

CREATE TRIGGER trg_exam_blueprint_item_refs_insert_v2
BEFORE INSERT ON exam_blueprint_items_v2
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_blueprint_assemblies_v2 assembly
    JOIN exam_assessment_items_v2 item
      ON item.assessment_version_id=assembly.assessment_version_id
     AND item.question_version_id=NEW.question_version_id
     AND item.answer_key_version_id=NEW.answer_key_version_id
     AND item.rubric_version_id=NEW.rubric_version_id
     AND item.link_set_id=NEW.link_set_id
     AND item.order_index=NEW.order_index
     AND ABS(item.score-NEW.score) < 0.000001
     AND item.state='active'
    WHERE assembly.id=NEW.assembly_id
  ) THEN RAISE(ABORT,'K1_BLUEPRINT_ITEM_ASSESSMENT_MISMATCH') END;
END;

CREATE TRIGGER trg_exam_blueprint_assembly_update_v2
BEFORE UPDATE ON exam_blueprint_assemblies_v2
BEGIN SELECT RAISE(ABORT,'K1_BLUEPRINT_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_blueprint_assembly_delete_v2
BEFORE DELETE ON exam_blueprint_assemblies_v2
BEGIN SELECT RAISE(ABORT,'K1_BLUEPRINT_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_blueprint_item_update_v2
BEFORE UPDATE ON exam_blueprint_items_v2
BEGIN SELECT RAISE(ABORT,'K1_BLUEPRINT_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_blueprint_item_delete_v2
BEFORE DELETE ON exam_blueprint_items_v2
BEGIN SELECT RAISE(ABORT,'K1_BLUEPRINT_IMMUTABLE'); END;
