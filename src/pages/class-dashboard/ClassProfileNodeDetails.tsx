import { ClassProfileNodeMetric } from "../../api/classDashboard";
import { profileCellClass, PROFILE_STATUS } from "./shared";

export function ClassProfileNodeDetails({
  node,
  onClose,
  onOpenLearning,
}: {
  node: ClassProfileNodeMetric;
  onClose: () => void;
  onOpenLearning: () => void;
}) {
  return (
    <div className="class-profile-detail">
      <div className="dashboard-panel-head">
        <div>
          <b>{node.target_title}</b>
          <span>{node.explanation}</span>
        </div>
        <button onClick={onClose}>关闭</button>
      </div>
      <div className="class-profile-detail-counts">
        <span>全班 <b>{node.total_student_count}</b></span>
        <span>有快照 <b>{node.snapshot_student_count}</b></span>
        <span>已评估 <b>{node.assessed_student_count}</b></span>
        <span>合格样本 <b>{node.eligible_student_count}</b></span>
        <span>需要支持 <b>{node.needs_support_count}</b></span>
        <span>发展中 <b>{node.developing_count}</b></span>
        <span>较稳定 <b>{node.stable_count}</b></span>
        <span>证据不足 <b>{node.insufficient_evidence_count}</b></span>
      </div>
      {!node.sample_sufficient && (
        <div className="dashboard-rule-note">
          当前班级样本不足，不形成共同薄弱结论。合格样本率为 {Math.round(node.eligible_ratio * 100)}%。
        </div>
      )}
      <div className="class-profile-detail-students">
        {node.cells.map((cell) => (
          <div key={cell.student.id}>
            <b>{cell.student.student_no}号 {cell.student.name}</b>
            <span className={profileCellClass(cell.status)}>
              {PROFILE_STATUS[cell.status] ?? cell.status}
            </span>
            <small>
              {cell.mastery_score === null ? "无正式掌握分" : `个人掌握 ${Math.round(cell.mastery_score * 100)}%`}
              {cell.last_evidence_at ? ` · 最近证据 ${new Date(cell.last_evidence_at).toLocaleDateString()}` : ""}
            </small>
          </div>
        ))}
      </div>
      <button className="link" onClick={onOpenLearning}>打开个人掌握与证据</button>
    </div>
  );
}
