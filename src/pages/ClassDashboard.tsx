import { useEffect, useMemo, useState } from "react";
import {
  AppModule,
  ClassProfileNodeMetric,
  ClassProfilePreview,
  ClassProfileSnapshot,
  ClassOperationsDashboard,
  DashboardTargetView,
  generateClassProfile,
  loadClassOperationsDashboard,
  loadLatestClassProfile,
  previewClassProfile,
} from "../api/classDashboard";
import { Class, classesList } from "../api/manage";

interface Props {
  onNavigate: (module: AppModule, view: DashboardTargetView | "students") => void;
  onOpenLearning: () => void;
}

const RECITATION_STATUS: Record<string, string> = {
  not_scheduled: "今日未布置",
  not_submitted: "未交",
  submitted: "已交待处理",
  partial: "部分完成",
  recognition_failed: "识别失败",
  pending_review: "待老师终审",
  completed: "已完成",
};

const EXAM_STATUS: Record<string, string> = {
  not_assigned: "暂无进行中作业",
  not_submitted: "未上传",
  ingesting: "图片处理中",
  grading: "批改中",
  ready_to_publish: "待发布",
  partial: "部分完成",
  published: "已发布",
};

function localDate() {
  const now = new Date();
  const year = now.getFullYear();
  const month = String(now.getMonth() + 1).padStart(2, "0");
  const day = String(now.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function daysBefore(value: string, days: number) {
  const date = new Date(`${value}T12:00:00`);
  date.setDate(date.getDate() - days);
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

const PROFILE_STATUS: Record<string, string> = {
  included: "已纳入",
  missing_snapshot: "无快照",
  scope_mismatch: "范围不符",
  stale_snapshot: "需更新",
  unassessed: "未评估",
  insufficient_evidence: "证据不足",
  needs_support: "需要支持",
  developing: "发展中",
  stable: "较稳定",
};

function profileCellClass(status: string) {
  return `class-profile-cell ${status.replace(/_/g, "-")}`;
}

function tagClass(status: string) {
  if (status === "completed" || status === "published") return "tag pass";
  if (status === "recognition_failed") return "tag fail";
  if (["pending_review", "ready_to_publish", "grading", "ingesting", "partial"].includes(status)) {
    return "tag wait";
  }
  return "tag";
}

export default function ClassDashboard({ onNavigate, onOpenLearning }: Props) {
  const [classes, setClasses] = useState<Class[]>([]);
  const [classId, setClassId] = useState<number | null>(null);
  const [asOfDate, setAsOfDate] = useState(localDate);
  const [dashboard, setDashboard] = useState<ClassOperationsDashboard | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [refreshTick, setRefreshTick] = useState(0);
  const [profileRangeStart, setProfileRangeStart] = useState(() => daysBefore(localDate(), 29));
  const [profileRangeEnd, setProfileRangeEnd] = useState(localDate);
  const [profilePreview, setProfilePreview] = useState<ClassProfilePreview | null>(null);
  const [profileSnapshot, setProfileSnapshot] = useState<ClassProfileSnapshot | null>(null);
  const [profileLoading, setProfileLoading] = useState(false);
  const [profileError, setProfileError] = useState("");
  const [profileView, setProfileView] = useState<"knowledge" | "ability">("knowledge");
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);

  useEffect(() => {
    classesList()
      .then((items) => {
        setClasses(items);
        setClassId((current) => current ?? items[0]?.id ?? null);
        if (items.length === 0) setLoading(false);
      })
      .catch((reason) => {
        setError(String(reason));
        setLoading(false);
      });
  }, []);

  useEffect(() => {
    if (classId === null) {
      setDashboard(null);
      return;
    }
    let current = true;
    setLoading(true);
    setError("");
    loadClassOperationsDashboard(classId, asOfDate)
      .then((value) => {
        if (current) setDashboard(value);
      })
      .catch((reason) => {
        if (current) {
          setDashboard(null);
          setError(String(reason));
        }
      })
      .finally(() => {
        if (current) setLoading(false);
      });
    return () => {
      current = false;
    };
  }, [classId, asOfDate, refreshTick]);

  useEffect(() => {
    if (classId === null) {
      setProfileSnapshot(null);
      setProfilePreview(null);
      return;
    }
    let current = true;
    setProfileLoading(true);
    setProfileError("");
    setProfilePreview(null);
    setSelectedNodeId(null);
    loadLatestClassProfile(classId)
      .then((value) => {
        if (current) setProfileSnapshot(Array.isArray(value) ? null : value);
      })
      .catch((reason) => {
        if (current) {
          setProfileSnapshot(null);
          setProfileError(String(reason));
        }
      })
      .finally(() => {
        if (current) setProfileLoading(false);
      });
    return () => {
      current = false;
    };
  }, [classId]);

  const exceptionCount = useMemo(
    () =>
      (dashboard?.recitation.recognition_failure_count ?? 0)
      + (dashboard?.exam.open_pipeline_issue_count ?? 0),
    [dashboard],
  );

  const profileMetrics = profileView === "knowledge"
    ? profileSnapshot?.knowledge_metrics ?? []
    : profileSnapshot?.ability_metrics ?? [];
  const selectedNode = profileMetrics.find((item) => item.public_id === selectedNodeId) ?? null;
  const commonSupportNodes = profileSnapshot?.knowledge_metrics.filter(
    (item) => item.class_status === "common_needs_support",
  ) ?? [];

  const runProfilePreview = async () => {
    if (classId === null) return;
    setProfileLoading(true);
    setProfileError("");
    try {
      const value = await previewClassProfile({
        classId,
        rangeStart: profileRangeStart,
        rangeEnd: profileRangeEnd,
      });
      setProfilePreview(value);
    } catch (reason) {
      setProfilePreview(null);
      setProfileError(String(reason));
    } finally {
      setProfileLoading(false);
    }
  };

  const confirmProfileGeneration = async () => {
    if (classId === null || !profilePreview) return;
    setProfileLoading(true);
    setProfileError("");
    try {
      const value = await generateClassProfile({
        classId,
        rangeStart: profileRangeStart,
        rangeEnd: profileRangeEnd,
        expectedSourceWatermark: profilePreview.source_watermark,
      });
      setProfileSnapshot(value);
      setProfilePreview(null);
      setSelectedNodeId(null);
    } catch (reason) {
      setProfileError(String(reason));
    } finally {
      setProfileLoading(false);
    }
  };

  if (classes.length === 0 && !loading) {
    return (
      <div className="page class-dashboard">
        <div className="page-head"><h1>班级概览</h1></div>
        <div className="sub">先建立班级并加入学生，系统才有明确、可解释的统计分母。</div>
        {error && <div className="error">{error}</div>}
        <div className="empty-state">
          暂无班级<br />
          <button className="primary" onClick={() => onNavigate("recitation", "students")}>
            去建立班级
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="page class-dashboard">
      <div className="page-head dashboard-head">
        <div>
          <h1>班级概览</h1>
          <div className="sub">运行事实与老师确认生成的掌握快照分区展示；未提交、无快照和证据不足都不解释为能力差。</div>
        </div>
        <div className="dashboard-scope">
          <label>
            <span>班级</span>
            <select value={classId ?? ""} onChange={(event) => setClassId(Number(event.target.value))}>
              {classes.map((item) => <option value={item.id} key={item.id}>{item.name}</option>)}
            </select>
          </label>
          <label>
            <span>背诵日期</span>
            <input type="date" value={asOfDate} onChange={(event) => setAsOfDate(event.target.value)} />
          </label>
          <button onClick={() => setRefreshTick((value) => value + 1)}>刷新</button>
        </div>
      </div>

      {error && <div className="error">{error}</div>}
      {loading && !dashboard && <div className="loading">正在汇总班级运行事实…</div>}
      {dashboard && (
        <>
          <div className="dashboard-context">
            <b>{dashboard.class.name}</b>
            <span>{dashboard.class.enabled_student_count} 名启用学生</span>
            {dashboard.class.term && <span>{dashboard.class.term}</span>}
            {dashboard.class.textbook && <span>{dashboard.class.textbook}</span>}
            <span className="spacer" />
            <span>计算于 {new Date(dashboard.meta.calculated_at).toLocaleString()}</span>
          </div>

          <div className="dashboard-stats">
            <button className="dashboard-stat ok" onClick={() => onNavigate("recitation", "today")}>
              <span>背诵完成</span>
              <b className="num">{dashboard.recitation.completed_student_count}<small> / {dashboard.recitation.expected_student_count} 人</small></b>
              <em>{dashboard.recitation.confirmed_task_count} / {dashboard.recitation.expected_task_count} 项已终审</em>
            </button>
            <button className="dashboard-stat" onClick={() => onNavigate("exam", "exam")}>
              <span>当前作业已上传</span>
              <b className="num">{dashboard.exam.submitted_submission_count}<small> / {dashboard.exam.expected_submission_count} 份</small></b>
              <em>{dashboard.exam.active_assessment_count} 个进行中作业</em>
            </button>
            <button className="dashboard-stat warn" onClick={() => onNavigate("recitation", "today")}>
              <span>背诵待老师终审</span>
              <b className="num">{dashboard.recitation.pending_teacher_review_count}</b>
              <em>{dashboard.recitation.overdue_pending_review_count} 项已经逾期</em>
            </button>
            <button
              className="dashboard-stat bad"
              onClick={() => dashboard.recitation.recognition_failure_count > 0
                ? onNavigate("recitation", "desk")
                : onNavigate("exam", "exam")}
            >
              <span>识别 / 导入异常</span>
              <b className="num">{exceptionCount}</b>
              <em>背诵 {dashboard.recitation.recognition_failure_count} · 作业 {dashboard.exam.open_pipeline_issue_count}</em>
            </button>
            <button className="dashboard-stat warn" onClick={() => onNavigate("exam", "exam")}>
              <span>作业待发布</span>
              <b className="num">{dashboard.exam.ready_to_publish_attempt_count}</b>
              <em>成绩必须由老师显式发布</em>
            </button>
          </div>

          <div className="dashboard-grid">
            <section className="dashboard-panel">
              <div className="dashboard-panel-head">
                <div>
                  <b>现在需要处理</b>
                  <span>按风险排序，点击后回到原工作台处理</span>
                </div>
                <span className="tag">{dashboard.actions.length} 类</span>
              </div>
              {dashboard.actions.length === 0 ? (
                <div className="empty-state compact">当前没有待处理项。</div>
              ) : (
                <div className="dashboard-actions">
                  {dashboard.actions.map((action) => (
                    <button
                      key={action.kind}
                      className={`dashboard-action ${action.severity}`}
                      onClick={() => onNavigate(action.target_module, action.target_view)}
                    >
                      <span className="num">{action.count}</span>
                      <div><b>{action.title}</b><em>{action.detail}</em></div>
                      <i>去处理 →</i>
                    </button>
                  ))}
                </div>
              )}
            </section>

            <section className="dashboard-panel dashboard-rules">
              <div className="dashboard-panel-head"><b>本页统计口径</b></div>
              <div>
                <span>背诵</span>
                <p>{dashboard.recitation.denominator_note}</p>
              </div>
              <div>
                <span>作业</span>
                <p>{dashboard.exam.denominator_note}</p>
              </div>
              <div className="dashboard-rule-note">
                未提交、识别失败和证据不足都不是“能力差”；本页不读取或展示旧的 mastery 聚合。
              </div>
            </section>
          </div>

          <section className="dashboard-panel class-profile-panel">
            <div className="dashboard-panel-head class-profile-head">
              <div>
                <b>班级掌握快照</b>
                <span>只汇总最新、范围一致且未过期的个人快照；不自动生成、不做学生排名</span>
              </div>
              <div className="class-profile-scope">
                <label>
                  <span>开始</span>
                  <input
                    type="date"
                    value={profileRangeStart}
                    onChange={(event) => {
                      setProfileRangeStart(event.target.value);
                      setProfilePreview(null);
                    }}
                  />
                </label>
                <label>
                  <span>结束</span>
                  <input
                    type="date"
                    value={profileRangeEnd}
                    onChange={(event) => {
                      setProfileRangeEnd(event.target.value);
                      setProfilePreview(null);
                    }}
                  />
                </label>
                <button onClick={runProfilePreview} disabled={profileLoading || classId === null}>
                  {profileLoading ? "正在检查…" : "预览班级掌握"}
                </button>
              </div>
            </div>

            {profileError && <div className="error">{profileError}</div>}

            {profilePreview && (
              <div className="class-profile-preview">
                <div>
                  <b>生成前确认</b>
                  <span>{profilePreview.range_start} 至 {profilePreview.range_end}</span>
                </div>
                <div className="class-profile-preview-counts">
                  <span>有当前快照 <b>{profilePreview.counts.snapshot_student_count}/{profilePreview.counts.total_student_count}</b> 人</span>
                  <span>至少一个节点达门槛 <b>{profilePreview.counts.eligible_student_count}/{profilePreview.counts.total_student_count}</b> 人</span>
                  <span>无快照 <b>{profilePreview.counts.missing_snapshot_count}</b></span>
                  <span>范围不符 <b>{profilePreview.counts.scope_mismatch_count}</b></span>
                  <span>需更新 <b>{profilePreview.counts.stale_snapshot_count}</b></span>
                </div>
                <p>{profilePreview.denominator_note}</p>
                {profilePreview.blocker && <div className="warn">{profilePreview.blocker}</div>}
                <div className="class-profile-preview-actions">
                  <button className="link" onClick={onOpenLearning}>去生成或更新个人快照</button>
                  <button
                    className="primary"
                    disabled={!profilePreview.can_generate || profileLoading}
                    onClick={confirmProfileGeneration}
                  >
                    确认生成班级快照
                  </button>
                </div>
              </div>
            )}

            {!profileSnapshot && !profileLoading && !profilePreview && (
              <div className="empty-state compact">
                尚未生成班级掌握快照。先检查范围，系统只会汇总已有个人快照。
              </div>
            )}

            {profileSnapshot && (
              <>
                {profileSnapshot.is_stale && (
                  <div className="warn class-profile-stale">
                    {profileSnapshot.stale_reason ?? "班级掌握快照已有新输入，建议重新生成。"}
                  </div>
                )}
                <div className="class-profile-summary">
                  <div>
                    <span>个人快照覆盖</span>
                    <b>{profileSnapshot.snapshot_student_count}/{profileSnapshot.total_student_count} 人</b>
                  </div>
                  <div>
                    <span>至少一个节点达个人门槛</span>
                    <b>{profileSnapshot.eligible_student_count}/{profileSnapshot.total_student_count} 人</b>
                  </div>
                  <div>
                    <span>知识节点样本充足</span>
                    <b>{profileSnapshot.knowledge_node_sample_sufficient}/{profileSnapshot.knowledge_node_total}</b>
                  </div>
                  <div>
                    <span>数据范围</span>
                    <b>{profileSnapshot.range_start} 至 {profileSnapshot.range_end}</b>
                  </div>
                </div>

                <div className="class-profile-grid">
                  <section className="class-profile-common">
                    <div className="dashboard-panel-head">
                      <div>
                        <b>全班共同需要支持</b>
                        <span>按知识点顺序展示，不是学生或知识点排行榜</span>
                      </div>
                    </div>
                    {commonSupportNodes.length === 0 ? (
                      <div className="empty-state compact">
                        当前没有达到班级双门槛的共同薄弱结论；可能是表现尚可，也可能是样本不足。
                      </div>
                    ) : commonSupportNodes.map((node) => (
                      <button
                        className="class-profile-common-item"
                        key={node.public_id}
                        onClick={() => {
                          setProfileView("knowledge");
                          setSelectedNodeId(node.public_id);
                        }}
                      >
                        <b>{node.target_title}</b>
                        <span>合格样本 {node.eligible_student_count}/{node.total_student_count} 人</span>
                        <span>需要支持 {node.needs_support_count}/{node.eligible_student_count} 人</span>
                      </button>
                    ))}
                  </section>

                  <section className="class-profile-inputs">
                    <div className="dashboard-panel-head">
                      <div>
                        <b>个人快照输入</b>
                        <span>缺失原因保留在分母中</span>
                      </div>
                    </div>
                    <div className="class-profile-input-list">
                      {profileSnapshot.inputs.map((item) => (
                        <div key={item.student.id}>
                          <b>{item.student.student_no}号 {item.student.name}</b>
                          <span className={profileCellClass(item.inclusion_status)}>
                            {PROFILE_STATUS[item.inclusion_status] ?? item.inclusion_status}
                          </span>
                          <small>{item.detail}</small>
                        </div>
                      ))}
                    </div>
                  </section>
                </div>

                <div className="class-profile-toolbar">
                  <div className="segmented">
                    <button
                      className={profileView === "knowledge" ? "active" : ""}
                      onClick={() => {
                        setProfileView("knowledge");
                        setSelectedNodeId(null);
                      }}
                    >
                      知识点
                    </button>
                    <button
                      className={profileView === "ability" ? "active" : ""}
                      onClick={() => {
                        setProfileView("ability");
                        setSelectedNodeId(null);
                      }}
                    >
                      能力维度
                    </button>
                  </div>
                  <span>点击列标题查看分母；颜色只作辅助，每格都有文字</span>
                </div>

                {profileMetrics.length === 0 ? (
                  <div className="empty-state compact">当前范围没有可展示的正式节点。</div>
                ) : (
                  <div className="class-profile-heatmap-wrap">
                    <table className="class-profile-heatmap">
                      <thead>
                        <tr>
                          <th>学生</th>
                          {profileMetrics.map((node) => (
                            <th key={node.public_id}>
                              <button onClick={() => setSelectedNodeId(node.public_id)}>
                                {node.target_title}
                                <small>合格 {node.eligible_student_count}/{node.total_student_count}</small>
                              </button>
                            </th>
                          ))}
                        </tr>
                      </thead>
                      <tbody>
                        {profileSnapshot.inputs.map((input) => (
                          <tr key={input.student.id}>
                            <th>{input.student.student_no}号 {input.student.name}</th>
                            {profileMetrics.map((node) => {
                              const cell = node.cells.find((item) => item.student.id === input.student.id);
                              const status = cell?.status ?? input.inclusion_status;
                              return (
                                <td key={node.public_id}>
                                  <span className={profileCellClass(status)}>
                                    {PROFILE_STATUS[status] ?? status}
                                  </span>
                                </td>
                              );
                            })}
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                )}

                {selectedNode && (
                  <ClassProfileNodeDetails
                    node={selectedNode}
                    onClose={() => setSelectedNodeId(null)}
                    onOpenLearning={onOpenLearning}
                  />
                )}

                <div className="class-profile-footnote">
                  快照 v{profileSnapshot.revision} · 截至 {new Date(profileSnapshot.evidence_cutoff_at).toLocaleString()}
                  {" · "}生成于 {new Date(profileSnapshot.generated_at).toLocaleString()}
                  {" · "}策略 v{profileSnapshot.policy.revision}
                </div>
              </>
            )}
          </section>

          <section className="dashboard-panel dashboard-students">
            <div className="dashboard-panel-head">
              <div><b>学生运行状态</b><span>便于核对谁没交、谁待终审；这里不做学生能力排名</span></div>
            </div>
            <div className="dashboard-table-wrap">
              <table className="tbl">
                <thead><tr><th>学生</th><th>今日背诵</th><th>当前作业</th><th>下一步</th></tr></thead>
                <tbody>
                  {dashboard.students.map((student) => (
                    <tr key={student.student_id}>
                      <td>
                        <b>{student.student_name}</b>
                        <span className="dashboard-student-no">{student.student_no}号</span>
                      </td>
                      <td>
                        <span className={tagClass(student.recitation_status)}>
                          {RECITATION_STATUS[student.recitation_status] ?? student.recitation_status}
                        </span>
                        <small className="dashboard-count">
                          {student.recitation_confirmed_task_count}/{student.recitation_due_task_count} 项
                        </small>
                      </td>
                      <td>
                        <span className={tagClass(student.exam_status)}>
                          {EXAM_STATUS[student.exam_status] ?? student.exam_status}
                        </span>
                        <small className="dashboard-count">
                          上传 {student.exam_submitted_submission_count}/{student.exam_expected_submission_count}
                          {" · "}发布 {student.exam_published_submission_count}
                        </small>
                      </td>
                      <td>
                        {["recognition_failed", "submitted", "pending_review", "partial"].includes(student.recitation_status) ? (
                          <button className="link" onClick={() => onNavigate("recitation", student.recitation_status === "recognition_failed" ? "desk" : "today")}>
                            看背诵
                          </button>
                        ) : ["ingesting", "grading", "ready_to_publish", "partial"].includes(student.exam_status) ? (
                          <button className="link" onClick={() => onNavigate("exam", "exam")}>看作业</button>
                        ) : <span className="muted">—</span>}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>
        </>
      )}
    </div>
  );
}

function ClassProfileNodeDetails({
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
