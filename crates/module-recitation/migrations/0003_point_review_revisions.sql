-- M1.1 老师逐点评审 revision。
--
-- 总体 pass/fail 仍由 verdicts + decision_effects 管理；本迁移只记录老师是否明确
-- 接受或修正了某一次结构化机器评分中的每个评分点。仅点击总体结论不会创建本表记录。

CREATE TABLE rec_point_review_revisions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  submission_id INTEGER NOT NULL REFERENCES submissions(id) ON DELETE RESTRICT,
  score_run_id INTEGER NOT NULL REFERENCES rec_score_runs(id) ON DELETE RESTRICT,
  verdict_id INTEGER NOT NULL REFERENCES verdicts(id) ON DELETE RESTRICT,
  decision_effect_id INTEGER NOT NULL REFERENCES decision_effects(id) ON DELETE RESTRICT,
  revision INTEGER NOT NULL CHECK(revision >= 1),
  item_count INTEGER NOT NULL CHECK(item_count >= 1),
  definition_hash TEXT NOT NULL
    CHECK(length(definition_hash)=64
      AND lower(definition_hash)=definition_hash
      AND definition_hash NOT GLOB '*[^0-9a-f]*'),
  overall_result TEXT NOT NULL CHECK(overall_result IN ('pass','fail')),
  state TEXT NOT NULL DEFAULT 'building'
    CHECK(state IN ('building','active','superseded','reverted')),
  created_by TEXT NOT NULL CHECK(length(trim(created_by)) > 0),
  created_at TEXT NOT NULL,
  superseded_at TEXT,
  reverted_at TEXT,
  UNIQUE(submission_id, revision),
  CHECK((state IN ('building','active') AND superseded_at IS NULL AND reverted_at IS NULL)
        OR (state='superseded' AND superseded_at IS NOT NULL AND reverted_at IS NULL)
        OR (state='reverted' AND reverted_at IS NOT NULL))
);
CREATE UNIQUE INDEX idx_rec_point_review_active
  ON rec_point_review_revisions(submission_id) WHERE state='active';
CREATE INDEX idx_rec_point_review_effect
  ON rec_point_review_revisions(decision_effect_id, state, revision DESC);

CREATE TABLE rec_point_review_items (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  review_revision_id INTEGER NOT NULL
    REFERENCES rec_point_review_revisions(id) ON DELETE RESTRICT,
  rubric_point_id INTEGER NOT NULL
    REFERENCES rec_rubric_points(id) ON DELETE RESTRICT,
  source_point_result_id INTEGER NOT NULL
    REFERENCES rec_point_results(id) ON DELETE RESTRICT,
  confirmation_level TEXT NOT NULL
    CHECK(confirmation_level IN ('accepted','corrected')),
  confirmed_state TEXT NOT NULL
    CHECK(confirmed_state IN ('covered','partial','omitted','contradiction','uncertain')),
  evidence_spans_json TEXT NOT NULL,
  teacher_note TEXT,
  created_at TEXT NOT NULL,
  UNIQUE(review_revision_id, rubric_point_id),
  UNIQUE(review_revision_id, source_point_result_id),
  CHECK(COALESCE(json_valid(evidence_spans_json),0)=1),
  CHECK(COALESCE(json_type(evidence_spans_json),0)='array'),
  CHECK((confirmation_level='accepted')
         OR (teacher_note IS NOT NULL AND length(trim(teacher_note)) > 0))
);
CREATE INDEX idx_rec_point_review_item_revision
  ON rec_point_review_items(review_revision_id, rubric_point_id);

-- review 必须绑定同一 submission 的当前评分、verdict 和仍生效的效果账本。
CREATE TRIGGER trg_rec_point_review_scope_insert
BEFORE INSERT ON rec_point_review_revisions
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM rec_score_runs score
    JOIN verdicts verdict
      ON verdict.id=NEW.verdict_id
     AND verdict.submission_id=NEW.submission_id
    JOIN decision_effects effect
      ON effect.id=NEW.decision_effect_id
     AND effect.verdict_id=verdict.id
    WHERE score.id=NEW.score_run_id
      AND score.submission_id=NEW.submission_id
      AND score.state='active'
      AND effect.module='recitation'
      AND effect.state='active'
      AND effect.result=NEW.overall_result
      AND verdict.human_result=NEW.overall_result
  ) THEN RAISE(ABORT,'M1_POINT_REVIEW_SCOPE_MISMATCH') END;
