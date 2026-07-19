"""K1-5 题目实际表现与版本变更影响浏览器冒烟测试。

验证页面只展示已发布的老师确认统计，老师可查看新版对历史作业、学习证据和
图谱快照的影响；确认时先建立复核计划，再冻结逐份待处理证据。老师保存新
评分后旧正式成绩保持不变，只有再次点击明确发布才切换正式结果。
"""

from pathlib import Path

from playwright.sync_api import expect, sync_playwright


OUTPUT_DIR = Path(
    "/Users/zzx/agent-shared/笔记/教辅系统/验收/2026-07-19-k1-performance-impact"
)

MOCK_SCRIPT = r"""
window.__performanceCalls = [];
window.__impactCases = [];
window.__TAURI_INTERNALS__ = {
  transformCallback: () => 1,
  unregisterCallback: () => undefined,
  convertFileSrc: (path) => path,
  invoke: async (cmd, args = {}) => {
    window.__performanceCalls.push({ cmd, args });
    if (cmd === "classes_list") {
      return [{ id: 1, name: "八年级一班", term: "2026秋", textbook: "中国历史八上" }];
    }
    if (cmd === "list_profile_scope_options") return [];
    if (cmd === "class_operations_dashboard") {
      return {
        meta: {
          schema_version: 2, rule_version: "m6.1-operations-v2",
          calculated_at: "2026-07-19T21:00:00Z", as_of_date: args.asOfDate,
          recitation_watermark: null, exam_watermark: null
        },
        class: {
          id: 1, name: "八年级一班", term: "2026秋",
          textbook: "中国历史八上", enabled_student_count: 0
        },
        recitation: {
          expected_student_count: 0, completed_student_count: 0,
          expected_task_count: 0, confirmed_task_count: 0, submitted_task_count: 0,
          not_submitted_student_count: 0, pending_teacher_review_count: 0,
          overdue_pending_review_count: 0, recognition_failure_count: 0,
          recognition_processing_count: 0, denominator_note: "暂无任务。"
        },
        exam: {
          active_assessment_count: 0, expected_submission_count: 0,
          submitted_submission_count: 0, missing_submission_count: 0,
          ingesting_attempt_count: 0, grading_attempt_count: 0,
          ready_to_publish_attempt_count: 0, published_submission_count: 0,
          open_pipeline_issue_count: 0, denominator_note: "暂无作业。"
        },
        students: [], actions: []
      };
    }
    if (cmd === "latest_class_profile") return null;
    if (cmd === "list_class_teaching_events"
        || cmd === "list_class_teaching_inputs"
        || cmd === "list_class_action_drafts") return [];
    if (cmd === "k1_blueprint_options") {
      return {
        classes: [{ id: 1, name: "八年级一班", term: "2026秋" }],
        knowledge_maps: [{ public_id: "map-1", title: "中国历史八年级上册", revision: 1 }],
        curriculum_nodes: [],
        knowledge_nodes: [],
        question_types: ["single", "multiple", "true_false", "fill_blank", "short_answer"]
      };
    }
    if (cmd === "k1_blueprint_list"
        || cmd === "k1_source_inbox"
        || cmd === "k1_answer_targets"
        || cmd === "k1_answer_inbox"
        || cmd === "k1_link_review_inbox") return [];
    if (cmd === "k1_link_review_catalog") return { maps: [] };
    if (cmd === "k1_question_performance") {
      return {
        schemaVersion: 1,
        ruleVersion: "k1-current-publication-performance-v1",
        calculatedAt: "2026-07-19T21:00:00Z",
        boundaryNote: "只读取当前有效发布中老师确认的评分；订正单独计数。",
        items: [{
          questionVersionPublicId: "question-version-1",
          revision: 1,
          ownerScope: "personal",
          questionType: "single",
          stem: "鸦片战争爆发于哪一年？",
          maxScore: 2,
          qualityLevel: "L3",
          state: "published",
          assessmentUsageCount: 2,
          publishedResponseCount: 40,
          fullCreditCount: 28,
          partialCreditCount: 4,
          zeroScoreCount: 8,
          averageScoreRate: 0.75,
          fullCreditRate: 0.7,
          firstAttemptCount: 36,
          correctionAttemptCount: 4,
          latestPublishedAt: "2026-07-19T20:00:00Z",
          contextBreakdown: [{
            assessmentContext: "quiz",
            publishedResponseCount: 40,
            averageScoreRate: 0.75,
            fullCreditRate: 0.7
          }],
          hasVersionUpdateImpact: true
        }]
      };
    }
    if (cmd === "k1_question_impact_preview") {
      return {
        schemaVersion: 1,
        ruleVersion: "k1-version-impact-plan-v1",
        calculatedAt: "2026-07-19T21:01:00Z",
        previewHash: "a".repeat(64),
        questionVersionPublicId: "question-version-1",
        questionType: "single",
        stem: "鸦片战争爆发于哪一年？",
        target: {
          answerKeyVersionPublicId: "answer-v2", answerKeyRevision: 2,
          rubricVersionPublicId: "rubric-v2", rubricRevision: 2,
          linkSetPublicId: "links-v2", linkSetRevision: 2
        },
        affectedAssessmentCount: 2,
        affectedAssessmentVersionCount: 2,
        affectedItemCount: 2,
        unpublishedAttemptCount: 3,
        publishedAttemptCount: 2,
        activeLearningEvidenceCount: 2,
        profileSnapshotCount: 12,
        rows: [{
          assessmentPublicId: "assessment-1",
          assessmentTitle: "第一单元检测",
          className: "八年级一班",
          assessmentVersionPublicId: "assessment-version-1",
          assessmentItemPublicId: "assessment-item-1",
          sourceAnswerKeyVersionPublicId: "answer-v1",
          sourceRubricVersionPublicId: "rubric-v1",
          sourceLinkSetPublicId: "links-v1",
          answerChanged: true,
          rubricChanged: true,
          linkChanged: true,
          unpublishedAttemptCount: 3,
          publishedAttemptCount: 2,
          activeLearningEvidenceCount: 2,
          profileSnapshotCount: 12
        }],
        boundaryNote: "预览只识别影响，不改写任何历史事实。"
      };
    }
    if (cmd === "k1_question_impact_confirm") {
      return {
        publicId: "impact-plan-1",
        questionVersionPublicId: "question-version-1",
        expectedPreviewHash: "a".repeat(64),
        action: args.input.action,
        taskCount: args.input.action === "review_published" ? 2 : 0,
        plannedBy: "local_teacher",
        plannedAt: "2026-07-19T21:02:00Z",
        changesAssessmentBinding: false,
        changesGrade: false,
        changesPublication: false,
        changesLearningEvidence: false
      };
    }
    if (cmd === "k1_question_impact_cases") {
      return {
        schemaVersion: 1,
        ruleVersion: "k1-version-impact-review-case-v1",
        planPublicId: args.planPublicId,
        cases: window.__impactCases,
        boundaryNote: "保存新评分不会自动发布；明确发布后才切换正式成绩和学习证据。"
      };
    }
    if (cmd === "k1_question_impact_prepare") {
      const students = [
        ["01", "学生甲", "decision-1", 2],
        ["02", "学生乙", "decision-2", 1]
      ];
      window.__impactCases = students.map(([studentNo, studentName, decisionId, score], index) => ({
        publicId: `review-case-${index + 1}`,
        impactTaskPublicId: `impact-task-${index + 1}`,
        planPublicId: args.input.planPublicId,
        caseKind: "published_review",
        assessmentTitle: "第一单元检测",
        className: "八年级一班",
        studentNo,
        studentName,
        questionVersionPublicId: "question-version-1",
        questionType: "single",
        questionStem: "鸦片战争爆发于哪一年？",
        questionNo: 1,
        maxScore: 2,
        attemptPublicId: `attempt-${index + 1}`,
        attemptState: "published",
        publicationPublicId: "publication-1",
        sourceGradeDecisionPublicId: decisionId,
        sourceGradeDecisionRevision: 1,
        sourceTeacherScore: score,
        sourcePointResultsJson: "{\"schema_version\":1}",
        sourceSnapshotHash: "b".repeat(64),
        studentResponseState: "recognized",
        studentResponseText: index === 0
          ? "{\"selected_labels\":[\"A\"]}"
          : "{\"selected_labels\":[\"B\"]}",
        cropPath: null,
        targetAnswerJson: "{\"schema_version\":1,\"correct_labels\":[\"B\"]}",
        targetComponents: [],
        targetAnswerKeyVersionPublicId: "answer-v2",
        targetAnswerKeyRevision: 2,
        targetRubricVersionPublicId: "rubric-v2",
        targetRubricRevision: 2,
        targetLinkSetPublicId: "links-v2",
        targetLinkSetRevision: 2,
        preparedBy: "local_teacher",
        preparedAt: "2026-07-19T21:03:00Z",
        state: "open",
        resolutionPublicId: null,
        resolvedGradeDecisionPublicId: null,
        resolvedTeacherScore: null,
        resolvedAt: null,
        activePublicationPublicId: "publication-1",
        nextStepNote: "保存新评分后旧正式成绩仍保持不变，直到明确生成新的发布 revision。",
        changesAssessmentBinding: false,
        changesGrade: false,
        changesPublication: false,
        changesLearningEvidence: false
      }));
      return {
        planPublicId: args.input.planPublicId,
        taskCount: 2,
        createdCount: 2,
        existingCount: 0,
        cases: window.__impactCases,
        changesAssessmentBinding: false,
        changesGrade: false,
        changesPublication: false,
        changesLearningEvidence: false
      };
    }
    if (cmd === "k1_question_impact_resolve") {
      const reviewCase = window.__impactCases.find(
        (item) => item.publicId === args.input.casePublicId
      );
      reviewCase.state = "grade_confirmed";
      reviewCase.attemptState = "ready_to_publish";
      reviewCase.resolutionPublicId = "resolution-1";
      reviewCase.resolvedGradeDecisionPublicId = "decision-new-1";
      reviewCase.resolvedTeacherScore = args.input.teacherScore;
      reviewCase.resolvedAt = "2026-07-19T21:04:00Z";
      reviewCase.changesGrade = true;
      reviewCase.nextStepNote = "新评分 revision 已确认但尚未正式生效；请明确发布整份成绩。";
      return {
        casePublicId: reviewCase.publicId,
        resolutionPublicId: reviewCase.resolutionPublicId,
        gradeDecision: {
          public_id: reviewCase.resolvedGradeDecisionPublicId,
          revision: 2,
          teacher_score: reviewCase.resolvedTeacherScore,
          point_results_json: "{\"schema_version\":3}",
          state: "active"
        },
        oldPublicationUnchanged: true,
        learningEvidenceUnchanged: true,
        requiresExplicitPublication: true
      };
    }
    if (cmd === "k1_question_impact_publish") {
      const reviewCase = window.__impactCases.find(
        (item) => item.publicId === args.input.casePublicId
      );
      reviewCase.state = "republished";
      reviewCase.attemptState = "published";
      reviewCase.activePublicationPublicId = "publication-2";
      reviewCase.changesPublication = true;
      reviewCase.changesLearningEvidence = true;
      reviewCase.nextStepNote = "新评分已由老师再次明确发布；旧发布快照保持审计。";
      return {
        casePublicId: reviewCase.publicId,
        publication: {
          public_id: "publication-2",
          revision: 2,
          state: "published",
          total_score: 0
        },
        priorPublicationSuperseded: true,
        learningEvidenceSwitched: true
      };
    }
    return [];
  }
};
"""


