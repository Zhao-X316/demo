import { useMemo, useRef, useState } from "react";
import type {
  ObjectiveWorkbench,
  SubjectiveWorkbench,
  DictationWorkbench,
} from "../../api/exam";
import { ObjectiveReviewTab } from "../exam/ObjectiveReviewTab";
import { SubjectiveReviewTab } from "../exam/SubjectiveReviewTab";
import { DictationReviewTab } from "../exam/DictationReviewTab";

interface Props {
  objective: ObjectiveWorkbench;
  subjective: SubjectiveWorkbench;
  dictation: DictationWorkbench;
  onDone: (message: string) => void;
  onError: (message: string) => void;
}
type Queue = "judgment" | "ready" | "blocked" | "all";
export default function TaskReview({
  objective,
  subjective,
  dictation,
  onDone,
  onError,
}: Props) {
  const [queue, setQueue] = useState<Queue | null>(null);
  const dirty = useRef(false);
  const change = (action: () => void) => {
    if (
      !dirty.current ||
      window.confirm("这道题的修改尚未保存。舍弃修改并切换？")
    ) {
      dirty.current = false;
      action();
    }
  };
  const saved = (message: string) => {
    dirty.current = false;
    onDone(message);
  };
  const [selected, setSelected] = useState("");
  const entries = useMemo(
    () =>
      [
        ...objective.rows.map((row) => ({
          kind: "objective" as const,
          id: `objective:${row.suggestion_id}`,
          row,
          confirmed: row.current_suggestion_confirmed,
          blocked: row.observation_state === "failed",
          ready: row.batch_eligible,
        })),
        ...subjective.rows.map((row) => ({
          kind: "subjective" as const,
          id: `subjective:${row.suggestion_id}`,
          row,
          confirmed: row.current_suggestion_confirmed,
          blocked: row.result_state === "recognize_failed",
          ready: false,
        })),
        ...dictation.rows.map((row) => ({
          kind: "dictation" as const,
          id: `dictation:${row.transcription_revision_id}`,
          row,
          confirmed: row.current_transcription_confirmed,
          blocked: row.result_state === "recognize_failed",
          ready:
            !row.requires_teacher_review && row.result_state === "recognized",
        })),
      ].sort(
        (a, b) =>
          a.row.student_no.localeCompare(b.row.student_no, "zh-CN", {
            numeric: true,
          }) || a.row.order_index - b.row.order_index,
      ),
    [objective, subjective, dictation],
  );
  const matches = (entry: (typeof entries)[number], value: Queue) =>
    value === "all" ||
    (!entry.confirmed &&
      (value === "blocked"
        ? entry.blocked
        : value === "ready"
          ? !entry.blocked && entry.ready
          : !entry.blocked && !entry.ready));
  const activeQueue =
    queue ??
    (entries.some((entry) => matches(entry, "blocked"))
      ? "blocked"
      : entries.some((entry) => matches(entry, "judgment"))
        ? "judgment"
        : "ready");
  const filtered = entries.filter((entry) => matches(entry, activeQueue));
  const current =
    filtered.find((entry) => entry.id === selected) ?? filtered[0];
  // Only the eligible queue offers domain-supported batch review, for the selected question.
  const group =
    activeQueue === "ready" && current
      ? filtered.filter(
          (entry) =>
            entry.kind === current.kind &&
            entry.row.assessment_item_id === current.row.assessment_item_id,
        )
      : current
        ? [current]
        : [];
  return (
    <section aria-label="本次任务核对">
      <div className="tabs">
        {(
          [
            ["judgment", "需要判断"],
            ["ready", "待确认"],
            ["blocked", "暂时无法处理"],
            ["all", "全部项目"],
          ] as const
        ).map(([value, label]) => (
          <button
            key={value}
            className={activeQueue === value ? "tab active" : "tab"}
            onClick={() =>
              change(() => {
                setQueue(value);
                setSelected("");
              })
            }
          >
            {label} {entries.filter((entry) => matches(entry, value)).length}
          </button>
        ))}
      </div>
      {!current ? (
        <div className="workspace-empty">
          <h2>这个队列已处理完</h2>
          <p>可以查看其他队列；所有评分确认后，再到结果页发布。</p>
        </div>
      ) : (
        <div className="task-review-layout">
          <div className="task-review-list" aria-label="待核对项目">
            {filtered.map((entry) => (
              <button
                key={entry.id}
                className={current.id === entry.id ? "active" : ""}
                onClick={() => change(() => setSelected(entry.id))}
              >
                {entry.row.student_name} · 第 {entry.row.question_no} 题
                <small>
                  {entry.kind === "objective"
                    ? "客观题"
                    : entry.kind === "subjective"
                      ? "主观题"
                      : "默写"}
                  {entry.confirmed ? " · 已确认" : ""}
                </small>
              </button>
            ))}
          </div>
          <div
            className="task-review-content"
            onChangeCapture={() => {
              dirty.current = true;
            }}
          >
            {current.kind === "objective" && (
              <ObjectiveReviewTab taskMode
                workbench={{
                  rows: group.flatMap((entry) =>
                    entry.kind === "objective" ? [entry.row] : [],
                  ),
                  attempts: [],
                }}
                onDone={saved}
                onError={onError}
              />
            )}
            {current.kind === "subjective" && (
              <SubjectiveReviewTab taskMode
                workbench={{ rows: [current.row], attempts: [] }}
                onDone={saved}
                onError={onError}
              />
            )}
            {current.kind === "dictation" && (
              <DictationReviewTab taskMode
                workbench={{
                  rows: group.flatMap((entry) =>
                    entry.kind === "dictation" ? [entry.row] : [],
                  ),
                  attempts: [],
                }}
                onDone={saved}
                onError={onError}
              />
            )}
          </div>
        </div>
      )}
    </section>
  );
}
