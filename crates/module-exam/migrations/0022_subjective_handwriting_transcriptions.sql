-- M2-B3a2：答题卡主观题区的追加式手写转写账本。
-- OCR 只读取学生裁图，不读取标准答案；本表不创建老师分数、发布或学习证据。

CREATE TABLE exam_subjective_transcription_revisions_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  attempt_id INTEGER NOT NULL REFERENCES exam_attempts_v2(id),
  assessment_item_id INTEGER NOT NULL REFERENCES exam_assessment_items_v2(id),
  answer_region_revision_id INTEGER NOT NULL REFERENCES exam_answer_region_revisions_v2(id),
  question_type TEXT NOT NULL CHECK(question_type IN ('fill_blank','short_answer')),
  revision INTEGER NOT NULL CHECK(revision > 0),
  source_ai_run_id INTEGER REFERENCES ai_runs(id),
  result_state TEXT NOT NULL CHECK(result_state IN (
    'recognized','not_written','unreadable','recognize_failed','ambiguous_final'
  )),
  raw_ocr_text TEXT,
  normalized_text TEXT,
  teacher_corrected_text TEXT,
  confidence REAL CHECK(confidence IS NULL OR (confidence >= 0.0 AND confidence <= 1.0)),
  failure_meta_json TEXT,
  corrected_by TEXT,
  corrected_at TEXT,
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','superseded','voided')),
  created_at TEXT NOT NULL,
  UNIQUE(attempt_id,assessment_item_id,answer_region_revision_id,revision),
  CHECK(failure_meta_json IS NULL OR (
    COALESCE(json_valid(failure_meta_json),0)=1
    AND COALESCE(json_type(failure_meta_json),0)='object'
    AND COALESCE(json_type(failure_meta_json,'$.schema_version'),0)='integer'
  )),
  CHECK((result_state='recognized' AND raw_ocr_text IS NOT NULL
         AND length(trim(raw_ocr_text)) > 0
         AND normalized_text IS NOT NULL AND length(trim(normalized_text)) > 0
         AND confidence IS NOT NULL AND failure_meta_json IS NULL)
        OR (result_state='recognize_failed' AND raw_ocr_text IS NULL
            AND normalized_text IS NULL AND confidence IS NULL
            AND failure_meta_json IS NOT NULL)
        OR (result_state IN ('not_written','unreadable','ambiguous_final')
            AND raw_ocr_text IS NULL AND normalized_text IS NULL
            AND confidence IS NULL AND failure_meta_json IS NULL)),
  CHECK((teacher_corrected_text IS NOT NULL AND length(trim(teacher_corrected_text)) > 0
         AND corrected_by IS NOT NULL AND length(trim(corrected_by)) > 0
         AND corrected_at IS NOT NULL)
        OR (teacher_corrected_text IS NULL AND corrected_by IS NULL AND corrected_at IS NULL))
);

CREATE UNIQUE INDEX idx_exam_subjective_transcription_active_v2
  ON exam_subjective_transcription_revisions_v2(
    attempt_id,assessment_item_id,answer_region_revision_id
  ) WHERE state='active';

CREATE TRIGGER trg_exam_subjective_transcription_scope_insert_v2
BEFORE INSERT ON exam_subjective_transcription_revisions_v2
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_answer_sheet_region_routes_v2 route
    JOIN exam_answer_sheet_page_materializations_v2 materialization
      ON materialization.id=route.materialization_id
    JOIN exam_answer_region_revisions_v2 region
      ON region.id=route.answer_region_revision_id
     AND region.id=NEW.answer_region_revision_id
     AND region.assessment_item_id=NEW.assessment_item_id
     AND region.state='active' AND region.decision='teacher_confirmed'
    JOIN exam_page_alignment_revisions_v2 alignment
      ON alignment.id=region.alignment_revision_id
     AND alignment.id=materialization.alignment_revision_id
     AND alignment.state='active' AND alignment.decision='teacher_confirmed'
    JOIN exam_page_match_revisions_v2 page_match
      ON page_match.id=alignment.match_revision_id
     AND page_match.state='active' AND page_match.decision='teacher_confirmed'
    JOIN exam_attempts_v2 attempt
      ON attempt.id=page_match.attempt_id AND attempt.id=NEW.attempt_id
     AND attempt.state<>'voided'
    JOIN exam_assessment_items_v2 item
      ON item.id=NEW.assessment_item_id
     AND item.assessment_version_id=attempt.assessment_version_id
     AND item.state='active'
    JOIN k1_question_versions question
      ON question.id=item.question_version_id AND question.state='published'
    JOIN exam_ingest_pages_v2 page
      ON page.id=region.page_id AND page.state<>'voided'
    JOIN exam_material_type_revisions_v2 material_type
      ON material_type.ingest_batch_id=page.batch_id
     AND material_type.state='active'
     AND material_type.decision='teacher_confirmed'
     AND material_type.material_type='answer_sheet'
    WHERE route.recognition_route='handwriting_ocr'
      AND route.question_type=NEW.question_type
      AND question.question_type=NEW.question_type
  ) THEN RAISE(ABORT,'M2_SUBJECTIVE_TRANSCRIPTION_SCOPE_MISMATCH') END;
END;

CREATE TRIGGER trg_exam_subjective_transcription_content_guard_v2
BEFORE UPDATE ON exam_subjective_transcription_revisions_v2
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.attempt_id IS NOT OLD.attempt_id
  OR NEW.assessment_item_id IS NOT OLD.assessment_item_id
  OR NEW.answer_region_revision_id IS NOT OLD.answer_region_revision_id
  OR NEW.question_type IS NOT OLD.question_type
  OR NEW.revision IS NOT OLD.revision
  OR NEW.source_ai_run_id IS NOT OLD.source_ai_run_id
  OR NEW.result_state IS NOT OLD.result_state
  OR NEW.raw_ocr_text IS NOT OLD.raw_ocr_text
  OR NEW.normalized_text IS NOT OLD.normalized_text
  OR NEW.teacher_corrected_text IS NOT OLD.teacher_corrected_text
  OR NEW.confidence IS NOT OLD.confidence
  OR NEW.failure_meta_json IS NOT OLD.failure_meta_json
  OR NEW.corrected_by IS NOT OLD.corrected_by
  OR NEW.corrected_at IS NOT OLD.corrected_at
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT,'M2_SUBJECTIVE_TRANSCRIPTION_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_subjective_transcription_delete_v2
BEFORE DELETE ON exam_subjective_transcription_revisions_v2
BEGIN SELECT RAISE(ABORT,'M2_SUBJECTIVE_TRANSCRIPTION_IMMUTABLE'); END;
