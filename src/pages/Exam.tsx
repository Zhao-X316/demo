import { useEffect, useState } from "react";
import {
  AnswerDetail,
  DictationWorkbench,
  FixedIntakeOption,
  KnowledgePoint,
  ObjectiveWorkbench,
  SubjectiveWorkbench,
  Question,
  examAnswerSheetSubjectiveWorkbench,
  examAnswersList,
  examDictationWorkbench,
  examFixedIntakeOptions,
  examObjectiveWorkbench,
  kpList,
  questionsList,
} from "../api/exam";
import { Student, studentsList } from "../api/manage";
import { DictationReviewTab } from "./exam/DictationReviewTab";
import { FixedIntakeTab } from "./exam/FixedIntakeTab";
import { GradeTab } from "./exam/GradeTab";
import { KnowledgeTab } from "./exam/KnowledgeTab";
import { ObjectiveReviewTab } from "./exam/ObjectiveReviewTab";
import { QuestionTab } from "./exam/QuestionTab";
import { SubjectiveReviewTab } from "./exam/SubjectiveReviewTab";


type Tab = "intake" | "objective" | "subjective" | "dictation" | "grade" | "questions" | "knowledge";


export default function Exam() {
  const [tab, setTab] = useState<Tab>("intake");
  const [students, setStudents] = useState<Student[]>([]);
  const [questions, setQuestions] = useState<Question[]>([]);
  const [knowledge, setKnowledge] = useState<KnowledgePoint[]>([]);
  const [answers, setAnswers] = useState<AnswerDetail[]>([]);
  const [objectiveWorkbench, setObjectiveWorkbench] = useState<ObjectiveWorkbench>({ rows: [], attempts: [] });
  const [subjectiveWorkbench, setSubjectiveWorkbench] = useState<SubjectiveWorkbench>({ rows: [], attempts: [] });
  const [dictationWorkbench, setDictationWorkbench] = useState<DictationWorkbench>({ rows: [], attempts: [] });
  const [intakeOptions, setIntakeOptions] = useState<FixedIntakeOption[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [toast, setToast] = useState("");

  const load = async () => {
    setError("");
    try {
      const [studentRows, questionRows, kpRows, answerRows, workbench, subjectiveRows, dictationRows, fixedOptions] = await Promise.all([
        studentsList(),
        questionsList(),
        kpList(),
        examAnswersList(),
        examObjectiveWorkbench(),
        examAnswerSheetSubjectiveWorkbench(),
        examDictationWorkbench(),
        examFixedIntakeOptions(),
      ]);
      setStudents(studentRows.filter((student) => student.enabled));
      setQuestions(questionRows.filter((question) => question.enabled));
      setKnowledge(kpRows);
      setAnswers(answerRows);
      setObjectiveWorkbench(workbench);
      setSubjectiveWorkbench(subjectiveRows);
      setDictationWorkbench(dictationRows);
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
        <button className={tab === "subjective" ? "tab active" : "tab"} onClick={() => setTab("subjective")}>答题卡主观题</button>
        <button className={tab === "dictation" ? "tab active" : "tab"} onClick={() => setTab("dictation")}>默写复核</button>
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
              onOpenReview={(reviewTab) => {
                void load();
                setTab(reviewTab);
              }}
              onDone={done}
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
          {tab === "subjective" && (
            <SubjectiveReviewTab
              workbench={subjectiveWorkbench}
              onDone={done}
              onError={(message) => setError(message)}
            />
          )}
          {tab === "dictation" && (
            <DictationReviewTab
              workbench={dictationWorkbench}
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
