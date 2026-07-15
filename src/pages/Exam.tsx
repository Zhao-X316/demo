import { useEffect, useMemo, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AnswerDetail,
  FixedIntakeOption,
  FixedIntakeResult,
  KnowledgePoint,
  ObjectiveWorkbench,
  ObjectiveWorkbenchRow,
  Question,
  QuestionInput,
  examAnswerHumanDecide,
  examAnswerSuggest,
  examAnswersList,
  examFixedIntakeConfirmGrouping,
  examFixedIntakeOptions,
  examFixedIntakeConfirmMaterialType,
  examFixedIntakePrepare,
  examObjectiveAccept,
  examObjectiveCorrect,
  examObjectivePublishAttempt,
  examObjectiveRecognizeRegion,
  examObjectiveStrictBatchAccept,
  examObjectiveWorkbench,
  kpCreate,
  kpList,
  questionCreate,
  questionsList,
} from "../api/exam";
import { Student, studentsList } from "../api/manage";

type Tab = "intake" | "objective" | "grade" | "questions" | "knowledge";

const TYPE_LABEL: Record<string, string> = {
  single: "单选",
  multi: "多选",
  judge: "判断",
  fill: "填空",
  subjective: "主观",
};

const OBJECTIVE_TYPE_LABEL: Record<string, string> = {
  single: "单选",
  multiple: "多选",
  true_false: "判断",
};

const OBSERVATION_LABEL: Record<string, string> = {
  recognized: "识别清晰",
  blank: "疑似空白",
  altered: "存在涂改",
  low_confidence: "低置信度",
  failed: "识别失败",
};

const EXCLUSION_LABEL: Record<string, string> = {
  ALREADY_CONFIRMED: "已终审",
  OBSERVATION_NOT_RECOGNIZED: "识别状态不合格",
  LOW_CONFIDENCE: "置信度不足",
  NOT_BATCH_ELIGIBLE: "不满足批量规则",
  UNSCORED: "机器无法确定计分",
};

const INTAKE_REASON_LABEL: Record<string, string> = {
  BATCH_EMPTY: "没有可处理页面",
  IMPORT_SEQUENCE_GAP: "页面顺序不连续",
  SOURCE_DOCUMENT_UNREGISTERED: "原始资料未完整登记",
  PAGE_QUALITY_REVIEW_REQUIRED: "需要检查页面清晰度",
  PAGE_IDENTITY_UNCONFIRMED: "等待确认学生和页码",
  PAGE_NUMBER_CONFLICT: "页码存在冲突",
  PAGE_SEQUENCE_INVALID: "同一学生页序异常",
  STUDENT_PAGES_NONCONTIGUOUS: "同一学生页面未连续排列",
  GROUP_FIRST_PAGE_INVALID: "学生卷未从第 1 页开始",
  GROUP_PAGE_COUNT_MISMATCH: "学生卷页数与设置不一致",
  ANSWER_AUTHORITY_MISSING: "等待确认标准答案",
  ANSWER_CANDIDATE_INVALID: "答案资料需要修正",
  ANSWER_CANDIDATE_DRIFT: "答案版本发生变化",
  ANSWER_SAME_LEVEL_CONFLICT: "答案资料存在分歧",
  ANSWER_REGION_MISSING: "等待定位答题区域",
  ANSWER_REGION_AMBIGUOUS: "答题区域不唯一",
  OBJECTIVE_RESULT_REVIEW_REQUIRED: "识别结果需要老师复核",
  ANSWER_VERSION_DRIFT: "识别使用的答案版本已变化",
  REVIEW_INPUT_PRESENT: "本批存在需复核资料",
  DUPLICATE_PAGE_ARTIFACT: "检测到重复页面",
  STUDENT_GROUP_EXCEEDS_ROSTER: "照片组数超过班级人数",
  STUDENT_RANGE_OR_ABSENCE_CONFIRMATION_REQUIRED: "需确认本批学号范围或缺交学生",
  MATERIAL_TYPE_CONFIRMATION_REQUIRED: "需确认普通试卷、答题卡或默写",
  PAGE_TYPE_CYCLE_UNVERIFIED: "多页卷需要确认页面周期",
  PAGE_TYPE_CYCLE_MISMATCH: "页面周期中疑似缺页或错序",
  ORDER_EVIDENCE_CONFLICT: "文件名顺序与拍摄时间存在分歧",
  CAPTURE_TIME_ORDER_CONFLICT: "拍摄时间与文件名顺序冲突",
  FILE_TIME_ORDER_CONFLICT: "文件时间与文件名顺序冲突",
  FILENAME_NATURAL_TIE: "存在无法单靠文件名区分的照片",
};

const MATERIAL_TYPE_LABEL: Record<string, string> = {
  ordinary_paper: "普通试卷",
  answer_sheet: "答题卡",
  dictation: "默写",
  unknown: "待确认",
};

const STUDENT_FILE_EXTENSIONS = ["jpg", "jpeg", "pdf"];
const ANSWER_FILE_EXTENSIONS = ["jpg", "jpeg", "pdf", "txt"];

