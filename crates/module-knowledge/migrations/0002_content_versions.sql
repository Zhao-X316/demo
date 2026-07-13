-- K1-1/2 最小内容版本：题目、答案、rubric、link set 与显式 legacy 映射。

CREATE TABLE k1_question_families (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  owner_scope TEXT NOT NULL CHECK(owner_scope IN ('personal', 'school', 'official')),
  owner_id TEXT NOT NULL,
  title TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE k1_questions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  owner_scope TEXT NOT NULL CHECK(owner_scope IN ('personal', 'school', 'official')),
  owner_id TEXT NOT NULL,
  question_family_id INTEGER REFERENCES k1_question_families(id),
  rights_status TEXT NOT NULL DEFAULT 'unknown'
    CHECK(rights_status IN ('unknown', 'cleared', 'restricted')),
  sharing_allowed INTEGER NOT NULL DEFAULT 0 CHECK(sharing_allowed IN (0,1)),
  created_at TEXT NOT NULL,
  CHECK(owner_scope <> 'official' OR sharing_allowed = 1)
);

CREATE TABLE k1_question_versions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  question_id INTEGER NOT NULL REFERENCES k1_questions(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  question_type TEXT NOT NULL
    CHECK(question_type IN ('single', 'multiple', 'true_false', 'fill_blank', 'short_answer')),
  stem TEXT NOT NULL CHECK(length(trim(stem)) > 0),
  material_text TEXT,
  max_score REAL NOT NULL CHECK(max_score > 0),
  content_hash TEXT NOT NULL CHECK(length(content_hash) = 64),
  source_artifact_id INTEGER REFERENCES artifacts(id),
  source_anchor_json TEXT,
  supersedes_version_id INTEGER REFERENCES k1_question_versions(id),
  quality_level TEXT NOT NULL DEFAULT 'L0'
    CHECK(quality_level IN ('C0','L0','L1','L2','L3','L4')),
  state TEXT NOT NULL DEFAULT 'draft'
    CHECK(state IN ('candidate','draft','review_pending','published','deprecated','archived')),
  created_at TEXT NOT NULL,
  UNIQUE(question_id, revision),
  CHECK(supersedes_version_id IS NULL OR supersedes_version_id <> id),
  CHECK(source_anchor_json IS NULL OR (
    COALESCE(json_valid(source_anchor_json), 0) = 1
    AND COALESCE(json_type(source_anchor_json, '$.schema_version') = 'integer', 0)
  ))
);
CREATE INDEX idx_k1_question_versions_exact
  ON k1_question_versions(content_hash, question_type, state);

CREATE TABLE k1_question_options (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  question_version_id INTEGER NOT NULL REFERENCES k1_question_versions(id),
  label TEXT NOT NULL,
  content TEXT NOT NULL CHECK(length(trim(content)) > 0),
  order_index INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  UNIQUE(question_version_id, label)
);

CREATE TABLE k1_answer_key_versions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  question_version_id INTEGER NOT NULL REFERENCES k1_question_versions(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  answer_json TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'draft' CHECK(state IN ('draft','confirmed','retired')),
  supersedes_answer_key_id INTEGER REFERENCES k1_answer_key_versions(id),
  created_at TEXT NOT NULL,
  confirmed_by TEXT,
  confirmed_at TEXT,
  UNIQUE(question_version_id, revision),
  CHECK(COALESCE(json_valid(answer_json), 0) = 1),
  CHECK(COALESCE(json_type(answer_json) = 'object', 0)),
  CHECK(COALESCE(json_type(answer_json, '$.schema_version') = 'integer', 0)),
  CHECK(supersedes_answer_key_id IS NULL OR supersedes_answer_key_id <> id),
  CHECK((state = 'confirmed' AND confirmed_by IS NOT NULL AND confirmed_at IS NOT NULL)
        OR state <> 'confirmed')
);

CREATE TABLE k1_answer_slots (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  stable_id TEXT NOT NULL,
  answer_key_version_id INTEGER NOT NULL REFERENCES k1_answer_key_versions(id),
  order_index INTEGER NOT NULL DEFAULT 0,
  canonical_answers_json TEXT NOT NULL,
  normalization_rules_json TEXT,
  max_score REAL NOT NULL CHECK(max_score > 0),
  created_at TEXT NOT NULL,
  UNIQUE(answer_key_version_id, stable_id),
  CHECK(COALESCE(json_valid(canonical_answers_json), 0) = 1),
  CHECK(COALESCE(json_type(canonical_answers_json) = 'object', 0)),
  CHECK(COALESCE(json_type(canonical_answers_json, '$.schema_version') = 'integer', 0)),
  CHECK(normalization_rules_json IS NULL OR (
    COALESCE(json_valid(normalization_rules_json), 0) = 1
    AND COALESCE(json_type(normalization_rules_json, '$.schema_version') = 'integer', 0)
  ))
);

