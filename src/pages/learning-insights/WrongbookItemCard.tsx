import {
  CorrectionAssignmentStatus,
  ReinforcementAssignmentStatus,
  WrongbookQuestion,
  ErrorCauseReview,
  confirmWrongbookErrorCauses,
  CorrectionAssignment,
  createWrongbookSingleCorrection,
  ReinforcementAssignment,
  ReinforcementSuggestion,
  previewWrongbookReinforcement,
  confirmWrongbookReinforcement,
} from "../../api/learning";
import { useState, useEffect } from "react";
import {
  statusClass,
  STATUS_LABELS,
  QUESTION_TYPE_LABELS,
  score,
  CONTEXT_LABELS,
  formatTime,
} from "./shared";

const CORRECTION_STATUS_LABELS: Record<CorrectionAssignmentStatus, string> = {
  waiting_upload: "订正已建立 · 等待上传",
  in_progress: "订正处理中",
  ready_to_publish: "订正待发布",
  published: "订正已发布",
};

const REINFORCEMENT_STATUS_LABELS: Record<ReinforcementAssignmentStatus, string> = {
  scheduled: "巩固已安排",
  in_progress: "巩固处理中",
  ready_to_publish: "巩固待发布",
  published: "巩固已发布",
};

interface CauseEditorProps {
  classId: number;
  item: WrongbookQuestion;
  onSaved: (review: ErrorCauseReview) => void;
}

