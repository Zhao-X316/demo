-- M2 客观题批改闭环：机器只给建议，教师确认后才形成最终成绩和聚合。

ALTER TABLE exam_questions ADD COLUMN max_score REAL NOT NULL DEFAULT 1;

ALTER TABLE exam_student_answers ADD COLUMN max_score REAL NOT NULL DEFAULT 1;
ALTER TABLE exam_student_answers ADD COLUMN machine_correct INTEGER;
ALTER TABLE exam_student_answers ADD COLUMN human_correct INTEGER;
ALTER TABLE exam_student_answers ADD COLUMN status TEXT NOT NULL DEFAULT 'pending_review';
ALTER TABLE exam_student_answers ADD COLUMN human_note TEXT;
ALTER TABLE exam_student_answers ADD COLUMN decided_at TEXT;
ALTER TABLE exam_student_answers ADD COLUMN updated_at TEXT;

-- 兼容可能已经存在的旧作答：有最终结论的记录视为已由教师确认。
UPDATE exam_student_answers
SET status = CASE WHEN is_correct IS NULL THEN 'pending_review' ELSE 'confirmed' END,
    human_correct = is_correct,
    decided_at = CASE WHEN is_correct IS NULL THEN NULL ELSE created_at END,
    updated_at = created_at;

CREATE INDEX IF NOT EXISTS idx_exam_sa_status
  ON exam_student_answers(status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_exam_sa_student_kp
  ON exam_student_answers(student_id, knowledge_point_id, status);