CREATE TABLE k1_rubric_versions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  question_version_id INTEGER NOT NULL REFERENCES k1_question_versions(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  max_score REAL NOT NULL CHECK(max_score > 0),
  state TEXT NOT NULL DEFAULT 'draft' CHECK(state IN ('draft','confirmed','retired')),
  supersedes_rubric_id INTEGER REFERENCES k1_rubric_versions(id),
  created_at TEXT NOT NULL,
  confirmed_by TEXT,
  confirmed_at TEXT,
  UNIQUE(question_version_id, revision),
  CHECK(supersedes_rubric_id IS NULL OR supersedes_rubric_id <> id),
  CHECK((state = 'confirmed' AND confirmed_by IS NOT NULL AND confirmed_at IS NOT NULL)
        OR state <> 'confirmed')
);

CREATE TABLE k1_rubric_points (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  stable_id TEXT NOT NULL,
  rubric_version_id INTEGER NOT NULL REFERENCES k1_rubric_versions(id),
  order_index INTEGER NOT NULL DEFAULT 0,
  canonical_text TEXT NOT NULL CHECK(length(trim(canonical_text)) > 0),
  allowed_paraphrases_json TEXT,
  required_concepts_json TEXT,
  max_score REAL NOT NULL CHECK(max_score > 0),
  created_at TEXT NOT NULL,
  UNIQUE(rubric_version_id, stable_id),
  CHECK(allowed_paraphrases_json IS NULL OR COALESCE(json_valid(allowed_paraphrases_json), 0) = 1),
  CHECK(required_concepts_json IS NULL OR COALESCE(json_valid(required_concepts_json), 0) = 1)
);

CREATE TABLE k1_contradiction_rules (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  rubric_point_id INTEGER NOT NULL REFERENCES k1_rubric_points(id),
  rule_type TEXT NOT NULL CHECK(rule_type IN ('fact_conflict','mutual_exclusion','score_cap')),
  rule_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  CHECK(COALESCE(json_valid(rule_json), 0) = 1),
  CHECK(COALESCE(json_type(rule_json, '$.schema_version') = 'integer', 0))
);

