import {
  QuestionVersionImpactPreview,
  QuestionImpactReviewCase,
  QuestionPerformanceCatalog,
  QuestionPerformanceItem,
  QuestionImpactAction,
  QuestionImpactPlan,
  AssessmentDefaultUpgrade,
  loadQuestionPerformance,
  previewQuestionImpact,
  confirmQuestionImpact,
  loadQuestionImpactCases,
  prepareQuestionImpactCases,
  resolveQuestionImpactCase,
  publishQuestionImpactCase,
  upgradeAssessmentDefaultFromImpact,
} from "../../api/knowledge";
import { useState, useEffect } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { TYPE_LABEL } from "./shared";

function newImpactRequestKey() {
  const random = globalThis.crypto?.randomUUID?.()
    ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  return `k1-impact-${random}`;
}

const ASSESSMENT_CONTEXT_LABEL: Record<string, string> = {
  homework: "日常作业",
  quiz: "随堂测验",
  exam: "正式考试",
  practice: "练习",
  correction: "订正",
};

function percentage(value: number | null) {
  return value == null ? "暂无" : `${Math.round(value * 100)}%`;
}

function changeLabels(row: QuestionVersionImpactPreview["rows"][number]) {
  const labels: string[] = [];
  if (row.answerChanged) labels.push("答案");
  if (row.rubricChanged) labels.push("评分点");
  if (row.linkChanged) labels.push("知识链接");
  return labels.join("、") || "无";
}

function readableAnswerJson(raw: string) {
  try {
    const value = JSON.parse(raw) as Record<string, unknown>;
    const labels = value.correct_labels ?? value.correctLabels;
    if (Array.isArray(labels)) return labels.join("、");
    const answer = value.answer ?? value.correct_answer ?? value.correctAnswer;
    if (typeof answer === "string") return answer;
    return JSON.stringify(value, null, 2);
  } catch {
    return raw;
  }
}

type ImpactComponentEditor = {
  teacherScore: number;
  evidenceText: string;
  teacherNote: string;
};