function hasExtension(path: string, extensions: string[]) {
  const clean = path.split(/[?#]/, 1)[0].toLowerCase();
  return extensions.some((extension) => clean.endsWith(`.${extension}`));
}

function fileName(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

function displayTime(value: string) {
  return value.replace("T", " ").slice(0, 16);
}

export default function Exam() {
  const [tab, setTab] = useState<Tab>("intake");
  const [students, setStudents] = useState<Student[]>([]);
  const [questions, setQuestions] = useState<Question[]>([]);
  const [knowledge, setKnowledge] = useState<KnowledgePoint[]>([]);
  const [answers, setAnswers] = useState<AnswerDetail[]>([]);
  const [objectiveWorkbench, setObjectiveWorkbench] = useState<ObjectiveWorkbench>({ rows: [], attempts: [] });
  const [intakeOptions, setIntakeOptions] = useState<FixedIntakeOption[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [toast, setToast] = useState("");

  const load = async () => {
    setError("");
    try {
      const [studentRows, questionRows, kpRows, answerRows, workbench, fixedOptions] = await Promise.all([
        studentsList(),
        questionsList(),
        kpList(),
        examAnswersList(),
        examObjectiveWorkbench(),
        examFixedIntakeOptions(),
      ]);
      setStudents(studentRows.filter((student) => student.enabled));
      setQuestions(questionRows.filter((question) => question.enabled));
      setKnowledge(kpRows);
      setAnswers(answerRows);
      setObjectiveWorkbench(workbench);
      setIntakeOptions(fixedOptions);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    load();
  }, []);

  const done = (message: string) => {
    setToast(message);
    setError("");
    load();
  };

  return (
    <div className="page exam-page">
      <div className="page-head">
        <div>
          <h1>题目批改</h1>
          <div className="sub">选择班级和作业，上传照片或 PDF；系统整理证据，老师只处理分歧。</div>
        </div>
        <span className="tag makeup">M2 · 一站式批改</span>
      </div>
      <div className="exam-notice">
        <b>老师保留终审权</b>
        <span>上传只归档和整理资料；机器结果不会自动计分或发布，模糊、涂改和分歧统一交给老师确认。</span>
      </div>
      <div className="tabs">
        <button className={tab === "intake" ? "tab active" : "tab"} onClick={() => setTab("intake")}>上传批改</button>
        <button className={tab === "objective" ? "tab active" : "tab"} onClick={() => setTab("objective")}>标准卷终审</button>
        <button className={tab === "grade" ? "tab active" : "tab"} onClick={() => setTab("grade")}>老师补录</button>
        <button className={tab === "questions" ? "tab active" : "tab"} onClick={() => setTab("questions")}>题库</button>
        <button className={tab === "knowledge" ? "tab active" : "tab"} onClick={() => setTab("knowledge")}>知识点</button>
      </div>
      {error && <div className="error">{error}</div>}
      {toast && <div className="ok-banner">{toast}</div>}
      {loading ? <div className="loading">加载题目批改数据…</div> : (
        <>
          {tab === "intake" && (
            <FixedIntakeTab
              options={intakeOptions}
              onOpenReview={() => setTab("objective")}
              onError={(message) => setError(message)}
            />
          )}
          {tab === "objective" && (
            <ObjectiveReviewTab
              workbench={objectiveWorkbench}
              onDone={done}
              onError={(message) => setError(message)}
            />
          )}
          {tab === "grade" && (
            <GradeTab
              students={students}
              questions={questions}
              answers={answers}
              onDone={done}
              onError={(message) => setError(message)}
            />
          )}
          {tab === "questions" && (
            <QuestionTab
              questions={questions}
              knowledge={knowledge}
              onDone={done}
              onError={(message) => setError(message)}
            />
          )}
          {tab === "knowledge" && (
            <KnowledgeTab
              knowledge={knowledge}
              onDone={done}
              onError={(message) => setError(message)}
            />
          )}
        </>
      )}
    </div>
  );
}

function FixedIntakeTab({
  options,
  onOpenReview,
  onError,
}: {
  options: FixedIntakeOption[];
  onOpenReview: () => void;
  onError: (message: string) => void;
}) {
  const classOptions = useMemo(() => {
    const unique = new Map<number, string>();
    options.forEach((option) => unique.set(option.classId, option.className));
    return Array.from(unique, ([id, name]) => ({ id, name }));
  }, [options]);
  const [classId, setClassId] = useState(classOptions[0]?.id ?? 0);
  const assessmentOptions = options.filter((option) => option.classId === classId);
  const [assessmentVersionId, setAssessmentVersionId] = useState(
    assessmentOptions[0]?.assessmentVersionId ?? 0,
  );
  const [studentPaths, setStudentPaths] = useState<string[]>([]);
  const [answerPath, setAnswerPath] = useState<string | null>(null);
  const [answerText, setAnswerText] = useState("");
  const [expectedPages, setExpectedPages] = useState("1");
  const [busy, setBusy] = useState(false);
  const [confirmingType, setConfirmingType] = useState(false);
  const [confirmingGrouping, setConfirmingGrouping] = useState(false);
  const [groupingStartNo, setGroupingStartNo] = useState("");
  const [absentStudentNos, setAbsentStudentNos] = useState<string[]>([]);
  const [result, setResult] = useState<FixedIntakeResult | null>(null);
  const [requestKey, setRequestKey] = useState("");

  useEffect(() => {
    if (!classOptions.some((option) => option.id === classId)) {
      setClassId(classOptions[0]?.id ?? 0);
    }
  }, [classId, classOptions]);

  useEffect(() => {
    if (!assessmentOptions.some((option) => option.assessmentVersionId === assessmentVersionId)) {
      setAssessmentVersionId(assessmentOptions[0]?.assessmentVersionId ?? 0);
    }
  }, [assessmentOptions, assessmentVersionId]);

  const resetRequest = () => {
    setRequestKey("");
    setResult(null);
    setGroupingStartNo("");
    setAbsentStudentNos([]);
  };

  const pickStudentPapers = async () => {
    try {
      const selected = await open({ multiple: true });
      if (!selected) return;
      const paths = Array.isArray(selected) ? selected : [selected];
      const unsupported = paths.filter((path) => !hasExtension(path, STUDENT_FILE_EXTENSIONS));
      if (unsupported.length) {
        onError(`学生试卷只支持 JPG、JPEG 或 PDF；有 ${unsupported.length} 个文件未加入。`);
        return;
      }
      setStudentPaths(paths);
      resetRequest();
    } catch (err) {
      onError(`选择学生试卷失败：${String(err)}`);
    }
  };

  const pickAnswer = async () => {
    try {
      const selected = await open({ multiple: false });
      if (!selected || Array.isArray(selected)) return;
      if (!hasExtension(selected, ANSWER_FILE_EXTENSIONS)) {
        onError("答案资料只支持 JPG、JPEG、PDF 或 TXT。");
        return;
      }
      setAnswerPath(selected);
      setAnswerText("");
      resetRequest();
    } catch (err) {
      onError(`选择答案资料失败：${String(err)}`);
    }
  };

  const submit = async () => {
    const pageCount = Number(expectedPages);
    if (!assessmentVersionId) {
      onError("请先选择要批改的作业");
      return;
    }
    if (!studentPaths.length) {
      onError("请至少上传一份学生试卷");
      return;
    }
    if (!Number.isInteger(pageCount) || pageCount < 1) {
      onError("每名学生的页数必须是大于 0 的整数");
      return;
    }
    const currentKey = requestKey || crypto.randomUUID();
    setRequestKey(currentKey);
    setBusy(true);
    try {
      const prepared = await examFixedIntakePrepare({
        assessmentVersionId,
        studentPaths,
        answerPath,
        answerText: answerText.trim() || null,
        expectedPagesPerAttempt: pageCount,
        materialType: "auto",
        idempotencyKey: currentKey,
      });
      setResult(prepared);
      setGroupingStartNo(
        prepared.groupingFirstStudentNo
          || prepared.groupingRoster[0]?.studentNo
          || "",
      );
      setAbsentStudentNos([]);
      setRequestKey("");
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const confirmGrouping = async () => {
    if (!result || !groupingStartNo) return;
    setConfirmingGrouping(true);
    try {
      const confirmed = await examFixedIntakeConfirmGrouping(
        result.batchId,
        groupingStartNo,
        absentStudentNos,
      );
      setResult({ ...result, ...confirmed });
    } catch (err) {
      onError(String(err));
    } finally {
      setConfirmingGrouping(false);
    }
  };

  const toggleAbsentStudent = (studentNo: string) => {
    setAbsentStudentNos((current) => current.includes(studentNo)
      ? current.filter((value) => value !== studentNo)
      : [...current, studentNo]);
  };

  const confirmMaterialType = async (
    materialType: "ordinary_paper" | "answer_sheet" | "dictation",
  ) => {
    if (!result) return;
    setConfirmingType(true);
    try {
      const confirmed = await examFixedIntakeConfirmMaterialType(result.batchId, materialType);
      setResult({
        ...result,
        ...confirmed,
        materialTypeNeedsConfirmation: false,
      });
    } catch (err) {
      onError(String(err));
    } finally {
      setConfirmingType(false);
    }
  };

  if (!options.length) {
    return (
      <div className="exam-card objective-empty">
        <b>还没有可上传的固定卷作业</b>
        <span className="muted">需先准备一份已确认、带客观题和标准答案的作业版本；上传入口不会临时拼出不可追溯的题目或答案。</span>
      </div>
    );
  }

  const routeLabel = result?.route === "ready_for_batch_confirm"
    ? "可批量确认"
    : result?.route === "review_required"
      ? "需老师复核"
      : "暂时受阻";
  const groupingStartIndex = result?.groupingRoster.findIndex(
    (student) => student.studentNo === groupingStartNo,
  ) ?? -1;
  const groupingAbsenceCandidates = groupingStartIndex >= 0
    ? result?.groupingRoster.slice(groupingStartIndex + 1) ?? []
    : [];

  return (
    <div className="intake-layout">
      <section className="exam-card intake-form-card">
        <div className="exam-card-head">
          <div>
            <b>上传后自动整理</b>
            <div className="muted">只需完成下面三步，答案资料可不传。</div>
          </div>
          <span className="tag">固定卷</span>
        </div>
        <div className="intake-steps">
          <div className="intake-step">
            <span>1</span>
            <div className="intake-step-fields">
              <label className="field">
                <span className="fl">班级</span>
                <select value={classId} onChange={(event) => {
                  setClassId(Number(event.target.value));
                  resetRequest();
                }}>
                  {classOptions.map((option) => <option key={option.id} value={option.id}>{option.name}</option>)}
                </select>
              </label>
              <label className="field">
                <span className="fl">批改哪份作业</span>
                <select value={assessmentVersionId} onChange={(event) => {
                  setAssessmentVersionId(Number(event.target.value));
                  resetRequest();
                }}>
                  {assessmentOptions.map((option) => (
                    <option key={option.assessmentVersionId} value={option.assessmentVersionId}>
                      {option.assessmentTitle} · {option.itemCount} 题
                    </option>
                  ))}
                </select>
              </label>
            </div>
          </div>
          <div className="intake-step">
            <span>2</span>
            <div className="intake-upload-line">
              <div>
                <b>学生试卷</b>
                <small>支持 JPG、JPEG、PDF，可一次选择全班文件</small>
              </div>
              <button onClick={pickStudentPapers}>选择试卷</button>
              <strong>{studentPaths.length ? `已选 ${studentPaths.length} 份` : "未选择"}</strong>
            </div>
            {studentPaths.length > 0 && (
              <div className="intake-file-preview">
                {studentPaths.slice(0, 4).map((path) => <span key={path}>{fileName(path)}</span>)}
                {studentPaths.length > 4 && <span>另有 {studentPaths.length - 4} 份</span>}
              </div>
            )}
            <details className="intake-advanced">
              <summary>每名学生不是 1 页？按学生、页码顺序选择文件</summary>
              <label className="field">
                <span className="fl">每名学生固定页数</span>
                <input value={expectedPages} inputMode="numeric" onChange={(event) => {
                  setExpectedPages(event.target.value);
                  resetRequest();
                }} />
              </label>
            </details>
          </div>
          <div className="intake-step optional">
            <span>3</span>
            <div className="intake-upload-line">
              <div>
                <b>答案资料（选填）</b>
                <small>上传图片、PDF、TXT，或直接粘贴</small>
              </div>
              <button className="secondary" onClick={pickAnswer}>选择答案</button>
              <strong>{answerPath ? fileName(answerPath) : "可跳过"}</strong>
            </div>
            <textarea
              rows={3}
              value={answerText}
              disabled={Boolean(answerPath)}
              placeholder={answerPath ? "已选择答案文件" : "也可以在这里粘贴答案"}
              onChange={(event) => {
                setAnswerText(event.target.value);
                resetRequest();
              }}
            />
            {(answerPath || answerText) && (
              <button className="link-button" onClick={() => {
                setAnswerPath(null);
                setAnswerText("");
                resetRequest();
              }}>清除答案资料</button>
            )}
          </div>
        </div>
        <button className="primary intake-primary" disabled={busy} onClick={submit}>
          {busy ? "正在安全归档并拆分页面…" : "上传并开始整理"}
        </button>
        <div className="muted intake-safe-note">原图保留；PDF 按页归档；本步骤不会自动计分或发布。</div>
      </section>

      <aside className="exam-card intake-result-card">
        <div className="exam-card-head">
          <b>本批处理状态</b>
          {result && <span className={`tag intake-route ${result.route}`}>{routeLabel}</span>}
        </div>
        {!result ? (
          <div className="empty-state">上传后这里只显示三种结果和一个下一步，不让老师处理技术参数。</div>
        ) : (
          <>
            <div className="intake-import-summary">
              <b>已归档 {result.studentDocumentCount} 份学生卷，共 {result.studentPageCount} 页</b>
              <span>{result.answerDocumentCount ? "答案资料已归档，等待识别或确认" : "沿用作业已确认答案"}</span>
              <span>已按文件名自然顺序整理，并用拍摄/文件时间交叉核对 · 顺序可信度 {Math.round(result.orderConfidence * 100)}%</span>
              <span>资料类型：{MATERIAL_TYPE_LABEL[result.materialType] || result.materialType} · 预计 {result.studentGroupCount} 名学生</span>
            </div>
            {result.materialTypeNeedsConfirmation && (
              <div className="intake-material-confirm">
                <b>只确认一次，这批是什么？</b>
                <span>系统无法仅凭文件名可靠区分，不会直接进入错误识别路线。</span>
                <div>
                  <button disabled={confirmingType} onClick={() => confirmMaterialType("ordinary_paper")}>普通试卷</button>
                  <button disabled={confirmingType} onClick={() => confirmMaterialType("answer_sheet")}>答题卡</button>
                  <button disabled={confirmingType} onClick={() => confirmMaterialType("dictation")}>默写</button>
                </div>
              </div>
            )}
            {!result.materialTypeNeedsConfirmation
              && result.groupingRoute !== "blocked"
              && !result.groupingConfirmed && (
              <div className="intake-grouping-confirm">
                <b>确认照片从哪位学生开始</b>
                <span>系统会按学号升序连续对应 {result.studentGroupCount} 名学生；只需标出中间缺交的人。</span>
                <label className="field">
                  <span className="fl">第一份是谁</span>
                  <select value={groupingStartNo} onChange={(event) => {
                    setGroupingStartNo(event.target.value);
                    setAbsentStudentNos([]);
                  }}>
                    {result.groupingRoster.map((student) => (
                      <option key={student.studentId} value={student.studentNo}>
                        {student.studentNo}号 · {student.studentName}
                      </option>
                    ))}
                  </select>
                </label>
                <details>
                  <summary>{absentStudentNos.length ? `已标记 ${absentStudentNos.length} 人缺交` : "有人缺交？点这里勾选"}</summary>
                  <div className="intake-absence-list">
                    {groupingAbsenceCandidates.map((student) => (
                      <label key={student.studentId}>
                        <input
                          type="checkbox"
                          checked={absentStudentNos.includes(student.studentNo)}
                          onChange={() => toggleAbsentStudent(student.studentNo)}
                        />
                        <span>{student.studentNo}号 · {student.studentName}</span>
                      </label>
                    ))}
                  </div>
                </details>
                <button disabled={confirmingGrouping || !groupingStartNo} onClick={confirmGrouping}>
                  {confirmingGrouping ? "正在确认对应关系…" : `确认这 ${result.studentGroupCount} 份学生顺序`}
                </button>
              </div>
            )}
            {result.groupingConfirmed && (
              <div className="intake-grouping-confirm confirmed">
                <b>照片与学生顺序已确认</b>
                <span>{result.groupingFirstStudentNo}号至 {result.groupingLastStudentNo}号，共 {result.studentGroupCount} 名；后续页面质量异常只影响对应页组。</span>
              </div>
            )}
            <div className="intake-route-grid">
              <div className={result.route === "ready_for_batch_confirm" ? "active ready" : ""}>
                <span>可批量确认</span>
                <b>{result.targetCount ? result.readyCount : "—"}</b>
              </div>
              <div className={result.route === "review_required" ? "active review" : ""}>
                <span>需复核</span>
                <b>{result.targetCount ? result.reviewCount : "—"}</b>
              </div>
              <div className={result.route === "blocked" ? "active blocked" : ""}>
                <span>受阻</span>
                <b>{result.targetCount ? result.blockedCount : "—"}</b>
              </div>
            </div>
            {result.reasonCodes.length > 0 && (
              <div className="intake-reasons">
                {result.reasonCodes.slice(0, 3).map((code) => (
                  <span key={code}>{INTAKE_REASON_LABEL[code] || code}</span>
                ))}
              </div>
            )}
            {(result.orderConflictCodes.length > 0 || result.groupingIssueCodes.length > 0) && (
              <div className="intake-reasons">
                {[...result.orderConflictCodes, ...result.groupingIssueCodes]
                  .filter((code, index, all) => all.indexOf(code) === index)
                  .slice(0, 4)
                  .map((code) => <span key={code}>{INTAKE_REASON_LABEL[code] || code}</span>)}
              </div>
            )}
            <div className="intake-next">
              <span>下一步</span>
              <b>{result.nextAction}</b>
              <button disabled={result.route === "blocked" || result.groupingRoute === "blocked" || result.materialTypeNeedsConfirmation || !result.groupingConfirmed} onClick={onOpenReview}>进入批改终审</button>
            </div>
          </>
        )}
      </aside>
    </div>
  );
}

function displayObservedAnswer(row: ObjectiveWorkbenchRow) {
  if (!row.observed_answer_json) {
    return row.observation_state === "blank" ? "空白" : "—";
  }
  try {
    const value = JSON.parse(row.observed_answer_json) as {
      selected?: boolean;
      selected_labels?: string[];
      selected_values?: boolean[];
    };
    if (typeof value.selected === "boolean") {
      return value.selected ? "正确（√）" : "错误（×）";
    }
    if (Array.isArray(value.selected_labels)) {
      return value.selected_labels.join("、") || "—";
    }
    if (Array.isArray(value.selected_values)) {
      return value.selected_values.map((selected) => selected ? "√" : "×").join("、") + "（冲突）";
    }
  } catch {
    return row.observed_answer_json;
  }
  return row.observed_answer_json;
}

function objectiveOutcomeLabel(row: ObjectiveWorkbenchRow) {
  if (row.suggestion_outcome === "correct") return "建议正确";
  if (row.suggestion_outcome === "incorrect") return "建议错误";
  return "无法计分";
}

function ObjectiveReviewTab({
  workbench,
  onDone,
  onError,
}: {
  workbench: ObjectiveWorkbench;
  onDone: (message: string) => void;
  onError: (message: string) => void;
}) {
  const assessmentVersions = useMemo(() => {
    const unique = new Map<number, string>();
    workbench.rows.forEach((row) => unique.set(row.assessment_version_id, row.assessment_title));
    return Array.from(unique, ([id, title]) => ({ id, title }));
  }, [workbench.rows]);
  const [assessmentVersionId, setAssessmentVersionId] = useState(assessmentVersions[0]?.id ?? 0);
  const [assessmentItemId, setAssessmentItemId] = useState(0);
  const [busy, setBusy] = useState(false);
  const [manualScores, setManualScores] = useState<Record<number, string>>({});
  const [manualNotes, setManualNotes] = useState<Record<number, string>>({});

  useEffect(() => {
    if (!assessmentVersions.some((version) => version.id === assessmentVersionId)) {
      setAssessmentVersionId(assessmentVersions[0]?.id ?? 0);
    }
  }, [assessmentVersionId, assessmentVersions]);

  const itemOptions = useMemo(() => {
    const unique = new Map<number, ObjectiveWorkbenchRow>();
    workbench.rows
      .filter((row) => row.assessment_version_id === assessmentVersionId)
      .forEach((row) => unique.set(row.assessment_item_id, row));
    return Array.from(unique.values()).sort((left, right) => left.order_index - right.order_index);
  }, [assessmentVersionId, workbench.rows]);

  useEffect(() => {
    if (!itemOptions.some((row) => row.assessment_item_id === assessmentItemId)) {
      setAssessmentItemId(itemOptions[0]?.assessment_item_id ?? 0);
    }
  }, [assessmentItemId, itemOptions]);

  const rows = workbench.rows
    .filter((row) => row.assessment_version_id === assessmentVersionId && row.assessment_item_id === assessmentItemId)
    .sort((left, right) => left.student_no.localeCompare(right.student_no, "zh-CN", { numeric: true }));
  const attempts = workbench.attempts.filter((attempt) => attempt.assessment_version_id === assessmentVersionId);
  const confirmedCount = rows.filter((row) => row.current_suggestion_confirmed).length;
  const eligibleCount = rows.filter((row) => row.batch_eligible && !row.current_suggestion_confirmed).length;
  const exceptionCount = rows.length - confirmedCount - eligibleCount;

  const acceptOne = async (row: ObjectiveWorkbenchRow) => {
    setBusy(true);
    try {
      await examObjectiveAccept(row.suggestion_id);
      onDone(`已确认 ${row.student_name} 的第 ${row.order_index} 题，成绩仍需整卷显式发布`);
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const correctOne = async (row: ObjectiveWorkbenchRow) => {
    const rawScore = manualScores[row.suggestion_id] ?? String(row.suggested_score ?? "");
    const score = Number(rawScore);
    const note = (manualNotes[row.suggestion_id] ?? "").trim();
    if (!Number.isFinite(score) || score < 0 || score > row.max_score) {
      onError(`人工得分必须位于 0~${row.max_score} 分`);
      return;
    }
    if (!note) {
      onError("人工记分必须填写查看原图后的证据依据");
      return;
    }
    setBusy(true);
    try {
      await examObjectiveCorrect(row.suggestion_id, score, note);
      onDone(`已人工确认 ${row.student_name} 的第 ${row.order_index} 题为 ${score} 分`);
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const acceptStrictBatch = async () => {
    if (eligibleCount === 0) return;
    setBusy(true);
    try {
      const batch = await examObjectiveStrictBatchAccept(
        rows.map((row) => row.suggestion_id),
        `objective-review-${crypto.randomUUID()}`,
      );
      onDone(`严格批量终审完成：确认 ${batch.confirmed_count} 条，排除 ${batch.excluded_count} 条异常`);
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const retryRecognition = async (row: ObjectiveWorkbenchRow) => {
    setBusy(true);
    try {
      await examObjectiveRecognizeRegion(
        row.answer_region_revision_id,
        `objective-omr-${row.answer_region_revision_id}-${crypto.randomUUID()}`,
      );
      onDone(`已重新整理 ${row.student_name} 的第 ${row.order_index} 题；机器结果仍需老师确认`);
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const publishAttempt = async (attemptId: number, studentName: string) => {
    if (!window.confirm(`确认发布 ${studentName} 的本次整卷成绩？发布只采用当前老师终审 revision。`)) return;
    setBusy(true);
    try {
      const publication = await examObjectivePublishAttempt(attemptId);
      onDone(`${studentName} 的成绩已发布：${publication.total_score} 分（发布 revision ${publication.revision}）`);
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  };

  if (workbench.rows.length === 0) {
    return (
      <div className="empty-state objective-empty">
        <b>还没有可终审的标准卷客观题。</b>
        <span>真实视觉识别只处理已完成学生匹配、页面配准、答案区域和答题格确认的数据；上传后仍有异常时会先留给老师确认。</span>
        <span>需要临时录入时可继续使用“快速批改”兜底。</span>
      </div>
    );
  }

  return (
    <>
      <section className="objective-toolbar exam-card">
        <label className="field">
          <span className="fl">作业版本</span>
          <select value={assessmentVersionId} onChange={(event) => setAssessmentVersionId(Number(event.target.value))}>
            {assessmentVersions.map((version) => <option value={version.id} key={version.id}>{version.title} · v{version.id}</option>)}
          </select>
        </label>
        <label className="field">
          <span className="fl">按题终审</span>
          <select value={assessmentItemId} onChange={(event) => setAssessmentItemId(Number(event.target.value))}>
            {itemOptions.map((row) => (
              <option value={row.assessment_item_id} key={row.assessment_item_id}>
                第 {row.order_index} 题 · {OBJECTIVE_TYPE_LABEL[row.question_type]} · {row.question_stem.slice(0, 34)}
              </option>
            ))}
          </select>
        </label>
        <div className="objective-stats">
          <span><b>{rows.length}</b> 份作答</span>
          <span className="ok-text"><b>{confirmedCount}</b> 已确认</span>
          <span><b>{eligibleCount}</b> 可严格批量</span>
          <span className={exceptionCount ? "bad-text" : ""}><b>{exceptionCount}</b> 需单独处理</span>
        </div>
        <button className="primary" disabled={busy || eligibleCount === 0} onClick={acceptStrictBatch}>
          {busy ? "处理中…" : `确认 ${eligibleCount} 条高置信度结果`}
        </button>
        <div className="meta objective-batch-note">阈值固定为 0.95；空白、涂改、低置信度、识别失败和已终审记录都会明确排除。</div>
      </section>

      <div className="sech">本题证据 <span className="n">按学号排序</span></div>
      <div className="objective-review-list">
        {rows.map((row) => {
          const canAccept = row.suggested_score != null && !row.current_suggestion_confirmed;
          return (
            <article className={row.current_suggestion_confirmed ? "objective-review-row confirmed" : "objective-review-row"} key={row.suggestion_id}>
              <div className="objective-student">
                <b>{row.student_name}</b>
                <span>{row.student_no}</span>
                <span className={row.current_suggestion_confirmed ? "tag pass" : row.batch_eligible ? "tag wait" : "tag fail"}>
                  {row.current_suggestion_confirmed ? "已终审" : row.batch_eligible ? "可严格批量" : "需单独处理"}
                </span>
              </div>
              <div className="objective-evidence">
                {row.crop_path ? (
                  <img className="objective-crop" src={convertFileSrc(row.crop_path)} alt={`${row.student_name} 第 ${row.order_index} 题答案区域`} />
                ) : (
                  <div className="objective-crop missing">无答案裁剪</div>
                )}
                <div className="objective-facts">
                  <span>观察结果 <b>{OBSERVATION_LABEL[row.observation_state] ?? row.observation_state}</b></span>
                  <span>识别答案 <b>{displayObservedAnswer(row)}</b></span>
                  <span>置信度 <b>{row.confidence == null ? "—" : `${(row.confidence * 100).toFixed(1)}%`}</b></span>
                  <span>机器建议 <b className={row.suggestion_outcome === "correct" ? "ok-text" : row.suggestion_outcome === "incorrect" ? "bad-text" : ""}>{objectiveOutcomeLabel(row)}</b></span>
                  <span>建议得分 <b>{row.suggested_score == null ? "—" : `${row.suggested_score} / ${row.max_score}`}</b></span>
                  {row.exclusion_reason && <span>排除原因 <b>{EXCLUSION_LABEL[row.exclusion_reason] ?? row.exclusion_reason}</b></span>}
                  {row.current_suggestion_confirmed && <span>老师终审 <b>{row.teacher_score} 分 · {row.review_mode === "strict_batch" ? "严格批量" : "逐条确认"}</b></span>}
                </div>
              </div>
              <div className="objective-actions">
                {row.observation_state === "failed" && !row.current_suggestion_confirmed && (
                  <button disabled={busy} onClick={() => retryRecognition(row)}>重新整理本题</button>
                )}
                <button className={canAccept ? "primary" : ""} disabled={busy || !canAccept} onClick={() => acceptOne(row)}>
                  {row.current_suggestion_confirmed ? "已确认" : row.suggested_score == null ? "机器无法计分" : "接受本条建议"}
                </button>
                {!row.current_suggestion_confirmed && (
                  <details>
                    <summary>人工记分</summary>
                    <label className="field">
                      <span className="fl">得分（满分 {row.max_score}）</span>
                      <input
                        type="number"
                        min="0"
                        max={row.max_score}
                        step="0.5"
                        value={manualScores[row.suggestion_id] ?? String(row.suggested_score ?? "")}
                        onChange={(event) => setManualScores((current) => ({ ...current, [row.suggestion_id]: event.target.value }))}
                      />
                    </label>
                    <label className="field">
                      <span className="fl">证据依据</span>
                      <input
                        type="text"
                        placeholder="如：查看原图后确认涂改答案"
                        value={manualNotes[row.suggestion_id] ?? ""}
                        onChange={(event) => setManualNotes((current) => ({ ...current, [row.suggestion_id]: event.target.value }))}
                      />
                    </label>
                    <button disabled={busy} onClick={() => correctOne(row)}>保存人工 revision</button>
                  </details>
                )}
              </div>
            </article>
          );
        })}
      </div>

      <div className="sech">整卷发布 <span className="n">终审完成不等于已发布</span></div>
      <div className="objective-publish-grid">
        {attempts.map((attempt) => (
          <article className="exam-card objective-attempt" key={attempt.attempt_id}>
            <div className="exam-card-head">
              <div><b>{attempt.student_name}</b><div className="meta">{attempt.student_no}</div></div>
              <span className={attempt.attempt_state === "published" ? "tag pass" : attempt.can_publish ? "tag wait" : "tag"}>
                {attempt.attempt_state === "published" ? "已发布" : attempt.can_publish ? "待发布" : "终审中"}
              </span>
            </div>
            <div className="objective-attempt-score">
              <b>{attempt.teacher_total_score}</b><span>/ {attempt.max_total_score} 分</span>
            </div>
            <div className="meta">已终审 {attempt.confirmed_count} / {attempt.item_count} 题</div>
            {attempt.published_total_score != null && <div className="meta">当前已发布总分：{attempt.published_total_score}</div>}
            <button className="primary" disabled={busy || !attempt.can_publish} onClick={() => publishAttempt(attempt.attempt_id, attempt.student_name)}>
              {attempt.attempt_state === "published" ? "已发布" : attempt.can_publish ? "确认发布整卷" : "完成全部终审后发布"}
            </button>
          </article>
        ))}
      </div>
    </>
  );
}

function GradeTab({
  students,
  questions,
  answers,
  onDone,
  onError,
}: {
  students: Student[];
  questions: Question[];
  answers: AnswerDetail[];
  onDone: (message: string) => void;
  onError: (message: string) => void;
}) {
  const objective = questions.filter((question) => question.qtype !== "subjective");
  const [studentId, setStudentId] = useState(students[0]?.id ?? 0);
  const [questionId, setQuestionId] = useState(objective[0]?.id ?? 0);
  const [picked, setPicked] = useState("");
  const [busy, setBusy] = useState(false);
  const [notes, setNotes] = useState<Record<number, string>>({});

  const selectedQuestion = objective.find((question) => question.id === questionId);
  const pending = answers.filter((answer) => answer.status === "pending_review").length;

  const suggest = async () => {
    if (!studentId || !questionId || !picked.trim()) {
      onError("请选择学生、题目并填写学生答案");
      return;
    }
    setBusy(true);
    try {
      await examAnswerSuggest(studentId, questionId, picked);
      setPicked("");
      onDone("机器建议已生成，必须由老师确认后才会计分");
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const decide = async (answer: AnswerDetail, correct: boolean) => {
    const nextNote = (notes[answer.id] ?? answer.human_note ?? "").trim();
    const sameResult = answer.status === "confirmed" && answer.human_correct === correct;
    const sameNote = (answer.human_note ?? "") === nextNote;
    setBusy(true);
    try {
      await examAnswerHumanDecide(answer.id, correct, nextNote || null);
      if (sameResult && sameNote) {
        onDone("结论未变化，未重复累计错题或掌握度");
      } else if (sameResult) {
        onDone("终审备注已更新，结论和派生统计未变化");
      } else {
        onDone(answer.status === "confirmed" ? "改判已完成，错题与掌握度已重算" : "老师终审已保存");
      }
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <div className="exam-grid">
        <section className="exam-card">
          <div className="exam-card-head">
            <div>
              <b>录入一题作答</b>
              <div className="meta">机器结果只进入待终审队列</div>
            </div>
            <span className={pending ? "tag wait" : "tag pass"}>{pending} 条待确认</span>
          </div>
          {students.length === 0 || objective.length === 0 ? (
            <div className="empty-state">
              {students.length === 0 ? "请先在“学生”中导入学生。" : "请先在“题库”中录入客观题。"}
            </div>
          ) : (
            <div className="form exam-form">
              <label className="field">
                <span className="fl">学生</span>
                <select value={studentId} onChange={(event) => setStudentId(Number(event.target.value))}>
                  {students.map((student) => <option key={student.id} value={student.id}>{student.name} · {student.student_no}</option>)}
                </select>
              </label>
              <label className="field">
                <span className="fl">题目</span>
                <select value={questionId} onChange={(event) => setQuestionId(Number(event.target.value))}>
                  {objective.map((question) => (
                    <option key={question.id} value={question.id}>
                      {question.question_no ?? `#${question.id}`} · {TYPE_LABEL[question.qtype]} · {question.stem.slice(0, 28)}
                    </option>
                  ))}
                </select>
              </label>
              {selectedQuestion && (
                <div className="answer-reference">
                  <span>标准答案</span><b>{selectedQuestion.correct_answer || "未设置"}</b>
                  <span>分值</span><b>{selectedQuestion.max_score}</b>
                </div>
              )}
              <label className="field">
                <span className="fl">学生答案</span>
                <input
                  type="text"
                  value={picked}
                  placeholder={selectedQuestion?.qtype === "judge" ? "如：对 / 错" : "如：B / AC / 填空内容"}
                  onChange={(event) => setPicked(event.target.value)}
                  onKeyDown={(event) => event.key === "Enter" && suggest()}
                />
              </label>
              <button className="primary" disabled={busy} onClick={suggest}>
                {busy ? "处理中…" : "生成机器建议"}
              </button>
            </div>
          )}
        </section>
        <section className="exam-card principle-card">
          <div className="exam-card-head"><b>判定规则</b></div>
          <ol>
            <li>单选、多选、判断和填空只做确定性标准化对比。</li>
            <li>机器建议不写错题本，也不改变知识点掌握度。</li>
            <li>老师可以确认或改判；每次改判都会重算派生结果。</li>
            <li>主观题继续保留人工判定，不交给这条自动链路。</li>
          </ol>
        </section>
      </div>

      <div className="sech">终审队列 <span className="n">最近 {answers.length} 条</span></div>
      {answers.length === 0 && <div className="empty-state">暂无作答记录。</div>}
      {answers.map((answer) => (
        <div className={answer.status === "pending_review" ? "answer-row pending" : "answer-row"} key={answer.id}>
          <div className="answer-summary">
            <div className="answer-title">
              <b>{answer.student_name}</b>
              <span>{answer.question_no ?? `题目 #${answer.question_id}`}</span>
              <span className={answer.status === "pending_review" ? "tag wait" : "tag pass"}>
                {answer.status === "pending_review" ? "待老师确认" : "已终审"}
              </span>
            </div>
            <div className="answer-stem">{answer.question_stem}</div>
            <div className="answer-facts">
              <span>学生答案 <b>{answer.picked || "—"}</b></span>
              <span>标准答案 <b>{answer.correct_answer || "—"}</b></span>
              <span>机器建议 <b className={answer.machine_correct == null ? "" : answer.machine_correct ? "ok-text" : "bad-text"}>{answer.machine_correct == null ? "无" : answer.machine_correct ? "正确" : "错误"}</b></span>
              <span>老师结论 <b>{answer.human_correct == null ? "未确认" : answer.human_correct ? "正确" : "错误"}</b></span>
              {answer.knowledge_point_name && <span>命中知识点 <b>{answer.knowledge_point_name}</b></span>}
            </div>
            <div className="meta">{displayTime(answer.created_at)} · {answer.machine_note}</div>
          </div>
          <div className="answer-review">
            <input
              type="text"
              placeholder="终审备注（可选）"
              value={notes[answer.id] ?? answer.human_note ?? ""}
              onChange={(event) => setNotes((current) => ({ ...current, [answer.id]: event.target.value }))}
            />
            <div>
              <button disabled={busy} onClick={() => decide(answer, false)}>判为错误</button>
              <button className="primary" disabled={busy} onClick={() => decide(answer, true)}>判为正确</button>
            </div>
          </div>
        </div>
      ))}
    </>
  );
}

function QuestionTab({
  questions,
  knowledge,
  onDone,
  onError,
}: {
  questions: Question[];
  knowledge: KnowledgePoint[];
  onDone: (message: string) => void;
  onError: (message: string) => void;
}) {
  const [questionNo, setQuestionNo] = useState("");
  const [qtype, setQtype] = useState<Question["qtype"]>("single");
  const [stem, setStem] = useState("");
  const [correct, setCorrect] = useState("");
  const [maxScore, setMaxScore] = useState(1);
  const [kpId, setKpId] = useState(0);
  const [optionsText, setOptionsText] = useState("");
  const [busy, setBusy] = useState(false);

  const save = async () => {
    if (!stem.trim() || !correct.trim()) {
      onError("题干和标准答案不能为空");
      return;
    }
    const normalizedCorrect = correct.toUpperCase().replace(/[^A-Z0-9]/g, "");
    const options = optionsText
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean)
      .map((line, ord) => {
        const matched = line.match(/^([A-Za-z])[\.、:：)）\s-]*(.+)$/);
        const label = matched?.[1].toUpperCase() ?? String.fromCharCode(65 + ord);
        return {
          label,
          content: matched?.[2].trim() ?? line,
          is_correct: normalizedCorrect.includes(label),
          knowledge_point_id: kpId || null,
          analysis: null,
          ord,
        };
      });
    const input: QuestionInput = {
      subject_id: null,
      question_no: questionNo.trim() || null,
      qtype,
      stem: stem.trim(),
      image_path: null,
      correct_answer: correct.trim(),
      knowledge_point_id: kpId || null,
      difficulty: null,
      analysis: null,
      max_score: maxScore,
      enabled: true,
      options,
    };
    setBusy(true);
    try {
      await questionCreate(input);
      setQuestionNo("");
      setStem("");
      setCorrect("");
      setOptionsText("");
      onDone("题目已保存，可立即进入快速批改");
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="exam-grid wide-left">
      <section className="exam-card">
        <div className="exam-card-head"><div><b>手工录题</b><div className="meta">OCR 接入前的可用兜底入口</div></div></div>
        <div className="form exam-form">
          <div className="form-pair">
            <label className="field"><span className="fl">题号（可选，需唯一）</span><input type="text" value={questionNo} onChange={(event) => setQuestionNo(event.target.value)} placeholder="如 PHY-001" /></label>
            <label className="field"><span className="fl">题型</span><select value={qtype} onChange={(event) => setQtype(event.target.value as Question["qtype"])}><option value="single">单选</option><option value="multi">多选</option><option value="judge">判断</option><option value="fill">填空</option></select></label>
          </div>
          <label className="field"><span className="fl">题干</span><textarea rows={4} value={stem} onChange={(event) => setStem(event.target.value)} /></label>
          <div className="form-pair">
            <label className="field"><span className="fl">标准答案</span><input type="text" value={correct} onChange={(event) => setCorrect(event.target.value)} placeholder="如 B / AC / 对" /></label>
            <label className="field"><span className="fl">分值</span><input type="number" min="0.5" step="0.5" value={maxScore} onChange={(event) => setMaxScore(Number(event.target.value))} /></label>
          </div>
          <label className="field"><span className="fl">主知识点</span><select value={kpId} onChange={(event) => setKpId(Number(event.target.value))}><option value={0}>未设置</option>{knowledge.map((kp) => <option value={kp.id} key={kp.id}>{kp.code ? `${kp.code} · ` : ""}{kp.name}</option>)}</select></label>
          {(qtype === "single" || qtype === "multi") && <label className="field"><span className="fl">选项（每行一项，可选）</span><textarea rows={5} value={optionsText} onChange={(event) => setOptionsText(event.target.value)} placeholder={"A. 选项内容\nB. 选项内容"} /></label>}
          <button className="primary" disabled={busy} onClick={save}>{busy ? "保存中…" : "保存题目"}</button>
        </div>
      </section>
      <section className="exam-card question-list-card">
        <div className="exam-card-head"><b>题库</b><span className="tag">{questions.length} 题</span></div>
        {questions.length === 0 && <div className="empty-state">还没有题目。</div>}
        {questions.map((question) => (
          <div className="question-item" key={question.id}>
            <div><span className="cno">{question.question_no ?? `#${question.id}`}</span><span className="tag">{TYPE_LABEL[question.qtype]}</span></div>
            <b>{question.stem}</b>
            <div className="meta">答案 {question.correct_answer || "—"} · {question.max_score} 分 · {knowledge.find((kp) => kp.id === question.knowledge_point_id)?.name ?? "未关联知识点"}</div>
          </div>
        ))}
      </section>
    </div>
  );
}

function KnowledgeTab({
  knowledge,
  onDone,
  onError,
}: {
  knowledge: KnowledgePoint[];
  onDone: (message: string) => void;
  onError: (message: string) => void;
}) {
  const [name, setName] = useState("");
  const [code, setCode] = useState("");
  const [parentId, setParentId] = useState(0);
  const [busy, setBusy] = useState(false);
  const roots = useMemo(() => knowledge.filter((kp) => kp.parent_id == null), [knowledge]);

  const save = async () => {
    if (!name.trim()) {
      onError("知识点名称不能为空");
      return;
    }
    setBusy(true);
    try {
      await kpCreate(name.trim(), code.trim() || null, parentId || null);
      setName("");
      setCode("");
      onDone("知识点已创建");
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="exam-grid">
      <section className="exam-card">
        <div className="exam-card-head"><b>新增知识点</b></div>
        <div className="form exam-form">
          <label className="field"><span className="fl">名称</span><input type="text" value={name} onChange={(event) => setName(event.target.value)} placeholder="如 动量守恒" /></label>
          <label className="field"><span className="fl">编码（可选）</span><input type="text" value={code} onChange={(event) => setCode(event.target.value)} placeholder="如 PHY-MOM" /></label>
          <label className="field"><span className="fl">上级板块（可选）</span><select value={parentId} onChange={(event) => setParentId(Number(event.target.value))}><option value={0}>根节点</option>{roots.map((kp) => <option key={kp.id} value={kp.id}>{kp.name}</option>)}</select></label>
          <button className="primary" disabled={busy} onClick={save}>{busy ? "保存中…" : "创建知识点"}</button>
        </div>
      </section>
      <section className="exam-card">
        <div className="exam-card-head"><b>知识点树</b><span className="tag">{knowledge.length} 个</span></div>
        {roots.length === 0 && <div className="empty-state">还没有知识点。</div>}
        {roots.map((root) => (
          <div className="kp-group" key={root.id}>
            <div className="kp-root"><span>◆</span><b>{root.name}</b>{root.code && <code>{root.code}</code>}</div>
            {knowledge.filter((kp) => kp.parent_id === root.id).map((child) => <div className="kp-child" key={child.id}><span>└</span>{child.name}{child.code && <code>{child.code}</code>}</div>)}
          </div>
        ))}
      </section>
    </div>
  );
}
