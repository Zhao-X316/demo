"""M2.5-3a 批改中简答评分点更新建议浏览器冒烟测试。"""

from pathlib import Path

from playwright.sync_api import expect, sync_playwright


OUTPUT_DIR = Path(
    "/Users/zzx/agent-shared/笔记/教辅系统/验收/2026-07-19-m25-question-updates"
)

MOCK_SCRIPT = r"""
window.__rubricCalls = [];
window.__rubricPromoted = false;
const emptyOperations = {
  meta: {
    schema_version: 2, rule_version: "m6.1-operations-v2",
    calculated_at: "2026-07-19T23:00:00Z", as_of_date: "2026-07-19",
    recitation_watermark: null, exam_watermark: null
  },
  class: {
    id: 1, name: "八年级一班", term: "2026秋",
    textbook: "中国历史八上", enabled_student_count: 1
  },
  recitation: {
    expected_student_count: 0, completed_student_count: 0,
    expected_task_count: 0, confirmed_task_count: 0, submitted_task_count: 0,
    not_submitted_student_count: 0, pending_teacher_review_count: 0,
    overdue_pending_review_count: 0, recognition_failure_count: 0,
    recognition_processing_count: 0, denominator_note: "暂无任务。"
  },
  exam: {
    active_assessment_count: 1, expected_submission_count: 1,
    submitted_submission_count: 1, missing_submission_count: 0,
    ingesting_attempt_count: 0, grading_attempt_count: 1,
    ready_to_publish_attempt_count: 0, published_submission_count: 0,
    open_pipeline_issue_count: 0, denominator_note: "按当前作业计算。"
  },
  students: [], actions: []
};
function subjectiveWorkbench() {
  return {
    rows: [{
      assessment_id: 1,
      assessment_version_id: 1,
      assessment_title: "洋务运动随堂练习",
      attempt_id: 1,
      attempt_state: "grading",
      active_publication_id: null,
      student_id: 1,
      student_no: "01",
      student_name: "小林",
      assessment_item_id: 2,
      order_index: 1,
      question_no: "2",
      question_type: "short_answer",
      question_stem: "概括洋务运动失败的原因",
      max_score: 4,
      answer_region_revision_id: 7,
      crop_path: null,
      transcription_revision_id: 8,
      transcription_revision: 1,
      result_state: "recognized",
      raw_ocr_text: "只学习技术，没有改变封建制度",
      normalized_text: "只学习技术，没有改变封建制度",
      teacher_corrected_text: null,
      confidence: 0.96,
      answer_json: JSON.stringify({
        schema_version: 1,
        reference_answer: "没有改变封建制度"
      }),
      answer_slots_json: JSON.stringify({ schema_version: 1, answer_slots: [] }),
      rubric_points_json: JSON.stringify({
        schema_version: 1,
        rubric_points: [{
          source_public_id: "rubric-point-short",
          stable_id: "institution",
          order_index: 0,
          canonical_text: "没有改变封建制度",
          max_score: 4
        }]
      }),
      suggestion_id: 9,
      short_answer_analysis_id: 10,
      machine_grade_ai_run_id: 11,
      suggestion_outcome: "partial",
      suggested_score: 2,
      suggestion_result_json: JSON.stringify({
        schema_version: 1,
        point_results: [{
          stable_id: "institution",
          status: "partial",
          suggested_score: 2,
          evidence_snippets: ["只学习技术"],
          reason: "机器认为表述不完整"
        }]
      }),
      batch_eligible: false,
      exclusion_reason: "TEACHER_REVIEW_REQUIRED",
      grade_decision_id: 12,
      grade_decision_revision: 1,
      teacher_score: 4,
      confirmation_level: "teacher_corrected",
      review_mode: "teacher_corrected",
      current_suggestion_confirmed: true,
      decided_at: "2026-07-19T23:00:00Z",
      teacher_components_json: JSON.stringify({
        schema_version: 1,
        component_results: [{
          source_type: "rubric_point",
          source_public_id: "rubric-point-short",
          stable_id: "institution",
          order_index: 0,
          teacher_score: 4,
          max_score: 4,
          result_status: "correct",
          evidence_text: "只学习技术",
          teacher_note: "属于制度局限的合理表述"
        }]
      }),
      accepted_answer_promotion_id: null,
      accepted_answer_promoted_at: null,
      rubric_evidence_promotions_json: JSON.stringify({
        schema_version: 1,
        promotions: window.__rubricPromoted ? [{
          promotion_id: 21,
          source_public_id: "rubric-point-short",
          rubric_point_stable_id: "institution",
          evidence_text: "只学习技术",
          adopted_assessment_version_id: 2,
          adopted_rubric_version_id: 3,
          adopted_link_set_id: 3,
          created_at: "2026-07-19T23:01:00Z"
        }] : []
      })
    }],
    attempts: []
  };
}
window.__TAURI_INTERNALS__ = {
  transformCallback: () => 1,
  unregisterCallback: () => undefined,
  convertFileSrc: (path) => path,
  invoke: async (cmd, args = {}) => {
    window.__rubricCalls.push({ cmd, args });
    if (cmd === "classes_list") {
      return [{ id: 1, name: "八年级一班", term: "2026秋", textbook: "中国历史八上" }];
    }
    if (cmd === "class_operations_dashboard") return emptyOperations;
    if (cmd === "latest_class_profile") return null;
    if (cmd === "list_profile_scope_options"
        || cmd === "list_class_teaching_events"
        || cmd === "list_class_teaching_inputs"
        || cmd === "list_class_action_drafts"
        || cmd === "students_list"
        || cmd === "questions_list"
        || cmd === "kp_list"
        || cmd === "exam_answers_list"
        || cmd === "exam_fixed_intake_options") return [];
    if (cmd === "exam_objective_workbench"
        || cmd === "exam_dictation_workbench") return { rows: [], attempts: [] };
    if (cmd === "exam_answer_sheet_subjective_workbench") return subjectiveWorkbench();
    if (cmd === "exam_subjective_link_editor") {
      return {
        assessment_item_id: 2,
        question_version_id: 2,
        question_type: "short_answer",
        question_stem: "概括洋务运动失败的原因",
        current_assessment_version_id: 1,
        current_assessment_revision: 1,
        current_link_set_id: 2,
        current_link_set_revision: 1,
        knowledge_map_public_id: "map-1",
        knowledge_map_title: "中国历史八上",
        sources: [],
        knowledge_options: [],
        ability_options: []
      };
    }
    if (cmd === "exam_answer_sheet_promote_rubric_evidence") {
      window.__rubricPromoted = true;
      return {
        outcome: "created_new_version",
        promotion_id: 21,
        rubric_point_stable_id: "institution",
        evidence_text: "只学习技术",
        adopted_assessment_version_id: 2,
        adopted_assessment_revision: 2,
        adopted_rubric_version_id: 3,
        adopted_link_set_id: 3,
        carried_knowledge_link_count: 1,
        carried_ability_link_count: 1,
        current_grade_unchanged: true,
        current_publication_unchanged: true
      };
    }
    return [];
  }
};
"""


