import { useRef, useState } from "react";
import {
  examObjectivePublishAttempt,
  examDictationPublishAttempt,
  examAnswerSheetSubjectivePublishAttempt,
  type ObjectiveWorkbench,
  type SubjectiveWorkbench,
  type DictationWorkbench,
  type ObjectiveAttemptSummary,
} from "../../api/exam";

interface Props {
  objective: ObjectiveWorkbench;
  subjective: SubjectiveWorkbench;
  dictation: DictationWorkbench;
  onDone: (message: string) => void;
  onError: (message: string) => void;
}
export default function TaskResults({
  objective,
  subjective,
  dictation,
  onDone,
  onError,
}: Props) {
  const publishing = useRef(false);
  const [busy, setBusy] = useState(false);
  const attempts = new Map<number, ObjectiveAttemptSummary>();
  for (const attempt of [
    ...objective.attempts,
    ...subjective.attempts,
    ...dictation.attempts,
  ]) {
    const current = attempts.get(attempt.attempt_id);
    attempts.set(
      attempt.attempt_id,
      current
        ? {
            ...current,
            can_publish: current.can_publish && attempt.can_publish,
          }
        : attempt,
    );
  }
  async function publish(attempt: ObjectiveAttemptSummary) {
    if (publishing.current) return;
    if (
      !window.confirm(
        `确认发布 ${attempt.student_name} 的“${attempt.assessment_title}”？仅发布这名学生本次已确认的整卷成绩。`,
      )
    )
      return;
    publishing.current = true;
    setBusy(true);
    try {
      const command = dictation.attempts.some(
        (row) => row.attempt_id === attempt.attempt_id,
      )
        ? examDictationPublishAttempt
        : subjective.attempts.some(
              (row) => row.attempt_id === attempt.attempt_id,
            )
          ? examAnswerSheetSubjectivePublishAttempt
          : examObjectivePublishAttempt;
      const result = await command(attempt.attempt_id);
      onDone(
        `${attempt.student_name} 的本次成绩已发布：${result.total_score} 分`,
      );
    } catch (reason) {
      onError(String(reason));
    } finally {
      publishing.current = false;
      setBusy(false);
    }
  }
  return (
    <section aria-label="本次任务结果">
      <p className="sub">
        确认评分与发布分别进行。发布只采用老师已经确认的结果。
      </p>
      {!attempts.size ? (
        <div className="workspace-empty">
          还没有可查看的整卷结果，请先完成材料处理。
        </div>
      ) : (
        <div className="task-results">
          {[...attempts.values()].map((attempt) => (
            <article className="task-result" key={attempt.attempt_id}>
              <div>
                <h2>{attempt.student_name}</h2>
                <p>
                  {attempt.attempt_state === "published"
                    ? `已发布 · ${attempt.published_total_score ?? "—"} 分`
                    : `已确认 ${attempt.confirmed_count}/${attempt.item_count} 题 · 尚未发布`}
                </p>
              </div>
              {attempt.attempt_state !== "published" &&
                (attempt.can_publish ? (
                  <button
                    className="primary"
                    disabled={busy}
                    onClick={() => void publish(attempt)}
                  >
                    发布这份结果
                  </button>
                ) : (
                  <span className="muted">完成全部核对后可发布</span>
                ))}
            </article>
          ))}
        </div>
      )}
    </section>
  );
}