END;

-- accepted 必须原样接受机器结论；corrected 才允许写老师修正后的状态与证据。
CREATE TRIGGER trg_rec_point_review_item_scope_insert
BEFORE INSERT ON rec_point_review_items
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM rec_point_review_revisions review
    JOIN rec_point_results result
      ON result.id=NEW.source_point_result_id
     AND result.score_run_id=review.score_run_id
    WHERE review.id=NEW.review_revision_id
      AND review.state='building'
      AND result.rubric_point_id=NEW.rubric_point_id
      AND (
        NEW.confirmation_level='corrected'
        OR (
          NEW.confirmed_state=result.machine_state
          AND NEW.evidence_spans_json=result.evidence_spans_json
        )
      )
  ) THEN RAISE(ABORT,'M1_POINT_REVIEW_ITEM_SCOPE_MISMATCH') END;
END;

CREATE TRIGGER trg_rec_point_review_fact_immutable
BEFORE UPDATE ON rec_point_review_revisions
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.submission_id IS NOT OLD.submission_id
  OR NEW.score_run_id IS NOT OLD.score_run_id
  OR NEW.verdict_id IS NOT OLD.verdict_id
  OR NEW.decision_effect_id IS NOT OLD.decision_effect_id
  OR NEW.revision IS NOT OLD.revision
  OR NEW.item_count IS NOT OLD.item_count
  OR NEW.definition_hash IS NOT OLD.definition_hash
  OR NEW.overall_result IS NOT OLD.overall_result
  OR NEW.created_by IS NOT OLD.created_by
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT,'M1_POINT_REVIEW_IMMUTABLE'); END;

CREATE TRIGGER trg_rec_point_review_state_transition
BEFORE UPDATE OF state ON rec_point_review_revisions
WHEN NOT (
  (OLD.state='building' AND NEW.state='active')
  OR (OLD.state='active' AND NEW.state IN ('superseded','reverted'))
  OR (OLD.state='superseded' AND NEW.state='reverted')
)
BEGIN SELECT RAISE(ABORT,'M1_POINT_REVIEW_STATE_TRANSITION_INVALID'); END;

CREATE TRIGGER trg_rec_point_review_seal_complete
BEFORE UPDATE OF state ON rec_point_review_revisions
WHEN OLD.state='building'
  AND NEW.state='active'
  AND (
    SELECT count(*) FROM rec_point_review_items item
    WHERE item.review_revision_id=OLD.id
  ) <> OLD.item_count
BEGIN SELECT RAISE(ABORT,'M1_POINT_REVIEW_ITEMS_INCOMPLETE'); END;

CREATE TRIGGER trg_rec_point_review_timestamps_immutable
BEFORE UPDATE ON rec_point_review_revisions
WHEN NEW.state=OLD.state
  AND (
    NEW.superseded_at IS NOT OLD.superseded_at
    OR NEW.reverted_at IS NOT OLD.reverted_at
  )
BEGIN SELECT RAISE(ABORT,'M1_POINT_REVIEW_TIMESTAMP_IMMUTABLE'); END;

CREATE TRIGGER trg_rec_point_review_delete
BEFORE DELETE ON rec_point_review_revisions
BEGIN SELECT RAISE(ABORT,'M1_POINT_REVIEW_IMMUTABLE'); END;

CREATE TRIGGER trg_rec_point_review_item_update
BEFORE UPDATE ON rec_point_review_items
BEGIN SELECT RAISE(ABORT,'M1_POINT_REVIEW_ITEM_IMMUTABLE'); END;

CREATE TRIGGER trg_rec_point_review_item_delete
BEFORE DELETE ON rec_point_review_items
BEGIN SELECT RAISE(ABORT,'M1_POINT_REVIEW_ITEM_IMMUTABLE'); END;

-- 先回滚逐点评审，再允许回滚其总体终审效果，避免留下仍生效的逐点结论。
CREATE TRIGGER trg_rec_point_review_before_effect_revert
BEFORE UPDATE OF state ON decision_effects
WHEN OLD.module='recitation'
  AND OLD.state='active'
  AND NEW.state='reverted'
  AND EXISTS (
    SELECT 1 FROM rec_point_review_revisions review
    WHERE review.decision_effect_id=OLD.id AND review.state='active'
  )
BEGIN SELECT RAISE(ABORT,'M1_POINT_REVIEW_MUST_REVERT_FIRST'); END;
