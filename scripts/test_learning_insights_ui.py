"""M3 错题事实/老师确认错因与 M6 掌握入口浏览器冒烟测试。"""

from playwright.sync_api import expect, sync_playwright


MOCK_SCRIPT = r"""
window.__learningCalls = [];
window.__teacherAssessmentRevision = 0;
window.__TAURI_INTERNALS__ = {
  transformCallback: () => 1,
  unregisterCallback: () => undefined,
  convertFileSrc: (path) => path,
  invoke: async (cmd, args = {}) => {
    window.__learningCalls.push({ cmd, args });
    if (cmd === "plugin:dialog|save") {
      return "/tmp/八年级一班_错题事实汇总.csv";
    }
    if (cmd === "classes_list") {
      return window.__NO_CLASSES__ ? [] : [
        { id: 1, name: "八年级一班", term: "2026秋", textbook: "中国历史八上" },
        { id: 2, name: "八年级二班", term: "2026秋", textbook: "中国历史八上" }
      ];
    }
    if (cmd === "students_list") {
      return [
        { id: 1, student_no: "01", name: "小林", class_id: 1, enabled: true },
        { id: 2, student_no: "02", name: "小周", class_id: 1, enabled: true },
        { id: 3, student_no: "03", name: "小许", class_id: 2, enabled: true }
      ];
    }
    if (cmd === "preview_student_profile") {
      const input = args.input;
      return {
        schema_version: 3, rule_version: "m6-confirmed-evidence-profile-v3",
        calculated_at: "2026-07-17T03:00:00Z",
        student: input.studentId === 2
          ? { id: 2, class_id: 1, student_no: "02", name: "小周" }
          : { id: 1, class_id: 1, student_no: "01", name: "小林" },
        range_start: input.rangeStart, range_end: input.rangeEnd,
        policy: {
          public_id: "profile-policy-1", revision: 1,
          min_independent_groups: 3, min_distinct_dates: 2,
          min_distinct_sources: 2, needs_support_below: 0.6,
          stable_at_or_above: 0.85, freshness_days: 30
        },
        counts: {
          mapped_formal_evidence: 4, knowledge_node_total: 3,
          knowledge_node_assessed: 2, knowledge_node_eligible: 1,
          ability_node_total: 2, ability_node_assessed: 1,
          ability_node_eligible: 0, machine_only_excluded: 2,
          teacher_overall_excluded: 1, unmapped_formal_excluded: 1,
          unsupported_contract_excluded: 1,
          referenced_knowledge_map_count: 1
        },
        recitation_summary: {
          overall_count: 1, fluency_count: 1, retention_count: 1,
          latest_overall_value: 1, latest_fluency_value: 0.82,
          latest_retention_value: 0.76, latest_at: "2026-07-16T07:30:00Z",
          evidence: []
        },
        wrongbook_summary: {
          fact_count: 1, needs_correction_count: 1, corrected_once_count: 0,
          rechecked_correct_count: 0, repeated_error_count: 1,
          latest_response_at: "2026-07-16T08:00:00Z",
          note: "错题恢复事实不直接等同掌握。",
          facts: [{
            question_version_public_id: "qv-1", question_type: "single",
            stem: "洋务运动失败的根本原因是？", status: "needs_correction",
            first_error_at: "2026-07-14T08:00:00Z",
            last_error_at: "2026-07-16T08:00:00Z",
            latest_response_at: "2026-07-16T08:00:00Z",
            published_response_count: 2, error_response_count: 2,
            repeated_error: true, correction_status: null, reinforcement_status: null,
            knowledge_nodes: [{ public_id: "k-1", title: "洋务运动失败原因" }],
            ability_dimensions: [{ public_id: "a-1", title: "因果分析" }]
          }]
        },
        source_watermark: "profile-watermark-1", can_generate: true, blocker: null,
        scope_note: "范围来自当前正式证据引用的已确认知识地图。",
        evidence_note: "只纳入老师接受或修正的正式逐点证据。"
      };
    }
    if (cmd === "generate_student_profile") {
      const input = args.input;
      window.__generatedProfile = {
        public_id: "profile-snapshot-1", revision: 1,
        student: { id: input.studentId, class_id: input.classId,
          student_no: input.studentId === 2 ? "02" : "01",
          name: input.studentId === 2 ? "小周" : "小林" },
        range_start: input.rangeStart, range_end: input.rangeEnd,
        scope_kind: "confirmed_evidence_maps",
        evidence_cutoff_at: "2026-07-17T03:00:00Z",
        policy: {
          public_id: "profile-policy-1", revision: 1,
          min_independent_groups: 3, min_distinct_dates: 2,
          min_distinct_sources: 2, needs_support_below: 0.6,
          stable_at_or_above: 0.85, freshness_days: 30
        },
        source_watermark: "profile-watermark-1", evidence_count: 4,
        knowledge_node_total: 3, knowledge_node_assessed: 2,
        knowledge_node_eligible: 1, ability_node_total: 2,
        ability_node_assessed: 1, ability_node_eligible: 0,
        state: "teacher_confirmed", payload_sha256: "c".repeat(64),
        generated_by: "local_teacher", generated_at: "2026-07-17T03:01:00Z",
        confirmed_by: "local_teacher", confirmed_at: "2026-07-17T03:01:00Z",
        is_stale: false, stale_reason: null,
        recitation_summary: {
          overall_count: 1, fluency_count: 1, retention_count: 1,
          latest_overall_value: 1, latest_fluency_value: 0.82,
          latest_retention_value: 0.76, latest_at: "2026-07-16T07:30:00Z",
          evidence: []
        },
        wrongbook_summary: {
          fact_count: 1, needs_correction_count: 1, corrected_once_count: 0,
          rechecked_correct_count: 0, repeated_error_count: 1,
          latest_response_at: "2026-07-16T08:00:00Z",
          note: "错题恢复事实不直接等同掌握。",
          facts: [{
            question_version_public_id: "qv-1", question_type: "single",
            stem: "洋务运动失败的根本原因是？", status: "needs_correction",
            first_error_at: "2026-07-14T08:00:00Z",
            last_error_at: "2026-07-16T08:00:00Z",
            latest_response_at: "2026-07-16T08:00:00Z",
            published_response_count: 2, error_response_count: 2,
            repeated_error: true, correction_status: null, reinforcement_status: null,
            knowledge_nodes: [{ public_id: "k-1", title: "洋务运动失败原因" }],
            ability_dimensions: [{ public_id: "a-1", title: "因果分析" }]
          }]
        },
        trend: {
          comparison_status: "comparable", comparison_kind: "same_scope_refresh",
          previous_snapshot_public_id: "profile-snapshot-0", previous_revision: 0,
          previous_generated_at: "2026-07-16T03:01:00Z",
          knowledge_assessed_before: 1, knowledge_assessed_current: 2,
          knowledge_assessed_delta: 1, ability_assessed_before: 0,
          ability_assessed_current: 1, ability_assessed_delta: 1,
          needs_support_before: 1, needs_support_current: 0,
          needs_support_delta: -1, stable_before: 0, stable_current: 1,
          stable_delta: 1,
          changed_nodes: [{
            target_type: "knowledge_node", target_public_id: "k-1",
            target_title: "洋务运动失败原因", previous_status: "developing",
            current_status: "stable", previous_mastery_score: 0.72,
            current_mastery_score: 0.88, mastery_score_delta: 0.16
          }],
          note: "只比较同范围、同策略和同证据契约的快照刷新。"
        },
        teacher_assessments: [],
        knowledge_metrics: [
          {
            public_id: "profile-metric-1", target_type: "knowledge_node",
            target_public_id: "k-1", target_title: "洋务运动失败原因",
            mastery_score: 0.88, status: "stable", confidence_level: "medium",
            freshness: "fresh", evidence_count: 3, independent_group_count: 3,
            distinct_date_count: 3, distinct_source_count: 3,
            last_evidence_at: "2026-07-16T08:00:00Z",
            source_breakdown: { grading: 3 },
            explanation: "多次跨日期正式证据显示当前表现较稳定。",
            evidence: [{
              public_id: "learning-evidence-1", source_module: "grading",
              source_type: "question_rubric_point", source_ref_type: "question",
              source_ref_id: "q-1", decision_ref_type: "grade_decision",
              decision_ref_id: "decision-1", decision_revision: 1,
              evidence_kind: "accuracy", value: 1, evidence_quality: 0.95,
              assessment_context: "closed_book",
              confirmation_level: "teacher_accepted",
              occurred_at: "2026-07-16T08:00:00Z",
              independence_group_key: "question:q-1:2026-07-16",
              effective_weight: 0.95
            }]
          },
          {
            public_id: "profile-metric-2", target_type: "knowledge_node",
            target_public_id: "k-2", target_title: "辛亥革命局限",
            mastery_score: 0.5, status: "insufficient_evidence", confidence_level: "low",
            freshness: "fresh", evidence_count: 1, independent_group_count: 1,
            distinct_date_count: 1, distinct_source_count: 1,
            last_evidence_at: "2026-07-15T08:00:00Z",
            source_breakdown: { grading: 1 },
            explanation: "证据尚未达到跨日期与不同来源门槛。",
            evidence: []
          },
          {
            public_id: "profile-metric-3", target_type: "knowledge_node",
            target_public_id: "k-3", target_title: "戊戌变法过程",
            mastery_score: null, status: "unassessed", confidence_level: "none",
            freshness: "none", evidence_count: 0, independent_group_count: 0,
            distinct_date_count: 0, distinct_source_count: 0,
            last_evidence_at: null, source_breakdown: {},
            explanation: "所选范围没有老师确认的逐点证据，不解释为零分或薄弱。",
            evidence: []
          }
        ],
        ability_metrics: [{
          public_id: "profile-metric-4", target_type: "ability_dimension",
          target_public_id: "a-1", target_title: "因果分析",
          mastery_score: 0.7, status: "insufficient_evidence", confidence_level: "low",
          freshness: "fresh", evidence_count: 1, independent_group_count: 1,
          distinct_date_count: 1, distinct_source_count: 1,
          last_evidence_at: "2026-07-16T08:00:00Z",
          source_breakdown: { grading: 1 },
          explanation: "高阶能力证据不足，暂不形成稳定结论。", evidence: []
        }]
      };
      return window.__generatedProfile;
    }
    if (cmd === "latest_student_profile") {
      return window.__generatedProfile ?? null;
    }
    if (cmd === "save_profile_teacher_assessment") {
      const input = args.input;
      window.__teacherAssessmentRevision += 1;
      return {
        public_id: `teacher-assessment-${window.__teacherAssessmentRevision}`,
        snapshot_public_id: input.snapshotPublicId,
        node_metric_public_id: input.nodeMetricPublicId,
        target_type: "knowledge_node", target_public_id: "k-1",
        target_title: "洋务运动失败原因",
        revision: window.__teacherAssessmentRevision,
        assessment: input.assessment,
        note: input.note ?? null,
        state: input.assessment ? "active" : "voided",
        supersedes_public_id: window.__teacherAssessmentRevision > 1
          ? `teacher-assessment-${window.__teacherAssessmentRevision - 1}`
          : null,
        created_by: "local_teacher", created_at: "2026-07-17T03:02:00Z"
      };
    }
    if (cmd === "wrongbook_schedule_policy") {
      return {
        id: 1, public_id: "policy-1", policy_key: "learning_default", revision: 1,
        timezone: "Asia/Shanghai", default_delay_days: 7, daily_limit_per_student: 3,
        weekend_policy: "next_workday", holiday_policy: "next_workday", max_shift_days: 60,
        state: "active", created_by: "system", created_at: "2026-07-16T07:00:00Z",
        holidays: []
      };
    }
    if (cmd === "update_wrongbook_schedule_policy") {
      const input = args.input;
      return {
        id: 2, public_id: "policy-2", policy_key: "learning_default", revision: 2,
        timezone: "Asia/Shanghai", default_delay_days: input.defaultDelayDays,
        daily_limit_per_student: input.dailyLimitPerStudent,
        weekend_policy: input.weekendPolicy, holiday_policy: input.holidayPolicy,
        max_shift_days: input.maxShiftDays, state: "active",
        created_by: "local_teacher", created_at: "2026-07-16T12:02:00Z",
        holidays: input.holidays
      };
    }
    if (cmd === "class_wrongbook_dashboard") {
      const classId = Number(args.classId);
      if (classId === 2) {
        return {
          meta: { schema_version: 4, rule_version: "m3-published-wrong-facts-v4",
            calculated_at: "2026-07-16T12:00:00Z", exam_watermark: null },
          class: { id: 2, name: "八年级二班", term: "2026秋",
            textbook: "中国历史八上", enabled_student_count: 1 },
          summary: {
            affected_student_count: 0, wrong_question_count: 0,
            needs_correction_count: 0, corrected_once_count: 0,
            rechecked_correct_count: 0, repeated_error_count: 0,
            denominator_note: "仅统计本班启用学生当前有效发布快照中的老师评分。"
          },
          items: []
        };
      }
      return {
        meta: { schema_version: 4, rule_version: "m3-published-wrong-facts-v4",
          calculated_at: "2026-07-16T12:00:00Z", exam_watermark: "2026-07-16T11:40:00Z" },
        class: { id: 1, name: "八年级一班", term: "2026秋",
          textbook: "中国历史八上", enabled_student_count: 3 },
        summary: {
          affected_student_count: 2, wrong_question_count: 3,
          needs_correction_count: 1, corrected_once_count: 1,
          rechecked_correct_count: 1, repeated_error_count: 1,
          denominator_note: "仅统计本班启用学生当前有效发布快照中的老师评分；非满分记为错题。订正一次和再次答对都只是题目事实，不等于知识点已掌握。"
        },
        items: [
          {
            student_id: 1, student_no: "01", student_name: "小林",
            question_version_id: "qv-1", question_type: "single",
            stem: "洋务运动失败的根本原因是？", status: "needs_correction",
            first_error_at: "2026-07-14T08:00:00Z", last_error_at: "2026-07-16T08:00:00Z",
            latest_response_at: "2026-07-16T08:00:00Z",
            latest_score: 0, latest_max_score: 2, latest_score_ratio: 0,
            published_response_count: 2, error_response_count: 2, repeated_error: true,
            latest_assessment_title: "近代化单元测验", latest_assessment_context: "quiz",
            latest_error_grade_decision_public_id: "decision-1",
            latest_error_publication_public_id: "publication-1",
            cause_options: [
              { code: "missing_answer", label: "未作答", description: "学生没有写出答案" },
              { code: "fact_error", label: "史实错误", description: "历史事实错误" },
              { code: "concept_confusion", label: "概念混淆", description: "相近概念混淆" },
              { code: "misread_prompt", label: "审题偏差", description: "回答方向偏差" },
              { code: "other", label: "其他", description: "老师补充" }
            ],
            cause_review: null,
            correction_assignment: null,
            reinforcement_assignment: null,
            knowledge_nodes: [{ public_id: "k-1", title: "洋务运动失败原因" }],
            ability_dimensions: [{ public_id: "a-1", title: "因果分析" }]
          },
          {
            student_id: 2, student_no: "02", student_name: "小周",
            question_version_id: "qv-2", question_type: "fill_blank",
            stem: "《南京条约》签订于____年。", status: "corrected_once",
            first_error_at: "2026-07-13T08:00:00Z", last_error_at: "2026-07-13T08:00:00Z",
            latest_response_at: "2026-07-15T08:00:00Z",
            latest_score: 1, latest_max_score: 1, latest_score_ratio: 1,
            published_response_count: 2, error_response_count: 1, repeated_error: false,
            latest_assessment_title: "第一单元作业", latest_assessment_context: "correction",
            latest_error_grade_decision_public_id: "decision-2",
            latest_error_publication_public_id: "publication-2",
            cause_options: [
              { code: "missing_answer", label: "未作答", description: "学生没有写出答案" },
              { code: "fact_error", label: "史实错误", description: "历史事实错误" },
              { code: "incomplete_expression", label: "表达不完整", description: "内容不完整" },
              { code: "other", label: "其他", description: "老师补充" }
            ],
            cause_review: null,
            correction_assignment: {
              public_id: "correction-2", class_id: 1, student_id: 2,
              student_no: "02", student_name: "小周",
              question_version_public_id: "qv-2",
              source_grade_decision_public_id: "decision-2",
              source_publication_public_id: "publication-2",
              assessment_public_id: "assessment-correction-2",
              assessment_version_public_id: "assessment-version-correction-2",
              assessment_title: "02号 小周 · 《南京条约》签订于____年。 · 订正",
              status: "published", latest_attempt_public_id: "attempt-correction-2",
              created_by: "local_teacher", created_at: "2026-07-14T10:00:00Z"
            },
            reinforcement_assignment: null,
            knowledge_nodes: [], ability_dimensions: []
          },
          {
            student_id: 2, student_no: "02", student_name: "小周",
            question_version_id: "qv-3", question_type: "true_false",
            stem: "辛亥革命结束了中国封建制度。", status: "rechecked_correct",
            first_error_at: "2026-07-10T08:00:00Z", last_error_at: "2026-07-10T08:00:00Z",
            latest_response_at: "2026-07-16T09:00:00Z",
            latest_score: 1, latest_max_score: 1, latest_score_ratio: 1,
            published_response_count: 2, error_response_count: 1, repeated_error: false,
            latest_assessment_title: "期末复测", latest_assessment_context: "exam",
            latest_error_grade_decision_public_id: "decision-3",
            latest_error_publication_public_id: "publication-3",
            cause_options: [
              { code: "fact_error", label: "史实错误", description: "历史事实错误" },
              { code: "concept_confusion", label: "概念混淆", description: "相近概念混淆" },
              { code: "other", label: "其他", description: "老师补充" }
            ],
            cause_review: {
              public_id: "cause-review-3", revision: 1,
              grade_decision_public_id: "decision-3",
              publication_public_id: "publication-3",
              cause_codes: ["concept_confusion"], teacher_note: null,
              confirmed_by: "local_teacher", confirmed_at: "2026-07-16T10:00:00Z"
            },
            correction_assignment: null,
            reinforcement_assignment: null,
            knowledge_nodes: [{ public_id: "k-2", title: "辛亥革命局限" }],
            ability_dimensions: []
          }
        ]
      };
    }
    if (cmd === "confirm_wrongbook_error_causes") {
      const input = args.input;
      return {
        public_id: "cause-review-new", revision: 1,
        grade_decision_public_id: input.gradeDecisionPublicId,
        publication_public_id: input.publicationPublicId,
        cause_codes: input.causeCodes,
        teacher_note: input.teacherNote ?? null,
        confirmed_by: "local_teacher",
        confirmed_at: "2026-07-16T12:10:00Z"
      };
    }
    if (cmd === "create_wrongbook_single_correction") {
      const input = args.input;
      return {
        public_id: "correction-new", class_id: input.classId, student_id: input.studentId,
        student_no: "01", student_name: "小林",
        question_version_public_id: input.questionVersionPublicId,
        source_grade_decision_public_id: input.sourceGradeDecisionPublicId,
        source_publication_public_id: input.sourcePublicationPublicId,
        assessment_public_id: "assessment-correction-new",
        assessment_version_public_id: "assessment-version-correction-new",
        assessment_title: "01号 小林 · 洋务运动失败的根本原因是？ · 订正",
        status: "waiting_upload", latest_attempt_public_id: null,
        created_by: "local_teacher", created_at: "2026-07-16T12:11:00Z"
      };
    }
    if (cmd === "preview_wrongbook_reinforcement") {
      const input = args.input;
      return {
        class_id: input.classId, student_id: input.studentId,
        student_no: "02", student_name: "小周",
        question_version_public_id: input.questionVersionPublicId,
        source_grade_decision_public_id: input.sourceGradeDecisionPublicId,
        source_publication_public_id: input.sourcePublicationPublicId,
        strategy: "same_question_recheck", priority: "normal",
        reason: "已完成一次订正；建议跨日期再次作答，验证是否保持。",
        policy_public_id: "policy-2", policy_revision: 2,
        previewed_as_of_date: "2026-07-16", corrected_on: "2026-07-15",
        earliest_due_date: "2026-07-22", suggested_due_date: "2026-07-22",
        shifted_days: 0, existing_task_count: 1, daily_limit_per_student: 4
      };
    }
    if (cmd === "confirm_wrongbook_reinforcement") {
      const input = args.input;
      return {
        public_id: "reinforcement-new", class_id: input.classId, student_id: input.studentId,
        student_no: "02", student_name: "小周",
        question_version_public_id: input.questionVersionPublicId,
        source_grade_decision_public_id: input.sourceGradeDecisionPublicId,
        source_publication_public_id: input.sourcePublicationPublicId,
        strategy: "same_question_recheck", priority: "normal",
        policy_public_id: input.expectedPolicyPublicId, policy_revision: 2,
        due_date: input.expectedDueDate,
        assessment_public_id: "assessment-reinforcement-new",
        assessment_version_public_id: "assessment-version-reinforcement-new",
        assessment_title: "02号 小周 · 《南京条约》签订于____年。 · 巩固复测",
        task_id: 19, status: "scheduled", latest_attempt_public_id: null,
        created_by: "local_teacher", created_at: "2026-07-16T12:12:00Z"
      };
    }
    if (cmd === "wrongbook_statistics") {
      const input = args.input;
      const studentOnly = input.studentId === 2;
      const emptyClass = input.classId === 2;
      return {
        meta: {
          schema_version: 1, rule_version: "m3-current-facts-statistics-v1",
          calculated_at: "2026-07-16T12:20:00Z",
          range_start: input.rangeStart, range_end: input.rangeEnd,
          exam_watermark: emptyClass ? null : "2026-07-16T11:40:00Z",
          activity_filter_rule: "仅纳入最近一次有效发布作答的上海业务日期落在所选范围内的当前错题事实。",
          evidence_count_rule: "证据数为入选事实关联的全部当前有效已发布老师评分次数，不等同于独立掌握证据数。"
        },
        class: {
          id: input.classId,
          name: emptyClass ? "八年级二班" : "八年级一班",
          term: "2026秋", textbook: "中国历史八上",
          enabled_student_count: emptyClass ? 1 : 3
        },
        selected_student: studentOnly
          ? { id: 2, student_no: "02", name: "小周" }
          : null,
        summary: emptyClass ? {
          student_count: 0, fact_count: 0, evidence_count: 0,
          needs_correction_count: 0, corrected_once_count: 0,
          rechecked_correct_count: 0, repeated_error_count: 0,
          confirmed_cause_review_count: 0, confirmed_cause_item_count: 0,
          unlinked_fact_count: 0
        } : studentOnly ? {
          student_count: 1, fact_count: 2, evidence_count: 4,
          needs_correction_count: 0, corrected_once_count: 1,
          rechecked_correct_count: 1, repeated_error_count: 0,
          confirmed_cause_review_count: 1, confirmed_cause_item_count: 1,
          unlinked_fact_count: 1
        } : {
          student_count: 2, fact_count: 3, evidence_count: 6,
          needs_correction_count: 1, corrected_once_count: 1,
          rechecked_correct_count: 1, repeated_error_count: 1,
          confirmed_cause_review_count: 1, confirmed_cause_item_count: 1,
          unlinked_fact_count: 1
        },
        students: emptyClass ? [] : studentOnly ? [{
          student_id: 2, student_no: "02", student_name: "小周",
          fact_count: 2, evidence_count: 4, needs_correction_count: 0,
          corrected_once_count: 1, rechecked_correct_count: 1,
          repeated_error_count: 0, confirmed_cause_review_count: 1,
          latest_response_at: "2026-07-16T09:00:00Z",
          latest_verification_at: "2026-07-16T09:00:00Z"
        }] : [
          {
            student_id: 1, student_no: "01", student_name: "小林",
            fact_count: 1, evidence_count: 2, needs_correction_count: 1,
            corrected_once_count: 0, rechecked_correct_count: 0,
            repeated_error_count: 1, confirmed_cause_review_count: 0,
            latest_response_at: "2026-07-16T08:00:00Z", latest_verification_at: null
          },
          {
            student_id: 2, student_no: "02", student_name: "小周",
            fact_count: 2, evidence_count: 4, needs_correction_count: 0,
            corrected_once_count: 1, rechecked_correct_count: 1,
            repeated_error_count: 0, confirmed_cause_review_count: 1,
            latest_response_at: "2026-07-16T09:00:00Z",
            latest_verification_at: "2026-07-16T09:00:00Z"
          }
        ],
        question_causes: emptyClass ? [] : [{
          public_id: "qv-3", title: "辛亥革命结束了中国封建制度。",
          confirmed_review_count: 1,
          causes: [{ cause_code: "concept_confusion", cause_label: "概念混淆", count: 1 }]
        }],
        knowledge_causes: emptyClass ? [] : [{
          public_id: "k-2", title: "辛亥革命局限",
          confirmed_review_count: 1,
          causes: [{ cause_code: "concept_confusion", cause_label: "概念混淆", count: 1 }]
        }]
      };
    }
    if (cmd === "create_wrongbook_report_snapshot") {
      const input = args.input;
      return {
        public_id: "report-snapshot-1", report_kind: input.reportKind,
        class_id: input.classId, student_id: input.studentId ?? null,
        range_start: input.rangeStart, range_end: input.rangeEnd,
        schema_version: 1, rule_version: "m3-wrongbook-report-v1",
        source_exam_watermark: "2026-07-16T11:40:00Z",
        evidence_count: input.studentId ? 4 : 6,
        fact_count: input.studentId ? 2 : 3,
        confirmed_cause_review_count: 1,
        payload_sha256: "a".repeat(64), csv_sha256: "b".repeat(64),
        suggested_file_name: input.studentId ? "小周_学习事实.csv" : "八年级一班_错题事实汇总.csv",
        generated_by: "local_teacher", generated_at: "2026-07-16T12:21:00Z"
      };
    }
    if (cmd === "write_wrongbook_report_snapshot") {
      return {
        snapshot_public_id: args.snapshotPublicId,
        file_name: "八年级一班_错题事实汇总.csv",
        byte_size: 2048, sha256: "b".repeat(64)
      };
    }
    if (cmd === "class_operations_dashboard") {
      return {
        meta: { schema_version: 2, rule_version: "m6.1-operations-v2",
          calculated_at: "2026-07-16T12:00:00Z", as_of_date: args.asOfDate,
          recitation_watermark: null, exam_watermark: null },
        class: { id: 1, name: "八年级一班", term: "2026秋",
          textbook: "中国历史八上", enabled_student_count: 3 },
        recitation: {
          expected_student_count: 0, completed_student_count: 0, expected_task_count: 0,
          confirmed_task_count: 0, submitted_task_count: 0, not_submitted_student_count: 0,
          pending_teacher_review_count: 0, overdue_pending_review_count: 0,
          recognition_failure_count: 0, recognition_processing_count: 0,
          denominator_note: "无"
        },
        exam: {
          active_assessment_count: 0, expected_submission_count: 0, submitted_submission_count: 0,
          missing_submission_count: 0, ingesting_attempt_count: 0, grading_attempt_count: 0,
          ready_to_publish_attempt_count: 0, published_submission_count: 0,
          open_pipeline_issue_count: 0, denominator_note: "无"
        },
        students: [], actions: []
      };
    }
    if (cmd === "exam_objective_workbench"
        || cmd === "exam_answer_sheet_subjective_workbench"
        || cmd === "exam_dictation_workbench") {
      return { rows: [], attempts: [] };
    }
    if (cmd === "contents_list" || cmd === "questions_list"
        || cmd === "kp_list" || cmd === "exam_answers_list"
        || cmd === "exam_fixed_intake_options" || cmd === "anomalies_list"
        || cmd === "recognition_failures_list") return [];
    return [];
  }
};
"""


