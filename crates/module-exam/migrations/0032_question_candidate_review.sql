-- M2.5-2：老师整理批改自动沉淀的私有 C0 候选。
-- 原候选和原题目版本保持不可变；老师确认后创建新的 L1 版本，或永久记录丢弃决定。

CREATE TABLE exam_question_candidate_reviews_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  request_key TEXT NOT NULL UNIQUE CHECK(length(trim(request_key)) > 0),
  request_hash TEXT NOT NULL CHECK(length(request_hash) = 64),
  candidate_id INTEGER NOT NULL UNIQUE REFERENCES exam_question_candidates_v2(id),
  action TEXT NOT NULL CHECK(action IN ('promote_l1','discard')),
  source_question_version_id INTEGER NOT NULL REFERENCES k1_question_versions(id),
  result_question_version_id INTEGER REFERENCES k1_question_versions(id),
  result_answer_key_version_id INTEGER REFERENCES k1_answer_key_versions(id),
  reviewed_by TEXT NOT NULL CHECK(length(trim(reviewed_by)) > 0),
  note TEXT,
  reviewed_at TEXT NOT NULL,
  CHECK(
    (action='promote_l1'
      AND result_question_version_id IS NOT NULL
      AND result_answer_key_version_id IS NOT NULL)
    OR
    (action='discard'
      AND result_question_version_id IS NULL
      AND result_answer_key_version_id IS NULL)
  )
);

CREATE TRIGGER trg_exam_question_candidate_review_immutable_update_v2
BEFORE UPDATE ON exam_question_candidate_reviews_v2
BEGIN SELECT RAISE(ABORT, 'M2_QUESTION_CANDIDATE_REVIEW_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_question_candidate_review_immutable_delete_v2
BEFORE DELETE ON exam_question_candidate_reviews_v2
BEGIN SELECT RAISE(ABORT, 'M2_QUESTION_CANDIDATE_REVIEW_RETAINED'); END;