function ImpactReviewCaseCard({
  reviewCase,
  disabled,
  onResolved,
  onPublished,
  onOpenExam,
}: {
  reviewCase: QuestionImpactReviewCase;
  disabled: boolean;
  onResolved: (
    reviewCase: QuestionImpactReviewCase,
    teacherScore: number | null,
    components: Array<{
      sourcePublicId: string;
      teacherScore: number;
      evidenceText: string | null;
      teacherNote: string | null;
    }>,
    note: string,
  ) => Promise<void>;
  onPublished: (reviewCase: QuestionImpactReviewCase) => Promise<void>;
  onOpenExam: () => void;
}) {
  const [teacherScore, setTeacherScore] = useState(
    reviewCase.sourceTeacherScore ?? reviewCase.maxScore,
  );
  const [note, setNote] = useState("");
  const [components, setComponents] = useState<Record<string, ImpactComponentEditor>>(
    Object.fromEntries(reviewCase.targetComponents.map((component) => [
      component.sourcePublicId,
      { teacherScore: 0, evidenceText: "", teacherNote: "" },
    ])),
  );
  const isObjective = reviewCase.targetComponents.length === 0;
  const canResolve = note.trim().length > 0 && (
    isObjective
      ? Number.isFinite(teacherScore)
        && teacherScore >= 0
        && teacherScore <= reviewCase.maxScore
      : reviewCase.targetComponents.every((component) => {
        const editor = components[component.sourcePublicId];
        return editor
          && Number.isFinite(editor.teacherScore)
          && editor.teacherScore >= 0
          && editor.teacherScore <= component.maxScore
          && (editor.teacherScore === 0 || editor.evidenceText.trim().length > 0);
      })
  );
  const submitResolution = () => onResolved(
    reviewCase,
    isObjective ? teacherScore : null,
    reviewCase.targetComponents.map((component) => {
      const editor = components[component.sourcePublicId];
      return {
        sourcePublicId: component.sourcePublicId,
        teacherScore: editor.teacherScore,
        evidenceText: editor.evidenceText.trim() || null,
        teacherNote: editor.teacherNote.trim() || null,
      };
    }),
    note.trim(),
  );

  return (
    <article className={`impact-review-card state-${reviewCase.state}`}>
      <div className="impact-case-head">
        <div>
          <b>{reviewCase.studentNo} · {reviewCase.studentName}</b>
          <span>
            {reviewCase.className} · {reviewCase.assessmentTitle} · 第 {reviewCase.questionNo} 题
          </span>
        </div>
        <span className={reviewCase.caseKind === "published_review" ? "tag warning" : "tag subtle"}>
          {reviewCase.caseKind === "published_review" ? "已发布复核" : "未发布重评"}
        </span>
      </div>
      <div className="impact-case-meta">
        <span>
          旧评分：
          {reviewCase.sourceTeacherScore == null
            ? "暂无"
            : `${reviewCase.sourceTeacherScore} 分（第 ${reviewCase.sourceGradeDecisionRevision} 版）`}
        </span>
        <span>目标答案第 {reviewCase.targetAnswerKeyRevision} 版</span>
        <span>目标评分点第 {reviewCase.targetRubricRevision} 版</span>
        <span>目标知识链接第 {reviewCase.targetLinkSetRevision} 版</span>
      </div>
      <div className="impact-evidence-grid">
        <div>
          <b>题目与目标答案</b>
          <p>{reviewCase.questionStem}</p>
          <pre>{readableAnswerJson(reviewCase.targetAnswerJson)}</pre>
        </div>
        <div>
          <b>学生原作答</b>
          {reviewCase.cropPath && (
            <img
              className="impact-answer-crop"
              src={convertFileSrc(reviewCase.cropPath)}
              alt={`${reviewCase.studentName} 第 ${reviewCase.questionNo} 题原作答`}
            />
          )}
          <pre>{reviewCase.studentResponseText || "当前没有可用的识别文本，请回批改台核对原图。"}</pre>
        </div>
      </div>
      {reviewCase.state === "open" ? (
        <div className="impact-resolution-editor">
          {isObjective ? (
            <label className="field compact">
              <span className="fl">按新答案确认得分（满分 {reviewCase.maxScore}）</span>
              <input
                aria-label={`${reviewCase.studentName} 新评分`}
                type="number"
                min={0}
                max={reviewCase.maxScore}
                step={0.5}
                value={teacherScore}
                onChange={(event) => setTeacherScore(Number(event.target.value))}
              />
            </label>
          ) : (
            <div className="impact-component-list">
              {reviewCase.targetComponents.map((component) => {
                const editor = components[component.sourcePublicId];
                return (
                  <div key={component.sourcePublicId} className="impact-component-row">
                    <div>
                      <b>{component.label}</b>
                      <span>满分 {component.maxScore}</span>
                    </div>
                    <input
                      aria-label={`${component.label} 得分`}
                      type="number"
                      min={0}
                      max={component.maxScore}
                      step={0.5}
                      value={editor.teacherScore}
                      onChange={(event) => setComponents((current) => ({
                        ...current,
                        [component.sourcePublicId]: {
                          ...current[component.sourcePublicId],
                          teacherScore: Number(event.target.value),
                        },
                      }))}
                    />
                    <input
                      aria-label={`${component.label} 学生答案证据`}
                      value={editor.evidenceText}
                      placeholder="给分时粘贴学生答案原句"
                      onChange={(event) => setComponents((current) => ({
                        ...current,
                        [component.sourcePublicId]: {
                          ...current[component.sourcePublicId],
                          evidenceText: event.target.value,
                        },
                      }))}
                    />
                    <input
                      aria-label={`${component.label} 备注`}
                      value={editor.teacherNote}
                      placeholder="可选备注"
                      onChange={(event) => setComponents((current) => ({
                        ...current,
                        [component.sourcePublicId]: {
                          ...current[component.sourcePublicId],
                          teacherNote: event.target.value,
                        },
                      }))}
                    />
                  </div>
                );
              })}
            </div>
          )}
          <label className="field">
            <span className="fl">本次复核说明（必填）</span>
            <input
              value={note}
              maxLength={300}
              placeholder="例如：按修订后的标准答案逐项核对"
              onChange={(event) => setNote(event.target.value)}
            />
          </label>
          <div className="impact-stage-action">
            <span>保存只产生新评分 revision，旧正式成绩和学习证据暂不改变。</span>
            <button
              className="primary"
              data-testid="impact-resolve-case"
              disabled={disabled || !canResolve}
              onClick={submitResolution}
            >
              确认新评分（暂不发布）
            </button>
          </div>
        </div>
      ) : reviewCase.state === "grade_confirmed" ? (
        <div className="impact-stage-action warning-box">
          <span>
            新评分 {reviewCase.resolvedTeacherScore} 分已保存；旧正式成绩仍然有效。
            {reviewCase.attemptState === "ready_to_publish"
              ? " 确认后将发布整份作业的新 revision。"
              : " 整份作业还有其他题待终审。"}
          </span>
          {reviewCase.attemptState === "ready_to_publish" ? (
            <button
              className="primary"
              data-testid="impact-publish-case"
              disabled={disabled}
              onClick={() => onPublished(reviewCase)}
            >
              明确发布整份新成绩
            </button>
          ) : (
            <button disabled={disabled} onClick={onOpenExam}>回批改台完成其余题目</button>
          )}
        </div>
      ) : (
        <div className="ok-banner">
          新评分已经明确发布；旧发布快照保留审计，正式学习证据已按新版本切换。
        </div>
      )}
      <p>{reviewCase.nextStepNote}</p>
    </article>
  );
}