def test_learning_insights(base_url: str) -> None:
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1440, "height": 1000})
        page.add_init_script(MOCK_SCRIPT)
        page.goto(base_url)
        page.wait_for_load_state("networkidle")

        page.locator(".mod-row", has_text="错题与掌握").click()
        expect(page.get_by_role("heading", name="错题与掌握")).to_be_visible()
        expect(page.get_by_text("订正一次 ≠ 已掌握", exact=False)).to_be_visible()
        page.get_by_role("button", name="个人掌握快照").click()
        expect(page.get_by_text("个人学习掌握快照", exact=True)).to_be_visible()
        expect(page.get_by_label("掌握快照学生")).to_have_value("1")
        expect(page.get_by_text("正式逐点证据", exact=True)).to_be_visible()
        preview_notes = page.locator(".profile-preview-notes")
        expect(preview_notes).to_contain_text("纯机器 2")
        expect(preview_notes).to_contain_text("另有 1 条仅总体确认")
        expect(preview_notes).to_contain_text("未映射逐点 1")
        page.get_by_role("button", name="确认生成快照").click()
        expect(page.get_by_text("第 1 版掌握快照", exact=True)).to_be_visible()
        expect(page.locator(".profile-metric-section").first
               .locator(".profile-metric-title")).to_contain_text("未评估不等于薄弱")
        snapshot_panel = page.locator(".profile-snapshot")
        expect(snapshot_panel.get_by_text("错题恢复过程", exact=True)).to_be_visible()
        expect(snapshot_panel.get_by_text("个人同口径趋势", exact=True)).to_be_visible()
        expect(snapshot_panel.get_by_text("知识已评估变化", exact=True)).to_be_visible()
        unassessed_metric = page.locator(".profile-metric", has_text="戊戌变法过程")
        expect(unassessed_metric.get_by_text("戊戌变法过程", exact=True)).to_be_visible()
        expect(unassessed_metric).to_contain_text("未评估")
        stable_metric = page.locator(".profile-metric", has_text="洋务运动失败原因")
        stable_metric.locator("summary").click()
        expect(stable_metric.get_by_text("闭卷", exact=True)).to_be_visible()
        stable_metric.get_by_label("洋务运动失败原因老师补充判断").select_option("observe")
        stable_metric.get_by_label("洋务运动失败原因老师补充说明").fill("课堂回答较好，继续观察")
        stable_metric.get_by_role("button", name="保存判断").click()
        expect(stable_metric.get_by_text("当前：继续观察", exact=False)).to_be_visible()
        assessment_calls = page.evaluate(
            "() => window.__learningCalls.filter((item) => "
            "item.cmd === 'save_profile_teacher_assessment')"
        )
        assert len(assessment_calls) == 1
        assert assessment_calls[0]["args"]["input"]["expectedRevision"] == 0
        assert assessment_calls[0]["args"]["input"]["assessment"] == "observe"
        stable_metric.get_by_role("button", name="清除").click()
        expect(stable_metric.get_by_text("当前：继续观察", exact=False)).not_to_be_visible()
        assessment_calls = page.evaluate(
            "() => window.__learningCalls.filter((item) => "
            "item.cmd === 'save_profile_teacher_assessment')"
        )
        assert len(assessment_calls) == 2
        assert assessment_calls[1]["args"]["input"]["expectedRevision"] == 1
        assert assessment_calls[1]["args"]["input"]["assessment"] is None
        profile_calls = page.evaluate(
            "() => window.__learningCalls.filter((item) => "
            "item.cmd === 'generate_student_profile')"
        )
        assert len(profile_calls) == 1
        assert profile_calls[0]["args"]["input"]["classId"] == 1
        assert profile_calls[0]["args"]["input"]["studentId"] == 1
        page.screenshot(path="/tmp/jiaofu-student-profile.png", full_page=True)

        page.get_by_role("button", name="错题事实").click()
        expect(page.locator(".learning-stat")).to_have_count(4)
        expect(page.locator(".wrongbook-item")).to_have_count(3)
        expect(page.get_by_text("洋务运动失败的根本原因是？", exact=True)).to_be_visible()
        expect(page.locator(".wrongbook-item .bad-text", has_text="重复出错")).to_be_visible()
        expect(page.get_by_text("尚未绑定已确认的知识点或能力", exact=False)).to_be_visible()

        page.get_by_role("button", name="统计与导出").click()
        expect(page.get_by_text("班级错题统计与导出", exact=True)).to_be_visible()
        expect(page.get_by_text("自然学号顺序，不按数量高低排序", exact=True)).to_be_visible()
        expect(page.get_by_text("题目错因分布", exact=True)).to_be_visible()
        expect(page.get_by_text("辛亥革命局限", exact=True)).to_be_visible()
        page.get_by_role("button", name="导出班级表格").click()
        expect(page.get_by_text("已导出 八年级一班_错题事实汇总.csv", exact=True)).to_be_visible()
        report_calls = page.evaluate(
            "() => window.__learningCalls.filter((item) => "
            "item.cmd === 'create_wrongbook_report_snapshot')"
        )
        assert len(report_calls) == 1
        assert report_calls[0]["args"]["input"]["reportKind"] == "class_summary"
        assert report_calls[0]["args"]["input"]["studentId"] is None

        page.get_by_role("button", name="巩固规则").click()
        expect(page.get_by_text("只有老师确认后才会建立任务", exact=False)).to_be_visible()
        limit_input = page.locator(".schedule-policy-main input[type=number]").nth(1)
        limit_input.fill("4")
        page.get_by_role("button", name="保存规则").click()
        expect(page.get_by_text("当前第 2 版", exact=True)).to_be_visible()
        policy_calls = page.evaluate(
            "() => window.__learningCalls.filter((item) => "
            "item.cmd === 'update_wrongbook_schedule_policy')"
        )
        assert len(policy_calls) == 1
        assert policy_calls[0]["args"]["input"]["dailyLimitPerStudent"] == 4
        page.get_by_role("button", name="收起规则").click()

        first_item = page.locator(".wrongbook-item").first
        first_item.get_by_role("button", name="确认错因").click()
        first_item.get_by_text("史实错误", exact=True).click()
        first_item.get_by_text("概念混淆", exact=True).click()
        first_item.get_by_label("错因备注").fill("根本原因与直接原因混淆")
        first_item.get_by_role("button", name="保存错因").click()
        expect(first_item.get_by_text("老师确认错因", exact=True)).to_be_visible()
        expect(first_item.get_by_text("根本原因与直接原因混淆", exact=True)).to_be_visible()
        cause_calls = page.evaluate(
            "() => window.__learningCalls.filter((item) => "
            "item.cmd === 'confirm_wrongbook_error_causes')"
        )
        assert len(cause_calls) == 1
        assert cause_calls[0]["args"]["input"]["causeCodes"] == ["fact_error", "concept_confusion"]
        assert cause_calls[0]["args"]["input"]["gradeDecisionPublicId"] == "decision-1"

        first_item.get_by_role("button", name="建立订正").click()
        expect(first_item.get_by_text("不会修改原成绩", exact=False)).to_be_visible()
        first_item.get_by_role("button", name="确认建立").click()
        expect(first_item.get_by_text("订正已建立 · 等待上传", exact=True)).to_be_visible()
        expect(first_item.get_by_role("button", name="去上传批改")).to_be_visible()
        correction_calls = page.evaluate(
            "() => window.__learningCalls.filter((item) => "
            "item.cmd === 'create_wrongbook_single_correction')"
        )
        assert len(correction_calls) == 1
        correction_input = correction_calls[0]["args"]["input"]
        assert correction_input["classId"] == 1
        assert correction_input["studentId"] == 1
        assert correction_input["questionVersionPublicId"] == "qv-1"
        assert correction_input["sourceGradeDecisionPublicId"] == "decision-1"
        assert correction_input["sourcePublicationPublicId"] == "publication-1"

        corrected_item = page.locator(".wrongbook-item", has_text="《南京条约》签订于____年。")
        corrected_item.get_by_role("button", name="安排巩固").click()
        expect(corrected_item.get_by_text("建议 2026-07-22 再做一次", exact=True)).to_be_visible()
        before_confirm = page.evaluate(
            "() => window.__learningCalls.filter((item) => "
            "item.cmd === 'confirm_wrongbook_reinforcement').length"
        )
        assert before_confirm == 0
        corrected_item.get_by_role("button", name="确认安排").click()
        expect(corrected_item.get_by_text("巩固已安排 · 2026-07-22", exact=True)).to_be_visible()
        reinforcement_calls = page.evaluate(
            "() => window.__learningCalls.filter((item) => "
            "item.cmd === 'confirm_wrongbook_reinforcement')"
        )
        assert len(reinforcement_calls) == 1
        reinforcement_input = reinforcement_calls[0]["args"]["input"]
        assert reinforcement_input["expectedPolicyPublicId"] == "policy-2"
        assert reinforcement_input["expectedDueDate"] == "2026-07-22"
        assert reinforcement_input["previewedAsOfDate"] == "2026-07-16"
        page.wait_for_function(
            "() => window.__learningCalls.filter((item) => "
            "item.cmd === 'wrongbook_statistics').length >= 4"
        )
        page.screenshot(path="/tmp/jiaofu-learning-insights.png", full_page=True)

        page.get_by_label("筛选订正状态").select_option("needs_correction")
        expect(page.locator(".wrongbook-item")).to_have_count(1)
        page.get_by_label("筛选订正状态").select_option("all")
        page.get_by_label("筛选学生").select_option("2")
        expect(page.locator(".wrongbook-item")).to_have_count(2)
        expect(page.get_by_text("02号 小周 · 学习事实报告", exact=True)).to_be_visible()
        page.get_by_role("button", name="导出家长沟通表").click()
        student_report_calls = page.evaluate(
            "() => window.__learningCalls.filter((item) => "
            "item.cmd === 'create_wrongbook_report_snapshot')"
        )
        assert len(student_report_calls) == 2
        assert student_report_calls[1]["args"]["input"]["reportKind"] == "student_parent"
        assert student_report_calls[1]["args"]["input"]["studentId"] == 2

        page.get_by_label("筛选学生").select_option("all")
        page.get_by_role("button", name="去题目批改").click()
        expect(page.get_by_role("heading", name="题目批改")).to_be_visible()

        page.locator(".mod-row", has_text="错题与掌握").click()
        page.locator(".learning-scope select").select_option("2")
        expect(page.get_by_text("当前没有符合口径的已发布错题。", exact=True)).to_be_visible()

        narrow = browser.new_page(viewport={"width": 760, "height": 900})
        narrow.add_init_script(MOCK_SCRIPT)
        narrow.goto(base_url)
        narrow.wait_for_load_state("networkidle")
        narrow.locator(".mod-row", has_text="错题与掌握").click()
        expect(narrow.locator(".wrongbook-item")).to_have_count(3)
        narrow.get_by_role("button", name="个人掌握快照").click()
        expect(narrow.get_by_text("个人学习掌握快照", exact=True)).to_be_visible()
        expect(narrow.get_by_role("button", name="确认生成快照")).to_be_visible()
        narrow.get_by_role("button", name="错题事实").click()
        narrow.get_by_role("button", name="统计与导出").click()
        expect(narrow.get_by_text("班级错题统计与导出", exact=True)).to_be_visible()
        narrow.screenshot(path="/tmp/jiaofu-learning-insights-narrow.png", full_page=True)

        empty = browser.new_page(viewport={"width": 1100, "height": 800})
        empty.add_init_script("window.__NO_CLASSES__ = true;")
        empty.add_init_script(MOCK_SCRIPT)
        empty.goto(base_url)
        empty.wait_for_load_state("networkidle")
        empty.locator(".mod-row", has_text="错题与掌握").click()
        expect(empty.locator(".empty-state")).to_contain_text("暂无班级")
        expect(empty.get_by_role("button", name="去建立班级")).to_be_visible()

        browser.close()


if __name__ == "__main__":
    test_learning_insights("http://127.0.0.1:4173")
    print("learning insights UI smoke: PASS")
