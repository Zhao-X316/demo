-- K1-0：教材、课程树、知识点、考点和能力维度。
-- 所有版本行均为 append-only；修订通过新 knowledge map / revision 表达。

CREATE TABLE k1_textbook_editions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  subject_id INTEGER NOT NULL REFERENCES subjects(id),
  publisher_code TEXT NOT NULL,
  edition_code TEXT NOT NULL,
  title TEXT NOT NULL,
  grade TEXT NOT NULL,
  volume TEXT NOT NULL CHECK(volume IN ('all', 'upper', 'lower')),
  curriculum_region TEXT,
  state TEXT NOT NULL DEFAULT 'active'
    CHECK(state IN ('draft', 'active', 'retired')),
  created_at TEXT NOT NULL,
  UNIQUE(subject_id, publisher_code, edition_code, grade, volume)
);

CREATE TABLE k1_knowledge_maps (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  textbook_edition_id INTEGER NOT NULL REFERENCES k1_textbook_editions(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  state TEXT NOT NULL DEFAULT 'draft'
    CHECK(state IN ('draft', 'confirmed', 'retired')),
  supersedes_map_id INTEGER REFERENCES k1_knowledge_maps(id),
  created_at TEXT NOT NULL,
  confirmed_at TEXT,
  UNIQUE(textbook_edition_id, revision),
  CHECK(supersedes_map_id IS NULL OR supersedes_map_id <> id),
  CHECK((state = 'confirmed' AND confirmed_at IS NOT NULL) OR state <> 'confirmed')
);

CREATE TABLE k1_curriculum_nodes (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  stable_id TEXT NOT NULL,
  knowledge_map_id INTEGER NOT NULL REFERENCES k1_knowledge_maps(id),
  parent_id INTEGER REFERENCES k1_curriculum_nodes(id),
  node_type TEXT NOT NULL CHECK(node_type IN ('unit', 'lesson', 'topic')),
  code TEXT,
  title TEXT NOT NULL,
  description TEXT,
  order_index INTEGER NOT NULL DEFAULT 0,
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active', 'deprecated')),
  created_at TEXT NOT NULL,
  UNIQUE(knowledge_map_id, stable_id)
);
CREATE UNIQUE INDEX idx_k1_curriculum_code
  ON k1_curriculum_nodes(knowledge_map_id, code) WHERE code IS NOT NULL;
CREATE INDEX idx_k1_curriculum_parent
  ON k1_curriculum_nodes(knowledge_map_id, parent_id, order_index);

CREATE TABLE k1_knowledge_nodes (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  stable_id TEXT NOT NULL,
  knowledge_map_id INTEGER NOT NULL REFERENCES k1_knowledge_maps(id),
  curriculum_node_id INTEGER REFERENCES k1_curriculum_nodes(id),
  parent_id INTEGER REFERENCES k1_knowledge_nodes(id),
  code TEXT,
  title TEXT NOT NULL,
  description TEXT,
  order_index INTEGER NOT NULL DEFAULT 0,
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active', 'deprecated')),
  created_at TEXT NOT NULL,
  UNIQUE(knowledge_map_id, stable_id)
);
CREATE UNIQUE INDEX idx_k1_knowledge_code
  ON k1_knowledge_nodes(knowledge_map_id, code) WHERE code IS NOT NULL;
CREATE INDEX idx_k1_knowledge_parent
  ON k1_knowledge_nodes(knowledge_map_id, parent_id, order_index);

CREATE TABLE k1_exam_points (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  stable_id TEXT NOT NULL,
  knowledge_map_id INTEGER NOT NULL REFERENCES k1_knowledge_maps(id),
  knowledge_node_id INTEGER REFERENCES k1_knowledge_nodes(id),
  code TEXT,
  title TEXT NOT NULL,
  description TEXT,
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active', 'deprecated')),
  created_at TEXT NOT NULL,
  UNIQUE(knowledge_map_id, stable_id)
);
CREATE UNIQUE INDEX idx_k1_exam_point_code
  ON k1_exam_points(knowledge_map_id, code) WHERE code IS NOT NULL;

CREATE TABLE k1_ability_dimensions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  stable_id TEXT NOT NULL,
  subject_id INTEGER NOT NULL REFERENCES subjects(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  code TEXT NOT NULL,
  title TEXT NOT NULL,
  description TEXT,
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active', 'deprecated')),
  supersedes_dimension_id INTEGER REFERENCES k1_ability_dimensions(id),
  created_at TEXT NOT NULL,
  UNIQUE(stable_id, revision),
  UNIQUE(subject_id, code, revision),
  CHECK(supersedes_dimension_id IS NULL OR supersedes_dimension_id <> id)
);

-- 父子和锚点必须位于同一 knowledge map；SQLite 普通 FK 无法表达这条跨列约束。
CREATE TRIGGER trg_k1_curriculum_parent_same_map_insert
BEFORE INSERT ON k1_curriculum_nodes
WHEN NEW.parent_id IS NOT NULL
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1 FROM k1_curriculum_nodes p
    WHERE p.id = NEW.parent_id AND p.knowledge_map_id = NEW.knowledge_map_id
  ) THEN RAISE(ABORT, 'K1_CURRICULUM_PARENT_MAP_MISMATCH') END;
END;

CREATE TRIGGER trg_k1_knowledge_refs_same_map_insert
BEFORE INSERT ON k1_knowledge_nodes
BEGIN
  SELECT CASE WHEN NEW.parent_id IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM k1_knowledge_nodes p
    WHERE p.id = NEW.parent_id AND p.knowledge_map_id = NEW.knowledge_map_id
  ) THEN RAISE(ABORT, 'K1_KNOWLEDGE_PARENT_MAP_MISMATCH') END;
  SELECT CASE WHEN NEW.curriculum_node_id IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM k1_curriculum_nodes c
    WHERE c.id = NEW.curriculum_node_id AND c.knowledge_map_id = NEW.knowledge_map_id
  ) THEN RAISE(ABORT, 'K1_CURRICULUM_ANCHOR_MAP_MISMATCH') END;
END;

CREATE TRIGGER trg_k1_exam_point_same_map_insert
BEFORE INSERT ON k1_exam_points
WHEN NEW.knowledge_node_id IS NOT NULL
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1 FROM k1_knowledge_nodes k
    WHERE k.id = NEW.knowledge_node_id AND k.knowledge_map_id = NEW.knowledge_map_id
  ) THEN RAISE(ABORT, 'K1_EXAM_POINT_MAP_MISMATCH') END;
END;

-- 版本内容只追加，不覆盖或删除。状态演进通过新 map/revision 和显式映射完成。
CREATE TRIGGER trg_k1_curriculum_immutable_update
BEFORE UPDATE ON k1_curriculum_nodes BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_curriculum_immutable_delete
BEFORE DELETE ON k1_curriculum_nodes BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_knowledge_immutable_update
BEFORE UPDATE ON k1_knowledge_nodes BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_knowledge_immutable_delete
BEFORE DELETE ON k1_knowledge_nodes BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_exam_point_immutable_update
BEFORE UPDATE ON k1_exam_points BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_exam_point_immutable_delete
BEFORE DELETE ON k1_exam_points BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_ability_immutable_update
BEFORE UPDATE ON k1_ability_dimensions BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_ability_immutable_delete
BEFORE DELETE ON k1_ability_dimensions BEGIN SELECT RAISE(ABORT, 'K1_VERSION_IMMUTABLE'); END;