def test_k1_question_performance(base_url: str) -> None:
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1500, "height": 1150})
        page.add_init_script(MOCK_SCRIPT)
        page.goto(base_url)
        page.wait_for_load_state("networkidle")

        page.locator(".mod-row").filter(has_text="改作业").click()
        page.get_by_role("button", name="题目与题库").click()
        page.get_by_role("button", name="表现与版本影响").click()

        expect(page.get_by_text("平均得分率", exact=True)).to_be_visible()
        expect(page.get_by_text("75%", exact=True).first).to_be_visible()
        expect(page.get_by_text("满分 28", exact=True)).to_be_visible()
        expect(page.get_by_text("随堂测验 40 份 · 75%", exact=True)).to_be_visible()
        page.screenshot(path=str(OUTPUT_DIR / "01-performance.png"), full_page=True)

        page.get_by_role("button", name="查看新版影响").click()
        expect(page.get_by_text("版本变更影响预览", exact=True)).to_be_visible()
        expect(page.get_by_text("已发布作答", exact=True).last).to_be_visible()
        expect(page.get_by_text("证据 2", exact=True)).to_be_visible()
        expect(page.get_by_text("图谱 12", exact=True)).to_be_visible()
        expect(page.get_by_text("不会切换作业版本", exact=False)).to_be_visible()
        page.screenshot(path=str(OUTPUT_DIR / "02-impact-preview.png"), full_page=True)

        page.get_by_text("复核已发布成绩", exact=True).click()
        page.get_by_role("button", name="确认处理方式").click()
        expect(page.get_by_text("已冻结处理计划，共 2 条待办", exact=False)).to_be_visible()
        page.screenshot(path=str(OUTPUT_DIR / "03-impact-plan.png"), full_page=True)

        page.get_by_test_id("impact-prepare-cases").click()
        expect(page.get_by_text("待处理记录 2 条", exact=True)).to_be_visible()
        expect(page.get_by_text("01 · 学生甲", exact=True)).to_be_visible()
        expect(page.get_by_text("已发布复核", exact=True).first).to_be_visible()
        expect(page.get_by_text("旧正式成绩仍保持不变", exact=False).first).to_be_visible()
        page.screenshot(path=str(OUTPUT_DIR / "04-impact-review-cases.png"), full_page=True)

        first_case = page.locator(".impact-review-card").first
        first_case.get_by_label("学生甲 新评分").fill("0")
        first_case.get_by_text("本次复核说明（必填）", exact=True).locator("..").get_by_role("textbox").fill(
            "按修订后的答案核对"
        )
        first_case.get_by_test_id("impact-resolve-case").click()
        expect(page.get_by_text("新评分 0 分已保存", exact=False)).to_be_visible()
        expect(page.get_by_text("旧正式成绩仍然有效", exact=False)).to_be_visible()
        page.screenshot(path=str(OUTPUT_DIR / "05-impact-grade-confirmed.png"), full_page=True)

        page.get_by_test_id("impact-publish-case").click()
        expect(page.get_by_text("新评分已经明确发布", exact=False)).to_be_visible()
        expect(page.get_by_text("正式学习证据已按新版本切换", exact=False)).to_be_visible()
        page.screenshot(path=str(OUTPUT_DIR / "06-impact-republished.png"), full_page=True)

        calls = page.evaluate(
            """window.__performanceCalls.filter((item) =>
              item.cmd === "k1_question_performance"
              || item.cmd === "k1_question_impact_preview"
              || item.cmd === "k1_question_impact_confirm"
              || item.cmd === "k1_question_impact_cases"
              || item.cmd === "k1_question_impact_prepare"
              || item.cmd === "k1_question_impact_resolve"
              || item.cmd === "k1_question_impact_publish")"""
        )
        assert [item["cmd"] for item in calls] == [
            "k1_question_performance",
            "k1_question_impact_preview",
            "k1_question_impact_confirm",
            "k1_question_impact_cases",
            "k1_question_impact_prepare",
            "k1_question_impact_resolve",
            "k1_question_impact_cases",
            "k1_question_impact_publish",
            "k1_question_impact_cases",
            "k1_question_performance",
        ]
        payload = calls[2]["args"]["input"]
        assert payload["action"] == "review_published"
        assert payload["plannedBy"] == "local_teacher"
        assert payload["expectedPreviewHash"] == "a" * 64
        prepare_payload = calls[4]["args"]["input"]
        assert prepare_payload["planPublicId"] == "impact-plan-1"
        assert prepare_payload["expectedTaskCount"] == 2
        assert prepare_payload["preparedBy"] == "local_teacher"
        resolve_payload = calls[5]["args"]["input"]
        assert resolve_payload["teacherScore"] == 0
        assert resolve_payload["components"] == []
        assert resolve_payload["teacherNote"] == "按修订后的答案核对"
        assert resolve_payload["resolvedBy"] == "local_teacher"
        publish_payload = calls[7]["args"]["input"]
        assert publish_payload["expectedGradeDecisionPublicId"] == "decision-new-1"
        assert publish_payload["publishedBy"] == "local_teacher"
        browser.close()


if __name__ == "__main__":
    test_k1_question_performance("http://127.0.0.1:4173")
    print("K1 question performance UI smoke: PASS")