function CauseEditor({ classId, item, onSaved }: CauseEditorProps) {
  const [open, setOpen] = useState(false);
  const [selected, setSelected] = useState<string[]>(item.cause_review?.cause_codes ?? []);
  const [note, setNote] = useState(item.cause_review?.teacher_note ?? "");
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState("");

  useEffect(() => {
    setSelected(item.cause_review?.cause_codes ?? []);
    setNote(item.cause_review?.teacher_note ?? "");
  }, [item.cause_review]);

  const toggleCause = (code: string) => {
    setSelected((current) => {
      if (current.includes(code)) return current.filter((value) => value !== code);
      if (current.length >= 3) return current;
      return [...current, code];
    });
  };

  const save = async () => {
    setSaving(true);
    setSaveError("");
    try {
      const review = await confirmWrongbookErrorCauses({
        classId,
        studentId: item.student_id,
        questionVersionPublicId: item.question_version_id,
        gradeDecisionPublicId: item.latest_error_grade_decision_public_id,
        publicationPublicId: item.latest_error_publication_public_id,
        causeCodes: selected,
        teacherNote: note.trim() || null,
      });
      onSaved(review);
      setOpen(false);
    } catch (reason) {
      setSaveError(String(reason));
    } finally {
      setSaving(false);
    }
  };

  const labelFor = (code: string) =>
    item.cause_options.find((option) => option.code === code)?.label ?? code;

  return (
    <div className="wrongbook-causes">
      {item.cause_review && (
        <div className="wrongbook-cause-summary">
          <b>老师确认错因</b>
          {item.cause_review.cause_codes.map((code) => (
            <span key={code}>{labelFor(code)}</span>
          ))}
          {item.cause_review.teacher_note && <em>{item.cause_review.teacher_note}</em>}
        </div>
      )}
      {!open ? (
        <button className="wrongbook-cause-open" onClick={() => setOpen(true)}>
          {item.cause_review ? "修改错因" : "确认错因"}
        </button>
      ) : (
        <div className="wrongbook-cause-editor">
          <div className="wrongbook-cause-title">
            <b>选择主要错因</b>
            <span>最多 3 个；只有保存后才进入正式统计</span>
          </div>
          <div className="wrongbook-cause-options">
            {item.cause_options.map((option) => {
              const checked = selected.includes(option.code);
              return (
                <label
                  className={checked ? "selected" : ""}
                  title={option.description}
                  key={option.code}
                >
                  <input
                    type="checkbox"
                    checked={checked}
                    disabled={!checked && selected.length >= 3}
                    onChange={() => toggleCause(option.code)}
                  />
                  <span>{option.label}</span>
                </label>
              );
            })}
          </div>
          <textarea
            aria-label="错因备注"
            value={note}
            maxLength={500}
            onChange={(event) => setNote(event.target.value)}
            placeholder={selected.includes("other") ? "选择“其他”时请补充说明" : "补充说明（可选）"}
          />
          {saveError && <div className="error">{saveError}</div>}
          <div className="wrongbook-cause-actions">
            <button
              onClick={() => {
                setOpen(false);
                setSelected(item.cause_review?.cause_codes ?? []);
                setNote(item.cause_review?.teacher_note ?? "");
                setSaveError("");
              }}
            >
              取消
            </button>
            <button className="primary" disabled={selected.length === 0 || saving} onClick={save}>
              {saving ? "保存中…" : "保存错因"}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

function CorrectionAction({
  classId,
  item,
  onCreated,
  onOpenExam,
}: {
  classId: number;
  item: WrongbookQuestion;
  onCreated: (assignment: CorrectionAssignment) => void;
  onOpenExam: () => void;
}) {
  const [confirming, setConfirming] = useState(false);
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState("");

  const create = async () => {
    setCreating(true);
    setCreateError("");
    try {
      const assignment = await createWrongbookSingleCorrection({
        classId,
        studentId: item.student_id,
        questionVersionPublicId: item.question_version_id,
        sourceGradeDecisionPublicId: item.latest_error_grade_decision_public_id,
        sourcePublicationPublicId: item.latest_error_publication_public_id,
      });
      onCreated(assignment);
      setConfirming(false);
    } catch (reason) {
      setCreateError(String(reason));
    } finally {
      setCreating(false);
    }
  };

  if (item.correction_assignment) {
    return (
      <div className="wrongbook-correction-summary">
        <div>
          <span>{CORRECTION_STATUS_LABELS[item.correction_assignment.status]}</span>
          <b title={item.correction_assignment.assessment_title}>
            {item.correction_assignment.assessment_title}
          </b>
        </div>
        <button onClick={onOpenExam}>
          {item.correction_assignment.status === "published" ? "查看批改" : "去上传批改"}
        </button>
      </div>
    );
  }

  if (item.status !== "needs_correction") return null;

  return (
    <div className="wrongbook-correction">
      {!confirming ? (
        <button className="primary" onClick={() => setConfirming(true)}>建立订正</button>
      ) : (
        <div className="wrongbook-correction-confirm">
          <b>为 {item.student_no} 号 {item.student_name} 建立这道订正？</b>
          <span>只建立这一题并等待照片上传；不会修改原成绩、原错因或掌握结论。</span>
          {createError && <div className="error">{createError}</div>}
          <div>
            <button
              disabled={creating}
              onClick={() => {
                setConfirming(false);
                setCreateError("");
              }}
            >
              取消
            </button>
            <button className="primary" disabled={creating} onClick={create}>
              {creating ? "建立中…" : "确认建立"}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

function ReinforcementAction({
  classId,
  item,
  onCreated,
  onOpenExam,
}: {
  classId: number;
  item: WrongbookQuestion;
  onCreated: (assignment: ReinforcementAssignment) => void;
  onOpenExam: () => void;
}) {
  const [preview, setPreview] = useState<ReinforcementSuggestion | null>(null);
  const [loadingPreview, setLoadingPreview] = useState(false);
  const [creating, setCreating] = useState(false);
  const [actionError, setActionError] = useState("");

  if (item.reinforcement_assignment) {
    const assignment = item.reinforcement_assignment;
    return (
      <div className="wrongbook-reinforcement-summary">
        <div>
          <span>{REINFORCEMENT_STATUS_LABELS[assignment.status]} · {assignment.due_date}</span>
          <b>{assignment.priority === "high" ? "重复出错，已按高优先级安排" : "跨日期再次作答"}</b>
        </div>
        <button onClick={onOpenExam}>
          {assignment.status === "published" ? "查看批改" : "去上传批改"}
        </button>
      </div>
    );
  }
  if (item.status !== "corrected_once") return null;

  const scope = {
    classId,
    studentId: item.student_id,
    questionVersionPublicId: item.question_version_id,
    sourceGradeDecisionPublicId: item.latest_error_grade_decision_public_id,
    sourcePublicationPublicId: item.latest_error_publication_public_id,
  };
  const loadPreview = async () => {
    setLoadingPreview(true);
    setActionError("");
    try {
      setPreview(await previewWrongbookReinforcement(scope));
    } catch (reason) {
      setActionError(String(reason));
    } finally {
      setLoadingPreview(false);
    }
  };
  const confirm = async () => {
    if (!preview) return;
    setCreating(true);
    setActionError("");
    try {
      const assignment = await confirmWrongbookReinforcement({
        ...scope,
        expectedPolicyPublicId: preview.policy_public_id,
        expectedDueDate: preview.suggested_due_date,
        previewedAsOfDate: preview.previewed_as_of_date,
      });
      onCreated(assignment);
      setPreview(null);
    } catch (reason) {
      setActionError(String(reason));
    } finally {
      setCreating(false);
    }
  };

  return (
    <div className="wrongbook-reinforcement">
      {!preview ? (
        <>
          <button className="primary" disabled={loadingPreview} onClick={loadPreview}>
            {loadingPreview ? "计算中…" : "安排巩固"}
          </button>
          {actionError && <div className="error">{actionError}</div>}
        </>
      ) : (
        <div className="wrongbook-reinforcement-confirm">
          <div>
            <b>建议 {preview.suggested_due_date} 再做一次</b>
            <span>{preview.reason}</span>
            <small>
              当天已有 {preview.existing_task_count} / {preview.daily_limit_per_student} 项；
              规则第 {preview.policy_revision} 版
            </small>
          </div>
          {actionError && <div className="error">{actionError}</div>}
          <div className="wrongbook-reinforcement-actions">
            <button
              disabled={creating}
              onClick={() => {
                setPreview(null);
                setActionError("");
              }}
            >
              取消
            </button>
            <button className="primary" disabled={creating} onClick={confirm}>
              {creating ? "安排中…" : "确认安排"}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

export function ItemCard({
  classId,
  item,
  onCauseSaved,
  onCorrectionCreated,
  onReinforcementCreated,
  onOpenExam,
}: {
  classId: number;
  item: WrongbookQuestion;
  onCauseSaved: (review: ErrorCauseReview) => void;
  onCorrectionCreated: (assignment: CorrectionAssignment) => void;
  onReinforcementCreated: (assignment: ReinforcementAssignment) => void;
  onOpenExam: () => void;
}) {
  return (
    <article className={`wrongbook-item ${item.repeated_error ? "repeated" : ""}`}>
      <div className="wrongbook-student">
        <span className="avt q">{item.student_no}</span>
        <div>
          <b>{item.student_name}</b>
          <span>{item.student_no} 号 · {item.latest_assessment_title}</span>
        </div>
        <span className={statusClass(item.status)}>{STATUS_LABELS[item.status]}</span>
      </div>
      <div className="wrongbook-question">
        <div className="wrongbook-question-head">
          <span className="cno">{QUESTION_TYPE_LABELS[item.question_type] ?? item.question_type}</span>
          <b>{item.stem}</b>
        </div>
        <div className="wrongbook-facts">
          <span>最近得分 <b>{score(item.latest_score)} / {score(item.latest_max_score)}</b></span>
          <span>{CONTEXT_LABELS[item.latest_assessment_context] ?? item.latest_assessment_context}</span>
          <span>已发布作答 {item.published_response_count} 次</span>
          <span>非满分 {item.error_response_count} 次</span>
          {item.repeated_error && <span className="bad-text">重复出错</span>}
        </div>
        <div className="wrongbook-labels">
          {item.knowledge_nodes.map((node) => (
            <span className="wrongbook-label knowledge" key={node.public_id}>知识 · {node.title}</span>
          ))}
          {item.ability_dimensions.map((node) => (
            <span className="wrongbook-label ability" key={node.public_id}>能力 · {node.title}</span>
          ))}
          {item.knowledge_nodes.length === 0 && item.ability_dimensions.length === 0 && (
            <span className="wrongbook-unlinked">尚未绑定已确认的知识点或能力，不影响错题事实</span>
          )}
        </div>
        <CauseEditor classId={classId} item={item} onSaved={onCauseSaved} />
        <CorrectionAction
          classId={classId}
          item={item}
          onCreated={onCorrectionCreated}
          onOpenExam={onOpenExam}
        />
        <ReinforcementAction
          classId={classId}
          item={item}
          onCreated={onReinforcementCreated}
          onOpenExam={onOpenExam}
        />
      </div>
      <div className="wrongbook-time">
        <span>最近错误</span>
        <b>{formatTime(item.last_error_at)}</b>
        <span>最近发布</span>
        <b>{formatTime(item.latest_response_at)}</b>
      </div>
    </article>
  );
}