CREATE TABLE k1_link_sets (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  question_version_id INTEGER NOT NULL REFERENCES k1_question_versions(id),
  knowledge_map_id INTEGER NOT NULL REFERENCES k1_knowledge_maps(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  state TEXT NOT NULL DEFAULT 'draft' CHECK(state IN ('draft','confirmed','retired')),
  supersedes_link_set_id INTEGER REFERENCES k1_link_sets(id),
  created_at TEXT NOT NULL,
  confirmed_by TEXT,
  confirmed_at TEXT,
  UNIQUE(question_version_id, revision),
  CHECK(supersedes_link_set_id IS NULL OR supersedes_link_set_id <> id),
  CHECK((state = 'confirmed' AND confirmed_by IS NOT NULL AND confirmed_at IS NOT NULL)
        OR state <> 'confirmed')
);

-- 内容版本不可变；质量/生命周期单独追加事件并由服务校验后切换当前状态。
CREATE TABLE k1_question_quality_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  question_version_id INTEGER NOT NULL REFERENCES k1_question_versions(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  from_quality TEXT NOT NULL CHECK(from_quality IN ('C0','L0','L1','L2','L3','L4')),
  to_quality TEXT NOT NULL CHECK(to_quality IN ('C0','L0','L1','L2','L3','L4')),
  from_state TEXT NOT NULL CHECK(from_state IN (
    'candidate','draft','review_pending','published','deprecated','archived'
  )),
  to_state TEXT NOT NULL CHECK(to_state IN (
    'candidate','draft','review_pending','published','deprecated','archived'
  )),
  reason TEXT,
  verified_by TEXT NOT NULL,
  verified_at TEXT NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE(question_version_id, revision)
);

CREATE TABLE k1_knowledge_links (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  link_set_id INTEGER NOT NULL REFERENCES k1_link_sets(id),
  source_type TEXT NOT NULL CHECK(source_type IN ('question','option','answer_slot','rubric_point')),
  source_public_id TEXT NOT NULL,
  knowledge_node_id INTEGER NOT NULL REFERENCES k1_knowledge_nodes(id),
  relation_type TEXT NOT NULL CHECK(relation_type IN (
    'direct_assessment','answer_basis','context','prerequisite','distractor','misconception','rubric_basis'
  )),
  confirmation_level TEXT NOT NULL
    CHECK(confirmation_level IN ('machine_suggested','teacher_confirmed')),
  verified_by TEXT,
  verified_at TEXT,
  created_at TEXT NOT NULL,
  UNIQUE(link_set_id, source_type, source_public_id, knowledge_node_id, relation_type),
  CHECK((confirmation_level = 'teacher_confirmed' AND verified_by IS NOT NULL AND verified_at IS NOT NULL)
        OR confirmation_level = 'machine_suggested')
);

CREATE TABLE k1_ability_links (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  link_set_id INTEGER NOT NULL REFERENCES k1_link_sets(id),
  source_type TEXT NOT NULL CHECK(source_type IN ('question','option','answer_slot','rubric_point')),
  source_public_id TEXT NOT NULL,
  ability_dimension_id INTEGER NOT NULL REFERENCES k1_ability_dimensions(id),
  evidence_strength REAL NOT NULL CHECK(evidence_strength >= 0 AND evidence_strength <= 1),
  response_mode TEXT NOT NULL CHECK(response_mode IN (
    'recognition','recall','structured_response','source_analysis','argumentation'
  )),
  confirmation_level TEXT NOT NULL
    CHECK(confirmation_level IN ('machine_suggested','teacher_confirmed')),
  verified_by TEXT,
  verified_at TEXT,
  created_at TEXT NOT NULL,
  UNIQUE(link_set_id, source_type, source_public_id, ability_dimension_id, response_mode),
  CHECK((confirmation_level = 'teacher_confirmed' AND verified_by IS NOT NULL AND verified_at IS NOT NULL)
        OR confirmation_level = 'machine_suggested')
);

-- 显式 legacy 映射：迁移不会自动插入任何猜测关系。
CREATE TABLE k1_legacy_knowledge_mappings (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  source_table TEXT NOT NULL CHECK(source_table IN ('knowledge_points','exam_knowledge_points')),
  legacy_id INTEGER NOT NULL,
  revision INTEGER NOT NULL CHECK(revision > 0),
  knowledge_node_id INTEGER NOT NULL REFERENCES k1_knowledge_nodes(id),
  mapping_method TEXT NOT NULL CHECK(mapping_method IN ('exact_code','manual','verified_import')),
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','reverted')),
  verified_by TEXT NOT NULL,
  verified_at TEXT NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE(source_table, legacy_id, revision)
);
CREATE UNIQUE INDEX idx_k1_legacy_knowledge_active
  ON k1_legacy_knowledge_mappings(source_table, legacy_id) WHERE state='active';

CREATE TABLE k1_legacy_question_mappings (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  source_table TEXT NOT NULL CHECK(source_table = 'exam_questions'),
  legacy_id INTEGER NOT NULL,
  revision INTEGER NOT NULL CHECK(revision > 0),
  question_version_id INTEGER NOT NULL REFERENCES k1_question_versions(id),
  answer_key_version_id INTEGER REFERENCES k1_answer_key_versions(id),
  rubric_version_id INTEGER REFERENCES k1_rubric_versions(id),
  link_set_id INTEGER REFERENCES k1_link_sets(id),
  mapping_method TEXT NOT NULL CHECK(mapping_method IN ('exact_content','manual','verified_import')),
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','reverted')),
  verified_by TEXT NOT NULL,
  verified_at TEXT NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE(source_table, legacy_id, revision)
);
CREATE UNIQUE INDEX idx_k1_legacy_question_active
  ON k1_legacy_question_mappings(source_table, legacy_id) WHERE state='active';

-- link set 固定的 knowledge map 必须与知识链接目标一致。
CREATE TRIGGER trg_k1_knowledge_link_map_insert
BEFORE INSERT ON k1_knowledge_links
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1 FROM k1_link_sets ls
    JOIN k1_knowledge_nodes kn ON kn.id = NEW.knowledge_node_id
    WHERE ls.id = NEW.link_set_id AND ls.knowledge_map_id = kn.knowledge_map_id
  ) THEN RAISE(ABORT, 'K1_LINK_MAP_MISMATCH') END;
END;

-- 内容事实 append-only；仅允许通过服务切换显式质量/生命周期字段。
CREATE TRIGGER trg_k1_question_version_immutable_update
BEFORE UPDATE ON k1_question_versions
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.question_id IS NOT OLD.question_id
  OR NEW.revision IS NOT OLD.revision
  OR NEW.question_type IS NOT OLD.question_type
  OR NEW.stem IS NOT OLD.stem
  OR NEW.material_text IS NOT OLD.material_text
  OR NEW.max_score IS NOT OLD.max_score
  OR NEW.content_hash IS NOT OLD.content_hash
  OR NEW.source_artifact_id IS NOT OLD.source_artifact_id
  OR NEW.source_anchor_json IS NOT OLD.source_anchor_json
  OR NEW.supersedes_version_id IS NOT OLD.supersedes_version_id
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_question_quality_update_guard
BEFORE UPDATE OF quality_level, state ON k1_question_versions
WHEN (NEW.quality_level IS NOT OLD.quality_level OR NEW.state IS NOT OLD.state)
  AND NOT EXISTS (
    SELECT 1 FROM k1_question_quality_events e
    WHERE e.question_version_id=OLD.id
      AND e.from_quality=OLD.quality_level
      AND e.to_quality=NEW.quality_level
      AND e.from_state=OLD.state
      AND e.to_state=NEW.state
  )
