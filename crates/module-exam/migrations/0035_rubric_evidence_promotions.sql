-- M2.5-3a：老师把一次逐评分点给分时引用的学生表述，显式加入未来评分规则。
--
-- 只追加新的 K1 rubric/link set 与 assessment version；当前评分、发布与学习证据
-- 均保持原样。提升来源和版本采用不可变账本，禁止事后改写。

CREATE TABLE exam_rubric_evidence_promotions_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  grade_decision_id INTEGER NOT NULL REFERENCES exam_grade_decisions_v2(id),
  component_id INTEGER NOT NULL UNIQUE
    REFERENCES exam_grade_decision_subjective_components_v2(id),
  suggestion_id INTEGER NOT NULL REFERENCES exam_subjective_grade_suggestions_v2(id),
  transcription_revision_id INTEGER NOT NULL
    REFERENCES exam_subjective_transcription_revisions_v2(id),
  assessment_id INTEGER NOT NULL REFERENCES exam_assessments_v2(id),
  question_version_id INTEGER NOT NULL REFERENCES k1_question_versions(id),
  source_assessment_version_id INTEGER NOT NULL REFERENCES exam_assessment_versions_v2(id),
  source_assessment_item_id INTEGER NOT NULL REFERENCES exam_assessment_items_v2(id),
  source_rubric_version_id INTEGER NOT NULL REFERENCES k1_rubric_versions(id),
  base_assessment_version_id INTEGER NOT NULL REFERENCES exam_assessment_versions_v2(id),
  base_assessment_item_id INTEGER NOT NULL REFERENCES exam_assessment_items_v2(id),
  base_rubric_version_id INTEGER NOT NULL REFERENCES k1_rubric_versions(id),
  base_link_set_id INTEGER NOT NULL REFERENCES k1_link_sets(id),
  adopted_assessment_version_id INTEGER NOT NULL UNIQUE
    REFERENCES exam_assessment_versions_v2(id),
  adopted_assessment_item_id INTEGER NOT NULL REFERENCES exam_assessment_items_v2(id),
  adopted_rubric_version_id INTEGER NOT NULL UNIQUE REFERENCES k1_rubric_versions(id),
  adopted_link_set_id INTEGER NOT NULL UNIQUE REFERENCES k1_link_sets(id),
  rubric_point_stable_id TEXT NOT NULL CHECK(length(trim(rubric_point_stable_id)) > 0),
  evidence_text TEXT NOT NULL CHECK(length(trim(evidence_text)) > 0),
  normalized_text TEXT NOT NULL CHECK(length(trim(normalized_text)) > 0),
  confirmed_by TEXT NOT NULL CHECK(length(trim(confirmed_by)) > 0),
  created_at TEXT NOT NULL,
  UNIQUE(
    assessment_id,question_version_id,rubric_point_stable_id,normalized_text
  )
);

