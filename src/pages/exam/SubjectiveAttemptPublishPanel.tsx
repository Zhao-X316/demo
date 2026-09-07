import type { ObjectiveAttemptSummary } from "../../api/exam";

export function SubjectiveAttemptPublishPanel({
  attempts,
  busy,
  onPublish,
}: {
  attempts: ObjectiveAttemptSummary[];
  busy: boolean;
  onPublish: (attemptId: number, studentName: string) => void;
}) {
  return (
    <>
      <div className="sech">整份答题卡发布 <span className="n">所有题型终审完成后才可发布</span></div>
      <div className="objective-publish-grid">
        {attempts.map((attempt) => (
          <article className="exam-card objective-attempt" key={attempt.attempt_id}>
            <div className="exam-card-head">
              <div><b>{attempt.student_name}</b><div className="meta">{attempt.student_no}</div></div>
              <span className={attempt.attempt_state === "published" ? "tag pass" : attempt.can_publish ? "tag wait" : "tag"}>
                {attempt.attempt_state === "published" ? "已发布" : attempt.can_publish ? "待发布" : "终审中"}
              </span>
            </div>
            <div className="objective-attempt-score"><b>{attempt.teacher_total_score}</b><span>/ {attempt.max_total_score} 分</span></div>
            <div className="meta">已终审 {attempt.confirmed_count} / {attempt.item_count} 题</div>
            {attempt.published_total_score != null && <div className="meta">当前已发布总分：{attempt.published_total_score}</div>}
            <button className="primary" disabled={busy || !attempt.can_publish} onClick={() => void onPublish(attempt.attempt_id, attempt.student_name)}>
              {attempt.attempt_state === "published" ? "已发布" : attempt.can_publish ? "确认发布整份答题卡" : "完成全部终审后发布"}
            </button>
          </article>
        ))}
      </div>
    </>
  );
}