export function QuestionPerformancePanel({ onOpenExam }: { onOpenExam: () => void }) {
  const [catalog, setCatalog] = useState<QuestionPerformanceCatalog | null>(null);
  const [selected, setSelected] = useState<QuestionPerformanceItem | null>(null);
  const [preview, setPreview] = useState<QuestionVersionImpactPreview | null>(null);
  const [action, setAction] = useState<QuestionImpactAction>("future_only");
  const [plan, setPlan] = useState<QuestionImpactPlan | null>(null);
  const [defaultUpgrades, setDefaultUpgrades] = useState<Record<string, AssessmentDefaultUpgrade>>({});
  const [reviewCases, setReviewCases] = useState<QuestionImpactReviewCase[]>([]);
  const [caseBoundary, setCaseBoundary] = useState("");
  const [loading, setLoading] = useState(true);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState("");

  const refresh = async () => {
    setLoading(true);
    setError("");
    try {
      setCatalog(await loadQuestionPerformance(200));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    refresh();
  }, []);

  const openImpact = async (item: QuestionPerformanceItem) => {
    setSelected(item);
    setPreview(null);
    setPlan(null);
    setDefaultUpgrades({});
    setReviewCases([]);
    setCaseBoundary("");
    setAction("future_only");
    setWorking(true);
    setError("");
    try {
      setPreview(await previewQuestionImpact(item.questionVersionPublicId));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  };

  const confirmImpact = async () => {
    if (!preview) return;
    setWorking(true);
    setError("");
    try {
      const nextPlan = await confirmQuestionImpact({
        requestKey: newImpactRequestKey(),
        questionVersionPublicId: preview.questionVersionPublicId,
        expectedPreviewHash: preview.previewHash,
        action,
        plannedBy: "local_teacher",
      });
      setPlan(nextPlan);
      if (nextPlan.taskCount > 0) {
        const existing = await loadQuestionImpactCases(nextPlan.publicId);
        setReviewCases(existing.cases);
        setCaseBoundary(existing.boundaryNote);
      }
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  };

  const prepareCases = async () => {
    if (!plan || plan.taskCount === 0) return;
    setWorking(true);
    setError("");
    try {
      const result = await prepareQuestionImpactCases({
        planPublicId: plan.publicId,
        expectedTaskCount: plan.taskCount,
        preparedBy: "local_teacher",
      });
      setReviewCases(result.cases);
      setCaseBoundary(
        "待处理 case 已冻结旧评分/发布证据与目标版本；当前成绩、发布结果和学习证据均未改变。",
      );
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  };

  const refreshCases = async (planPublicId: string) => {
    const next = await loadQuestionImpactCases(planPublicId);
    setReviewCases(next.cases);
    setCaseBoundary(next.boundaryNote);
  };

  const resolveCase = async (
    reviewCase: QuestionImpactReviewCase,
    teacherScore: number | null,
    components: Array<{
      sourcePublicId: string;
      teacherScore: number;
      evidenceText: string | null;
      teacherNote: string | null;
    }>,
    note: string,
  ) => {
    setWorking(true);
    setError("");
    try {
      await resolveQuestionImpactCase({
        requestKey: `${newImpactRequestKey()}-resolve`,
        casePublicId: reviewCase.publicId,
        expectedSourceSnapshotHash: reviewCase.sourceSnapshotHash,
        teacherScore,
        components,
        teacherNote: note,
        resolvedBy: "local_teacher",
      });
      await refreshCases(reviewCase.planPublicId);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  };

  const publishCase = async (reviewCase: QuestionImpactReviewCase) => {
    if (!reviewCase.resolvedGradeDecisionPublicId) return;
    setWorking(true);
    setError("");
    try {
      await publishQuestionImpactCase({
        casePublicId: reviewCase.publicId,
        expectedGradeDecisionPublicId: reviewCase.resolvedGradeDecisionPublicId,
        publishedBy: "local_teacher",
      });
      await refreshCases(reviewCase.planPublicId);
      await refresh();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  };

  const upgradeFutureDefault = async (row: QuestionVersionImpactPreview["rows"][number]) => {
    if (!plan || !row.isCurrentDefault) return;
    setWorking(true);
    setError("");
    try {
      const upgraded = await upgradeAssessmentDefaultFromImpact({
        requestKey: `${newImpactRequestKey()}-future-default`,
        planPublicId: plan.publicId,
        sourceAssessmentVersionPublicId: row.assessmentVersionPublicId,
        expectedCurrentDefaultVersionPublicId: row.assessmentVersionPublicId,
        upgradedBy: "local_teacher",
      });
      setDefaultUpgrades((current) => ({
        ...current,
        [row.assessmentVersionPublicId]: upgraded,
      }));
      await refresh();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  };

  return (
    <div className="question-performance-page">
      {error && <div className="error">{error}</div>}
      <section className="dashboard-panel question-performance-intro">
        <div className="dashboard-panel-head">
          <div>
            <b>题目实际表现</b>
            <span>只统计当前已发布成绩中老师确认的结果，不把单个班级正确率写成题目永久难度。</span>
          </div>
          <button disabled={loading || working} onClick={refresh}>刷新统计</button>
        </div>
        {catalog && <div className="hint">{catalog.boundaryNote}</div>}
      </section>

      {loading ? (
        <div className="loading">正在汇总题目表现…</div>
      ) : !catalog?.items.length ? (
        <section className="dashboard-panel empty-state">
          <b>还没有可汇总的正式题目</b>
          <span>题目被作业使用后会进入这里；发布老师确认的成绩后才会出现实际得分表现。</span>
        </section>
      ) : (
        <div className="question-performance-grid">
          {catalog.items.map((item) => (
            <article className="question-performance-card" key={item.questionVersionPublicId}>
              <div className="question-performance-card-head">
                <div>
                  <span className="tag">{TYPE_LABEL[item.questionType]}</span>
                  <span className="tag subtle">第 {item.revision} 版 · {item.qualityLevel}</span>
                </div>
                {item.hasVersionUpdateImpact && <span className="tag warning">有新版影响</span>}
              </div>
              <h3>{item.stem}</h3>
              <div className="question-performance-metrics">
                <div><b>{percentage(item.averageScoreRate)}</b><span>平均得分率</span></div>
                <div><b>{item.publishedResponseCount}</b><span>已发布作答</span></div>
                <div><b>{item.assessmentUsageCount}</b><span>使用作业数</span></div>
                <div><b>{percentage(item.fullCreditRate)}</b><span>满分率</span></div>
              </div>
              <div className="performance-score-split">
                <span>满分 {item.fullCreditCount}</span>
                <span>部分得分 {item.partialCreditCount}</span>
                <span>零分 {item.zeroScoreCount}</span>
                <span>首次 {item.firstAttemptCount} / 订正 {item.correctionAttemptCount}</span>
              </div>
              {item.contextBreakdown.length > 0 && (
                <div className="performance-contexts">
                  {item.contextBreakdown.map((context) => (
                    <span key={context.assessmentContext}>
                      {ASSESSMENT_CONTEXT_LABEL[context.assessmentContext] ?? context.assessmentContext}
                      {" "}{context.publishedResponseCount} 份 · {percentage(context.averageScoreRate)}
                    </span>
                  ))}
                </div>
              )}
              {item.latestPublishedAt && (
                <div className="muted">最近正式成绩：{new Date(item.latestPublishedAt).toLocaleString()}</div>
              )}
              {item.hasVersionUpdateImpact ? (
                <button className="primary" disabled={working} onClick={() => openImpact(item)}>
                  查看新版影响
                </button>
              ) : (
                <div className="ok-inline">当前作业均使用最新确认版本</div>
              )}
            </article>
          ))}
        </div>
      )}

      {selected && (
        <section className="dashboard-panel question-impact-panel">
          <div className="dashboard-panel-head">
            <div>
              <b>版本变更影响预览</b>
              <span>{selected.stem}</span>
            </div>
            <button disabled={working} onClick={() => {
              setSelected(null);
              setPreview(null);
              setPlan(null);
              setDefaultUpgrades({});
              setReviewCases([]);
              setCaseBoundary("");
            }}>关闭</button>
          </div>
          {working && !preview ? <div className="loading">正在核对历史使用范围…</div> : preview && (
            <>
              <div className="impact-target">
                <span>目标答案第 {preview.target.answerKeyRevision} 版</span>
                <span>目标评分点第 {preview.target.rubricRevision} 版</span>
                <span>目标知识链接第 {preview.target.linkSetRevision} 版</span>
              </div>
              <div className="question-impact-summary">
                <div><b>{preview.affectedAssessmentCount}</b><span>受影响作业</span></div>
                <div><b>{preview.unpublishedAttemptCount}</b><span>未发布作答</span></div>
                <div><b>{preview.publishedAttemptCount}</b><span>已发布作答</span></div>
                <div><b>{preview.activeLearningEvidenceCount}</b><span>有效学习证据</span></div>
                <div><b>{preview.profileSnapshotCount}</b><span>已有图谱引用</span></div>
              </div>
              <div className="impact-row-list">
                {preview.rows.map((row) => (
                  <article key={row.assessmentItemPublicId}>
                    <div>
                      <b>{row.assessmentTitle}</b>
                      <span>
                        {row.className} · 作业第 {row.assessmentVersionRevision} 版
                        {" · "}变化：{changeLabels(row)}
                      </span>
                    </div>
                    <div>
                      <span>未发布 {row.unpublishedAttemptCount}</span>
                      <span>已发布 {row.publishedAttemptCount}</span>
                      <span>证据 {row.activeLearningEvidenceCount}</span>
                      <span>图谱 {row.profileSnapshotCount}</span>
                    </div>
                    {plan && row.isCurrentDefault && (
                      defaultUpgrades[row.assessmentVersionPublicId] ? (
                        <div className="impact-default-result" data-testid="impact-default-upgraded">
                          <b>
                            未来上传默认第{" "}
                            {defaultUpgrades[row.assessmentVersionPublicId].defaultRevision} 版
                          </b>
                          <span>旧作业仍保留第 {row.assessmentVersionRevision} 版</span>
                        </div>
                      ) : (
                        <button
                          className="primary"
                          data-testid="impact-upgrade-default"
                          disabled={working}
                          onClick={() => upgradeFutureDefault(row)}
                        >
                          生成新版并用于以后上传
                        </button>
                      )
                    )}
                    {plan && !row.isCurrentDefault && (
                      <span className="tag subtle">历史版本保留，不切换</span>
                    )}
                  </article>
                ))}
              </div>
              <div className="impact-actions">
                <label className={action === "future_only" ? "selected" : ""}>
                  <input type="radio" checked={action === "future_only"} onChange={() => setAction("future_only")} />
                  <b>只用于以后新作业</b>
                  <span>不建立历史复核任务</span>
                </label>
                <label className={action === "recalculate_unpublished" ? "selected" : ""}>
                  <input
                    type="radio"
                    checked={action === "recalculate_unpublished"}
                    disabled={preview.unpublishedAttemptCount === 0}
                    onChange={() => setAction("recalculate_unpublished")}
                  />
                  <b>复核未发布作答</b>
                  <span>建立 {preview.unpublishedAttemptCount} 条待重新计算清单</span>
                </label>
                <label className={action === "review_published" ? "selected" : ""}>
                  <input
                    type="radio"
                    checked={action === "review_published"}
                    disabled={preview.publishedAttemptCount === 0}
                    onChange={() => setAction("review_published")}
                  />
                  <b>复核已发布成绩</b>
                  <span>建立 {preview.publishedAttemptCount} 条人工复核清单</span>
                </label>
              </div>
              <div className="warning-box">
                确认只冻结影响计划和待办清单，不会切换作业版本，不会改分、重新发布、改写学习证据或覆盖旧图谱。
              </div>
              {plan ? (
                <>
                  <div className="ok-banner">
                    已冻结处理计划，共 {plan.taskCount} 条待办；本次没有修改任何成绩和学习证据。
                  </div>
                  {preview.rows.some((row) => row.isCurrentDefault) && (
                    <div className="hint">
                      还需对当前默认作业点击“生成新版并用于以后上传”；系统不会把历史作业重绑到新版。
                    </div>
                  )}
                  {plan.taskCount > 0 && reviewCases.length === 0 && (
                    <div className="impact-prepare-row">
                      <div>
                        <b>下一步：建立逐份待处理记录</b>
                        <span>冻结每名学生当前评分或正式发布快照，供老师后续逐条重评。</span>
                      </div>
                      <button
                        className="primary"
                        data-testid="impact-prepare-cases"
                        disabled={working}
                        onClick={prepareCases}
                      >
                        {working ? "正在建立…" : `建立 ${plan.taskCount} 条待处理`}
                      </button>
                    </div>
                  )}
                  {reviewCases.length > 0 && (
                    <div className="impact-case-section" data-testid="impact-case-list">
                      <div className="impact-case-section-head">
                        <b>待处理记录 {reviewCases.length} 条</b>
                        <span>先确认新评分，再明确发布；两个动作不会被系统合并。</span>
                      </div>
                      <div className="impact-case-list">
                        {reviewCases.map((reviewCase) => (
                          <ImpactReviewCaseCard
                            key={reviewCase.publicId}
                            reviewCase={reviewCase}
                            disabled={working}
                            onResolved={resolveCase}
                            onPublished={publishCase}
                            onOpenExam={onOpenExam}
                          />
                        ))}
                      </div>
                      {caseBoundary && <div className="hint">{caseBoundary}</div>}
                    </div>
                  )}
                </>
              ) : (
                <button className="primary impact-confirm" disabled={working} onClick={confirmImpact}>
                  {working ? "正在冻结计划…" : "确认处理方式"}
                </button>
              )}
              <div className="hint">{preview.boundaryNote}</div>
            </>
          )}
        </section>
      )}
    </div>
  );
}
