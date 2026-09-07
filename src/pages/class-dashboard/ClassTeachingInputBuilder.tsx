import {
  ClassProfileSnapshot,
  ClassTeachingInputPreview,
  ClassTeachingInputDraft,
  listClassTeachingInputs,
  previewClassTeachingInput,
  confirmClassTeachingInput,
} from "../../api/classDashboard";
import { useState, useEffect } from "react";
import { newRequestKey } from "./shared";

export function ClassTeachingInputBuilder({
  snapshot,
  commonNodeCount,
  onClose,
}: {
  snapshot: ClassProfileSnapshot;
  commonNodeCount: number;
  onClose: () => void;
}) {
  const [preview, setPreview] = useState<ClassTeachingInputPreview | null>(null);
  const [drafts, setDrafts] = useState<ClassTeachingInputDraft[]>([]);
  const [savedDraft, setSavedDraft] = useState<ClassTeachingInputDraft | null>(null);
  const [title, setTitle] = useState("");
  const [teachingNote, setTeachingNote] = useState("");
  const [estimatedMinutes, setEstimatedMinutes] = useState(20);
  const [selectedNodes, setSelectedNodes] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  const reloadDrafts = async () => {
    const items = await listClassTeachingInputs(snapshot.class.id, 10);
    setDrafts(items);
  };

  useEffect(() => {
    let current = true;
    setLoading(true);
    setError("");
    setSavedDraft(null);
    previewClassTeachingInput(snapshot.public_id)
      .then((value) => {
        if (!current) return;
        setPreview(value);
        setTitle(value.suggested_title);
        setTeachingNote(value.suggested_teaching_note);
        setEstimatedMinutes(value.suggested_estimated_minutes);
        setSelectedNodes(new Set(value.items.map((item) => item.node_metric_public_id)));
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
    listClassTeachingInputs(snapshot.class.id, 10)
      .then((items) => {
        if (current) setDrafts(items);
      })
      .catch((reason) => {
        if (current) setError(String(reason));
      });
    return () => {
      current = false;
    };
  }, [snapshot.public_id, snapshot.class.id]);

  const toggleNode = (publicId: string) => {
    setSelectedNodes((current) => {
      const next = new Set(current);
      if (next.has(publicId)) next.delete(publicId);
      else next.add(publicId);
      return next;
    });
  };

  const saveTeachingInput = async () => {
    if (!preview) return;
    setLoading(true);
    setError("");
    try {
      const value = await confirmClassTeachingInput({
        requestKey: newRequestKey("class-teaching-input"),
        snapshotPublicId: preview.snapshot_public_id,
        expectedSnapshotPayloadSha256: preview.snapshot_payload_sha256,
        title: title.trim(),
        teachingNote: teachingNote.trim(),
        estimatedMinutes,
        selectedNodeMetricPublicIds: Array.from(selectedNodes),
      });
      setSavedDraft(value);
      await reloadDrafts();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  };

  const canSave = Boolean(
    preview?.can_confirm
      && title.trim()
      && teachingNote.trim()
      && estimatedMinutes >= 1
      && estimatedMinutes <= 240
      && selectedNodes.size > 0,
  );

  return (
    <section className="class-teaching-input-builder">
      <div className="dashboard-panel-head">
        <div>
          <b>本次教学重点</b>
          <span>
            系统已从当前班级快照整理 {commonNodeCount} 个共同支持节点；老师确认前可删减和改写
          </span>
        </div>
        <button onClick={onClose}>关闭</button>
      </div>

      {loading && !preview && <div className="loading">正在核对当前快照和班级分母…</div>}
      {error && <div className="error">{error}</div>}
      {preview && (
        <>
          {preview.blockers.map((blocker) => (
            <div className="warn" key={blocker}>{blocker}</div>
          ))}
          {preview.warnings.map((warning) => (
            <div className="dashboard-rule-note" key={warning}>{warning}</div>
          ))}

          <div className="class-teaching-input-fields">
            <label>
              <span>标题</span>
              <input
                value={title}
                maxLength={100}
                onChange={(event) => setTitle(event.target.value)}
              />
            </label>
            <label>
              <span>预计课堂用时</span>
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
              <span>课堂说明</span>
              <textarea
                value={teachingNote}
                maxLength={4000}
                onChange={(event) => setTeachingNote(event.target.value)}
              />
            </label>
          </div>

          <div className="class-teaching-input-items">
            <div className="dashboard-panel-head">
              <div>
                <b>纳入本次教学的节点</b>
                <span>默认全选；未评估或证据不足的节点不会出现在这里</span>
              </div>
              <span>{selectedNodes.size}/{preview.items.length} 项</span>
            </div>
            {preview.items.map((item) => (
              <label key={item.node_metric_public_id}>
                <input
                  type="checkbox"
                  checked={selectedNodes.has(item.node_metric_public_id)}
                  onChange={() => toggleNode(item.node_metric_public_id)}
                />
                <div>
                  <b>{item.target_title}</b>
                  <small>
                    {item.target_type === "knowledge_node" ? "知识点" : "能力项"}
                    {" · "}需要支持 {item.needs_support_count}/{item.eligible_student_count} 人
                    {" · "}合格样本 {item.eligible_student_count}/{item.total_student_count} 人
                  </small>
                </div>
                <span className="tag">可信度 {item.confidence_level}</span>
              </label>
            ))}
          </div>

          <div className="dashboard-rule-note">{preview.denominator_note}</div>
          <div className="dashboard-rule-note">{preview.boundary_note}</div>
          <div className="class-teaching-input-actions">
            <button
              className="primary"
              disabled={!canSave || loading}
              onClick={saveTeachingInput}
            >
              {loading ? "正在确认…" : "确认保存教学重点"}
            </button>
          </div>
        </>
      )}

      {savedDraft && (
        <div className="class-action-success">
          <div>
            <b>{savedDraft.title}</b>
            <span>
              已保存 {savedDraft.items.length} 个课堂重点；仅作为老师教学输入，未布置任务
            </span>
          </div>
        </div>
      )}

      {drafts.length > 0 && (
        <div className="class-teaching-input-history">
          <div className="dashboard-panel-head">
            <div>
              <b>最近保存的教学重点</b>
              <span>每次确认保留当时的班级快照和分母</span>
            </div>
          </div>
          {drafts.slice(0, 5).map((draft) => (
            <div key={draft.public_id}>
              <div>
                <b>{draft.title}</b>
                <small>
                  {draft.items.length} 项 · {draft.estimated_minutes} 分钟
                  {" · "}{new Date(draft.confirmed_at).toLocaleString()}
                </small>
              </div>
              <span className="tag">快照 v{draft.snapshot_revision}</span>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