BEGIN SELECT RAISE(ABORT, 'K1_QUALITY_EVENT_REQUIRED'); END;
CREATE TRIGGER trg_k1_question_version_immutable_delete
BEFORE DELETE ON k1_question_versions BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_option_immutable_update
BEFORE UPDATE ON k1_question_options BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_option_immutable_delete
BEFORE DELETE ON k1_question_options BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_answer_version_immutable_update
BEFORE UPDATE ON k1_answer_key_versions
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.question_version_id IS NOT OLD.question_version_id
  OR NEW.revision IS NOT OLD.revision
  OR NEW.answer_json IS NOT OLD.answer_json
  OR NEW.supersedes_answer_key_id IS NOT OLD.supersedes_answer_key_id
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_answer_version_immutable_delete
BEFORE DELETE ON k1_answer_key_versions BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_answer_slot_immutable_update
BEFORE UPDATE ON k1_answer_slots BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_answer_slot_immutable_delete
BEFORE DELETE ON k1_answer_slots BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_rubric_version_immutable_update
BEFORE UPDATE ON k1_rubric_versions
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.question_version_id IS NOT OLD.question_version_id
  OR NEW.revision IS NOT OLD.revision
  OR NEW.max_score IS NOT OLD.max_score
  OR NEW.supersedes_rubric_id IS NOT OLD.supersedes_rubric_id
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_rubric_version_immutable_delete
BEFORE DELETE ON k1_rubric_versions BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_rubric_point_immutable_update
BEFORE UPDATE ON k1_rubric_points BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_rubric_point_immutable_delete
BEFORE DELETE ON k1_rubric_points BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_link_set_immutable_update
BEFORE UPDATE ON k1_link_sets
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.question_version_id IS NOT OLD.question_version_id
  OR NEW.knowledge_map_id IS NOT OLD.knowledge_map_id
  OR NEW.revision IS NOT OLD.revision
  OR NEW.supersedes_link_set_id IS NOT OLD.supersedes_link_set_id
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_link_set_immutable_delete
BEFORE DELETE ON k1_link_sets BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_knowledge_link_immutable_update
BEFORE UPDATE ON k1_knowledge_links BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_knowledge_link_immutable_delete
BEFORE DELETE ON k1_knowledge_links BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_ability_link_immutable_update
BEFORE UPDATE ON k1_ability_links BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_ability_link_immutable_delete
BEFORE DELETE ON k1_ability_links BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_quality_event_immutable_update
BEFORE UPDATE ON k1_question_quality_events BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_quality_event_immutable_delete
BEFORE DELETE ON k1_question_quality_events BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;

-- 显式映射可切 active/reverted，但不能改写既有 revision 的事实或删除历史。
CREATE TRIGGER trg_k1_legacy_knowledge_mapping_update
BEFORE UPDATE ON k1_legacy_knowledge_mappings
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.source_table IS NOT OLD.source_table
  OR NEW.legacy_id IS NOT OLD.legacy_id
  OR NEW.revision IS NOT OLD.revision
  OR NEW.knowledge_node_id IS NOT OLD.knowledge_node_id
  OR NEW.mapping_method IS NOT OLD.mapping_method
  OR NEW.verified_by IS NOT OLD.verified_by
  OR NEW.verified_at IS NOT OLD.verified_at
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'K1_MAPPING_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_legacy_knowledge_mapping_delete
BEFORE DELETE ON k1_legacy_knowledge_mappings BEGIN SELECT RAISE(ABORT, 'K1_MAPPING_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_legacy_question_mapping_update
BEFORE UPDATE ON k1_legacy_question_mappings
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.source_table IS NOT OLD.source_table
  OR NEW.legacy_id IS NOT OLD.legacy_id
  OR NEW.revision IS NOT OLD.revision
  OR NEW.question_version_id IS NOT OLD.question_version_id
  OR NEW.answer_key_version_id IS NOT OLD.answer_key_version_id
  OR NEW.rubric_version_id IS NOT OLD.rubric_version_id
  OR NEW.link_set_id IS NOT OLD.link_set_id
  OR NEW.mapping_method IS NOT OLD.mapping_method
  OR NEW.verified_by IS NOT OLD.verified_by
  OR NEW.verified_at IS NOT OLD.verified_at
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'K1_MAPPING_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_legacy_question_mapping_delete
BEFORE DELETE ON k1_legacy_question_mappings BEGIN SELECT RAISE(ABORT, 'K1_MAPPING_IMMUTABLE'); END;
