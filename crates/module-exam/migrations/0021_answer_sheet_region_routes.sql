-- M2-B3a2：答题卡题区识别路由。
-- 客观格和主观作答区必须在模板物化时显式分流；主观区不得误入 OMR。

CREATE TABLE exam_answer_sheet_region_routes_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  materialization_id INTEGER NOT NULL
    REFERENCES exam_answer_sheet_page_materializations_v2(id),
  answer_region_revision_id INTEGER NOT NULL UNIQUE
    REFERENCES exam_answer_region_revisions_v2(id),
  assessment_item_id INTEGER NOT NULL REFERENCES exam_assessment_items_v2(id),
  region_index INTEGER NOT NULL CHECK(region_index >= 0),
  recognition_route TEXT NOT NULL CHECK(recognition_route IN (
    'objective_omr','handwriting_ocr'
  )),
  question_type TEXT NOT NULL CHECK(question_type IN (
    'single','multiple','true_false','fill_blank','short_answer'
  )),
  created_at TEXT NOT NULL,
  UNIQUE(materialization_id,assessment_item_id,region_index),
  CHECK((recognition_route='objective_omr' AND question_type IN (
           'single','multiple','true_false'
         ))
        OR (recognition_route='handwriting_ocr' AND question_type IN (
           'fill_blank','short_answer'
         )))
);

-- 既有 0015 物化记录全部来自旧版纯客观模板，可确定性回填为 objective_omr。
INSERT INTO exam_answer_sheet_region_routes_v2
  (materialization_id,answer_region_revision_id,assessment_item_id,region_index,
   recognition_route,question_type,created_at)
SELECT m.id,r.id,r.assessment_item_id,r.region_index,'objective_omr',
       json_extract(template_item.value,'$.question_type'),m.created_at
FROM exam_answer_sheet_page_materializations_v2 m
JOIN json_each(m.region_revision_ids_json,'$.region_revision_ids') ids ON 1=1
JOIN exam_answer_region_revisions_v2 r ON r.id=CAST(ids.value AS INTEGER)
JOIN exam_answer_sheet_template_revisions_v2 t ON t.id=m.template_revision_id
JOIN json_each(t.template_json,'$.items') template_item
  ON CAST(json_extract(template_item.value,'$.assessment_item_id') AS INTEGER)
       =r.assessment_item_id
 AND CAST(json_extract(template_item.value,'$.region_index') AS INTEGER)
       =r.region_index
WHERE json_extract(template_item.value,'$.question_type')
      IN ('single','multiple','true_false');

CREATE INDEX idx_exam_answer_sheet_region_route_materialization_v2
  ON exam_answer_sheet_region_routes_v2(materialization_id,id);

CREATE TRIGGER trg_exam_answer_sheet_region_route_update_v2
BEFORE UPDATE ON exam_answer_sheet_region_routes_v2
BEGIN SELECT RAISE(ABORT,'M2_ANSWER_SHEET_REGION_ROUTE_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_answer_sheet_region_route_delete_v2
BEFORE DELETE ON exam_answer_sheet_region_routes_v2
BEGIN SELECT RAISE(ABORT,'M2_ANSWER_SHEET_REGION_ROUTE_IMMUTABLE'); END;
