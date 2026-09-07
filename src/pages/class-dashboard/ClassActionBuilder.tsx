import {
  ClassActionKind,
  ClassProfileSnapshot,
  ClassProfileNodeMetric,
  AppModule,
  DashboardTargetView,
  ClassActionPreview,
  ClassActionDraft,
  listClassActionDrafts,
  previewClassAction,
  materializeClassAction,
  confirmClassAction,
} from "../../api/classDashboard";
import { useState, useEffect } from "react";
import { newRequestKey, profileCellClass, PROFILE_STATUS } from "./shared";

const CLASS_ACTION_TYPES: Array<{ value: ClassActionKind; label: string; hint: string }> = [
  { value: "reteach", label: "再讲解", hint: "保存一次短讲与核对计划" },
  { value: "practice", label: "题目练习", hint: "从已确认题库链接中选题并建立定向作业" },
  { value: "recitation", label: "背诵巩固", hint: "需要背诵内容先可靠链接到知识点" },
  { value: "temporary_group", label: "临时小组", hint: "只保存本次支持名单，不写学生标签" },
];

export function ClassActionBuilder({
  snapshot,
  node,
  onNavigate,
}: {
  snapshot: ClassProfileSnapshot;
  node: ClassProfileNodeMetric;
  onNavigate: (module: AppModule, view: DashboardTargetView | "students") => void;
}) {
  const [actionKind, setActionKind] = useState<ClassActionKind>("practice");
  const [preview, setPreview] = useState<ClassActionPreview | null>(null);
  const [drafts, setDrafts] = useState<ClassActionDraft[]>([]);
  const [title, setTitle] = useState("");
  const [rationale, setRationale] = useState("");
  const [estimatedMinutes, setEstimatedMinutes] = useState(15);
  const [selectedTargets, setSelectedTargets] = useState<Set<number>>(new Set());
  const [selectedCandidates, setSelectedCandidates] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [savedDraft, setSavedDraft] = useState<ClassActionDraft | null>(null);

  const reloadDrafts = async () => {
    const items = await listClassActionDrafts(snapshot.class.id, 20);
    setDrafts(items.filter((item) => item.node_metric_public_id === node.public_id));
  };

  useEffect(() => {
    let current = true;
    setLoading(true);
    setError("");
    setSavedDraft(null);
    previewClassAction({
      snapshotPublicId: snapshot.public_id,
      nodeMetricPublicId: node.public_id,
      actionKind,
    })
      .then((value) => {
        if (!current) return;
        setPreview(value);
        setTitle(value.suggested_title);
        setRationale(value.suggested_rationale);
        setEstimatedMinutes(value.suggested_estimated_minutes);
        setSelectedTargets(new Set(
          value.targets.filter((target) => target.recommended).map((target) => target.student.id),
        ));
        setSelectedCandidates(new Set(
          value.candidates
            .filter((candidate) => candidate.active_assignment_count === 0)
            .slice(0, 3)
            .map((candidate) => candidate.question_version_public_id),
        ));
      })
      .catch((reason) => {
        if (current) {
          setPreview(null);
          setError(String(reason));
        }
      })
      .finally(() => {
        if (current) setLoading(false);
      });
    listClassActionDrafts(snapshot.class.id, 20)
      .then((items) => {
        if (current) {
          setDrafts(items.filter((item) => item.node_metric_public_id === node.public_id));
        }
      })
      .catch((reason) => {
        if (current) setError(String(reason));
      });
    return () => {
      current = false;
    };
  }, [snapshot.public_id, node.public_id, actionKind]);

  const toggleTarget = (studentId: number) => {
    setSelectedTargets((current) => {
      const next = new Set(current);
      if (next.has(studentId)) next.delete(studentId);
      else next.add(studentId);
      return next;
    });
  };

  const toggleCandidate = (publicId: string) => {
    setSelectedCandidates((current) => {
      const next = new Set(current);
      if (next.has(publicId)) next.delete(publicId);
      else next.add(publicId);
      return next;
    });
  };

  const materializeDraft = async (draft: ClassActionDraft) => {
    setLoading(true);
    setError("");
    try {
      const materialized = await materializeClassAction(
        newRequestKey("class-action-materialize"),
        draft.public_id,
      );
      setSavedDraft(materialized);
      await reloadDrafts();
    } catch (reason) {
      setSavedDraft(draft);
      setError(`行动草稿已保存，但练习作业尚未建立：${String(reason)}`);
    } finally {
      setLoading(false);
    }
  };

  const saveAction = async () => {
    if (!preview) return;
    setLoading(true);
    setError("");
    try {
      const draft = await confirmClassAction({
        requestKey: newRequestKey("class-action-confirm"),
        snapshotPublicId: preview.snapshot_public_id,
        nodeMetricPublicId: preview.node_metric_public_id,
        actionKind,
        expectedSnapshotPayloadSha256: preview.snapshot_payload_sha256,
        title: title.trim(),
        rationale: rationale.trim(),
        estimatedMinutes,
        targetStudentIds: Array.from(selectedTargets),
        candidateQuestionVersionPublicIds: Array.from(selectedCandidates),
      });
      setSavedDraft(draft);
      if (actionKind === "practice") {
        setLoading(false);
        await materializeDraft(draft);
        return;
      }
      await reloadDrafts();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  };

  const selectedTargetCount = selectedTargets.size;
  const canSave = Boolean(
    preview?.can_confirm
      && title.trim()
      && rationale.trim()
      && estimatedMinutes >= 1
      && estimatedMinutes <= 240
      && selectedTargetCount > 0
      && (actionKind !== "temporary_group" || selectedTargetCount >= 2)
      && (actionKind !== "practice" || selectedCandidates.size > 0),
  );

  return (
    <section className="class-action-builder">
      <div className="dashboard-panel-head">
        <div>
          <b>下一步教学行动</b>
          <span>基于当前不可变快照生成；老师可修改，确认前不写任务</span>
        </div>
        <span className="tag">来源：{node.target_title}</span>
      </div>

      <div className="class-action-kind-list">
        {CLASS_ACTION_TYPES.map((item) => (
          <button
            key={item.value}
            className={actionKind === item.value ? "active" : ""}
            onClick={() => setActionKind(item.value)}
          >
            <b>{item.label}</b>
            <small>{item.hint}</small>
          </button>
        ))}
      </div>

      {loading && !preview && <div className="loading">正在核对快照、学生和题库版本…</div>}
      {error && <div className="error">{error}</div>}

      {preview && (
        <>
          {preview.blockers.map((blocker) => (
            <div className="warn" key={blocker}>{blocker}</div>
          ))}
          {preview.warnings.map((warning) => (
            <div className="dashboard-rule-note" key={warning}>{warning}</div>
          ))}

          <div className="class-action-fields">
            <label className="wide">
              <span>行动标题</span>
              <input
                value={title}
                maxLength={100}
                onChange={(event) => setTitle(event.target.value)}
              />
            </label>
            <label>
              <span>预计用时</span>
              <div className="class-action-minutes">
                <input
                  type="number"
                  min={1}
                  max={240}
                  value={estimatedMinutes}
                  onChange={(event) => setEstimatedMinutes(Number(event.target.value))}
                />
                <em>分钟</em>
              </div>
            </label>
            <label className="wide">
              <span>为什么这样安排</span>
              <textarea
                value={rationale}
                maxLength={1000}
                onChange={(event) => setRationale(event.target.value)}
              />
            </label>
          </div>

          <div className="class-action-selection">
            <section>
              <div className="dashboard-panel-head">
                <div>
                  <b>影响学生</b>
                  <span>默认只勾选当前快照中需要支持的学生；可由老师调整</span>
                </div>
                <span>{selectedTargetCount} 人</span>
              </div>
              <div className="class-action-check-list">
                {preview.targets.map((target) => (
                  <label key={target.student.id}>
                    <input
                      type="checkbox"
                      checked={selectedTargets.has(target.student.id)}
                      onChange={() => toggleTarget(target.student.id)}
                    />
                    <b>{target.student.student_no}号 {target.student.name}</b>
                    <span className={profileCellClass(target.source_status)}>
                      {PROFILE_STATUS[target.source_status] ?? target.source_status}
                    </span>
                  </label>
                ))}
              </div>
            </section>

            {actionKind === "practice" && (
              <section>
                <div className="dashboard-panel-head">
                  <div>
                    <b>练习题目</b>
                    <span>只列出已发布 L2+ 且链接由老师确认的固定版本</span>
                  </div>
                  <span>{selectedCandidates.size} 题</span>
                </div>
                <div className="class-action-check-list questions">
                  {preview.candidates.map((candidate) => (
                    <label key={candidate.question_version_public_id}>
                      <input
                        type="checkbox"
                        checked={selectedCandidates.has(candidate.question_version_public_id)}
                        onChange={() => toggleCandidate(candidate.question_version_public_id)}
                      />
                      <div>
                        <b>{candidate.title}</b>
                        <small>
                          {candidate.question_type} · {candidate.max_score} 分
                          {candidate.active_assignment_count > 0
                            ? ` · 当前已有 ${candidate.active_assignment_count} 份作业使用`
                            : ""}
                        </small>
                      </div>
                    </label>
                  ))}
                </div>
              </section>
            )}
          </div>

          <div className="dashboard-rule-note">{preview.boundary_note}</div>
          <div className="class-action-primary">
            {actionKind === "recitation" && (
              <button onClick={() => onNavigate("recitation", "today")}>去背诵内容与布置</button>
            )}
            <button
              className="primary"
              disabled={!canSave || loading}
              onClick={saveAction}
            >
              {loading
                ? "正在确认…"
                : actionKind === "practice"
                  ? "确认并建立练习"
                  : "确认行动草稿"}
            </button>
          </div>
        </>
      )}

      {savedDraft && (
        <div className="class-action-success">
          <div>
            <b>{savedDraft.title}</b>
            <span>
              已冻结 {savedDraft.targets.length} 名学生
              {savedDraft.candidates.length > 0 ? `、${savedDraft.candidates.length} 道题` : ""}
            </span>
          </div>
          {savedDraft.action_kind === "practice" && !savedDraft.materialization && (
            <button disabled={loading} onClick={() => materializeDraft(savedDraft)}>
              重试建立作业
            </button>
          )}
          {savedDraft.materialization && (
            <button className="primary" onClick={() => onNavigate("exam", "exam")}>去作业台</button>
          )}
        </div>
      )}

      {drafts.length > 0 && (
        <div className="class-action-history">
          <div className="dashboard-panel-head">
            <div><b>本节点最近行动</b><span>保留来源快照、目标学生和内容版本</span></div>
          </div>
          {drafts.map((draft) => (
            <div key={draft.public_id}>
              <span className="tag">
                {CLASS_ACTION_TYPES.find((item) => item.value === draft.action_kind)?.label}
              </span>
              <div>
                <b>{draft.title}</b>
                <small>
                  {draft.targets.length} 人
                  {draft.candidates.length > 0 ? ` · ${draft.candidates.length} 题` : ""}
                  {" · "}{new Date(draft.confirmed_at).toLocaleString()}
                </small>
              </div>
              {draft.action_kind === "practice" && (
                draft.materialization ? (
                  <button className="link" onClick={() => onNavigate("exam", "exam")}>看作业</button>
                ) : (
                  <button className="link" disabled={loading} onClick={() => materializeDraft(draft)}>
                    建立作业
                  </button>
                )
              )}
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