CREATE TRIGGER trg_exam_rubric_evidence_promotion_source_insert_v2
BEFORE INSERT ON exam_rubric_evidence_promotions_v2
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_grade_decisions_v2 decision
    JOIN exam_grade_decision_subjective_sources_v2 decision_source
      ON decision_source.grade_decision_id=decision.id
    JOIN exam_grade_decision_subjective_components_v2 component
      ON component.id=NEW.component_id
     AND component.grade_decision_id=decision.id
     AND component.source_type='rubric_point'
     AND component.teacher_score > 0.000001
     AND component.evidence_text IS NOT NULL
    JOIN exam_subjective_grade_suggestions_v2 suggestion
      ON suggestion.id=decision_source.suggestion_id
    JOIN exam_subjective_transcription_revisions_v2 transcription
      ON transcription.id=decision_source.transcription_revision_id
     AND transcription.id=suggestion.transcription_revision_id
    JOIN exam_attempts_v2 attempt ON attempt.id=decision.attempt_id
    JOIN exam_assessment_items_v2 source_item
      ON source_item.id=decision.assessment_item_id
     AND source_item.assessment_version_id=attempt.assessment_version_id
    JOIN exam_assessment_versions_v2 source_version
      ON source_version.id=source_item.assessment_version_id
    JOIN k1_question_versions question
      ON question.id=source_item.question_version_id
     AND question.question_type='short_answer'
    WHERE decision.id=NEW.grade_decision_id
      AND decision.state='active'
      AND decision.confirmation_level='teacher_corrected'
      AND suggestion.id=NEW.suggestion_id
      AND suggestion.state='active'
      AND transcription.id=NEW.transcription_revision_id
      AND transcription.state='active'
      AND transcription.result_state='recognized'
      AND source_version.assessment_id=NEW.assessment_id
      AND source_version.id=NEW.source_assessment_version_id
      AND source_item.id=NEW.source_assessment_item_id
      AND source_item.question_version_id=NEW.question_version_id
      AND source_item.rubric_version_id=NEW.source_rubric_version_id
      AND component.stable_id=NEW.rubric_point_stable_id
      AND trim(component.evidence_text)=trim(NEW.evidence_text)
  ) THEN RAISE(ABORT,'M2_RUBRIC_EVIDENCE_SOURCE_SCOPE_MISMATCH') END;
END;

CREATE TRIGGER trg_exam_rubric_evidence_promotion_adoption_insert_v2
BEFORE INSERT ON exam_rubric_evidence_promotions_v2
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_assessment_versions_v2 base_version
    JOIN exam_assessment_items_v2 base_item
      ON base_item.id=NEW.base_assessment_item_id
     AND base_item.assessment_version_id=base_version.id
    JOIN k1_rubric_versions base_rubric
      ON base_rubric.id=NEW.base_rubric_version_id
     AND base_rubric.id=base_item.rubric_version_id
    JOIN k1_link_sets base_links
      ON base_links.id=NEW.base_link_set_id
     AND base_links.id=base_item.link_set_id
    JOIN exam_assessment_versions_v2 adopted_version
      ON adopted_version.id=NEW.adopted_assessment_version_id
     AND adopted_version.assessment_id=base_version.assessment_id
     AND adopted_version.supersedes_version_id=base_version.id
    JOIN exam_assessment_items_v2 adopted_item
      ON adopted_item.id=NEW.adopted_assessment_item_id
     AND adopted_item.assessment_version_id=adopted_version.id
    JOIN k1_rubric_versions adopted_rubric
      ON adopted_rubric.id=NEW.adopted_rubric_version_id
     AND adopted_rubric.id=adopted_item.rubric_version_id
     AND adopted_rubric.supersedes_rubric_id=base_rubric.id
    JOIN k1_link_sets adopted_links
      ON adopted_links.id=NEW.adopted_link_set_id
     AND adopted_links.id=adopted_item.link_set_id
     AND adopted_links.supersedes_link_set_id=base_links.id
    WHERE base_version.id=NEW.base_assessment_version_id
      AND base_version.assessment_id=NEW.assessment_id
      AND base_item.question_version_id=NEW.question_version_id
      AND adopted_item.question_version_id=NEW.question_version_id
      AND adopted_rubric.question_version_id=NEW.question_version_id
      AND adopted_links.question_version_id=NEW.question_version_id
  ) THEN RAISE(ABORT,'M2_RUBRIC_EVIDENCE_ADOPTION_SCOPE_MISMATCH') END;
END;

CREATE TRIGGER trg_exam_rubric_evidence_promotion_update_v2
BEFORE UPDATE ON exam_rubric_evidence_promotions_v2
BEGIN SELECT RAISE(ABORT,'M2_RUBRIC_EVIDENCE_PROMOTION_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_rubric_evidence_promotion_delete_v2
BEFORE DELETE ON exam_rubric_evidence_promotions_v2
BEGIN SELECT RAISE(ABORT,'M2_RUBRIC_EVIDENCE_PROMOTION_IMMUTABLE'); END;