def test_rubric_update(base_url: str) -> None:
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1500, "height": 1100})
        page.add_init_script(MOCK_SCRIPT)
        page.goto(base_url)
        page.wait_for_load_state("networkidle")

        page.locator(".mod-row").filter(has_text="改作业").click()
        page.get_by_role("button", name="题目批改").click()
        page.get_by_role("button", name="答题卡主观题").click()

        expect(page.get_by_text("老师逐项确认 · 4 / 4 分", exact=True)).to_be_visible()
        expect(page.get_by_text("只学习技术", exact=True).last).to_be_visible()
        page.screenshot(path=str(OUTPUT_DIR / "01-rubric-update-suggestion.png"), full_page=True)

        page.once("dialog", lambda dialog: dialog.accept())
        page.get_by_role("button", name="加入未来评分点示例").click()
        expect(page.get_by_text("已创建新评分规则：只学习技术", exact=False)).to_be_visible()
        expect(page.get_by_text("已加入未来评分规则", exact=True)).to_be_visible()
        expect(page.get_by_text("已沉淀 1 条老师确认表述", exact=False)).to_be_visible()
        page.screenshot(path=str(OUTPUT_DIR / "02-rubric-update-confirmed.png"), full_page=True)

        calls = page.evaluate(
            """window.__rubricCalls.filter((item) =>
              item.cmd === "exam_answer_sheet_promote_rubric_evidence")"""
        )
        assert len(calls) == 1
        assert calls[0]["args"] == {
            "gradeDecisionId": 12,
            "sourcePublicId": "rubric-point-short",
        }
        browser.close()


if __name__ == "__main__":
    test_rubric_update("http://127.0.0.1:4173")
    print("M2.5 rubric update UI smoke: PASS")
