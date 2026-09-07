import type {
  FixedIntakeOption,
  PageCycleSuggestion,
} from "../../api/exam";
import { fileName } from "./examPure";

interface FixedIntakeClassOption {
  id: number;
  name: string;
}

interface FixedIntakeUploadFormProps {
  classOptions: FixedIntakeClassOption[];
  classId: number;
  selectClass: (classId: number) => void;
  assessmentOptions: FixedIntakeOption[];
  assessmentVersionId: number;
  selectAssessment: (assessmentVersionId: number) => void;
  pickStudentPapers: () => Promise<void>;
  studentPaths: string[];
  pageCycle: PageCycleSuggestion | null;
  expectedPages: string;
  changeExpectedPages: (expectedPages: string) => void;
  pickAnswer: () => Promise<void>;
  answerPath: string | null;
  answerText: string;
  changeAnswerText: (answerText: string) => void;
  clearAnswerSource: () => void;
  busy: boolean;
  submit: () => Promise<void>;
}

export function FixedIntakeUploadForm({
  classOptions,
  classId,
  selectClass,
  assessmentOptions,
  assessmentVersionId,
  selectAssessment,
  pickStudentPapers,
  studentPaths,
  pageCycle,
  expectedPages,
  changeExpectedPages,
  pickAnswer,
  answerPath,
  answerText,
  changeAnswerText,
  clearAnswerSource,
  busy,
  submit,
}: FixedIntakeUploadFormProps) {
  return (
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
                <select value={classId} onChange={(event) =>
                  selectClass(Number(event.target.value))
                }>
                  {classOptions.map((option) => <option key={option.id} value={option.id}>{option.name}</option>)}
                </select>
              </label>
              <label className="field">
                <span className="fl">批改哪份作业</span>
                <select value={assessmentVersionId} onChange={(event) =>
                  selectAssessment(Number(event.target.value))
                }>
                  {assessmentOptions.map((option) => (
                    <option key={option.assessmentVersionId} value={option.assessmentVersionId}>
                      {option.isDefault ? "默认 · " : ""}
                      {option.assessmentTitle} · 第 {option.revision} 版 · {option.itemCount} 题
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
              <summary>
                {pageCycle?.source === "visual_repeating_layout_v1"
                  ? `检测到版式每 ${pageCycle.expectedPagesPerAttempt} 页重复 · 可修改`
                  : pageCycle?.source === "pdf_document_page_count"
                    ? `检测到每份 PDF ${pageCycle.expectedPagesPerAttempt} 页 · 可修改`
                    : "没有识别出稳定重复？手动填写每人页数"}
              </summary>
              <label className="field">
                <span className="fl">每名学生固定页数</span>
                <input
                  value={expectedPages}
                  inputMode="numeric"
                  onChange={(event) => changeExpectedPages(event.target.value)}
                />
              </label>
              {pageCycle && (
                <small className="muted">
                  {pageCycle.needsTeacherInput
                    ? "现有照片不足以可靠判断，请确认页数。"
                    : `版式周期可信度 ${Math.round(pageCycle.confidence * 100)}%，最终仍在学生顺序卡中一次确认。`}
                </small>
              )}
            </details>
          </div>
          <div className="intake-step optional">
            <span>3</span>
            <div className="intake-upload-line">
              <div>
                <b>答案资料（选填）</b>
                <small>上传图片、PDF、Word、Excel、TXT，或直接粘贴</small>
              </div>
              <button className="secondary" onClick={pickAnswer}>选择答案</button>
              <strong>{answerPath ? fileName(answerPath) : "可跳过"}</strong>
            </div>
            <textarea
              rows={3}
              value={answerText}
              disabled={Boolean(answerPath)}
              placeholder={answerPath ? "已选择答案文件" : "也可以在这里粘贴答案"}
              onChange={(event) => changeAnswerText(event.target.value)}
            />
            {(answerPath || answerText) && (
              <button className="link-button" onClick={clearAnswerSource}>清除答案资料</button>
            )}
          </div>
        </div>
        <button className="primary intake-primary" disabled={busy} onClick={submit}>
          {busy ? "正在安全归档并拆分页面…" : "上传并开始整理"}
        </button>
        <div className="muted intake-safe-note">原文件保留；PDF 按页识别，Word/Excel 仅在本机提取文字；本步骤不会自动计分或发布。</div>
      </section>
  );
}
