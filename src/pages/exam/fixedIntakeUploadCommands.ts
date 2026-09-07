import { open } from "@tauri-apps/plugin-dialog";

import {
  examFixedIntakeInferPageCycle,
  examFixedIntakePrepare,
} from "../../api/exam.ts";
import { hasExtension } from "./examPure.ts";
import type { FixedIntakeControllerRuntime } from "./fixedIntakeControllerRuntime.ts";
import type { FixedIntakeCompletionIdentity } from "./fixedIntakeLifecycle.ts";
import { fixedIntakeCompletionIdentity } from "./fixedIntakeRootState.ts";
import type { FixedIntakeViewModel } from "./fixedIntakeViewModel.ts";

const STUDENT_FILE_EXTENSIONS = ["jpg", "jpeg", "pdf"];
const ANSWER_FILE_EXTENSIONS = ["jpg", "jpeg", "pdf", "docx", "xlsx", "txt"];

export function createFixedIntakeUploadCommands({
  runtime,
  view,
  selectStudentFiles,
  selectAnswerFile,
  analyzeAnswerSource,
  onError,
}: {
  runtime: FixedIntakeControllerRuntime;
  view: FixedIntakeViewModel;
  selectStudentFiles: (studentPaths: string[]) => FixedIntakeCompletionIdentity;
  selectAnswerFile: (answerPath: string) => void;
  analyzeAnswerSource: (batchId: number, retry?: boolean) => Promise<void>;
  onError: (message: string) => void;
}) {
  const {
    stateRef,
    dispatchBatchWorkflow,
    completionIsCurrent,
    claimOperation,
  } = runtime;
  const {
    assessmentVersionId,
    studentPaths,
    answerPath,
    answerText,
    expectedPages,
    requestKey,
  } = view;

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
      const identity = selectStudentFiles(paths);
      const release = claimOperation(
        `page-cycle:${identity.scopeRevision}:${identity.draftRevision}`,
      );
      if (!release) return;
      try {
        const inferred = await examFixedIntakeInferPageCycle(paths);
        dispatchBatchWorkflow({
          type: "FIXED_INTAKE_PAGE_CYCLE_RECEIVED",
          identity,
          pageCycle: inferred,
        });
      } catch (err) {
        if (completionIsCurrent(identity)) {
          onError(`选择学生试卷失败：${String(err)}`);
        }
      } finally {
        release();
      }
    } catch (err) {
      onError(`选择学生试卷失败：${String(err)}`);
    }
  };

  const pickAnswer = async () => {
    try {
      const selected = await open({ multiple: false });
      if (!selected || Array.isArray(selected)) return;
      if (!hasExtension(selected, ANSWER_FILE_EXTENSIONS)) {
        onError("答案资料只支持 JPG、JPEG、PDF、DOCX、XLSX 或 TXT。");
        return;
      }
      selectAnswerFile(selected);
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
    const identity = fixedIntakeCompletionIdentity(stateRef.current);
    const release = claimOperation(
      `prepare:${identity.scopeRevision}:${identity.draftRevision}`,
    );
    if (!release) return;
    const currentKey = requestKey || crypto.randomUUID();
    dispatchBatchWorkflow({ type: "PREPARE_STARTED", requestKey: currentKey });
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
      dispatchBatchWorkflow({
        type: "FIXED_INTAKE_COMPLETION_RECEIVED",
        identity,
        completion: { type: "PREPARE_SUCCEEDED", result: prepared },
      });
      if (prepared.answerDocumentCount > 0 && completionIsCurrent(identity)) {
        void analyzeAnswerSource(prepared.batchId);
      }
    } catch (err) {
      if (completionIsCurrent(identity)) {
        onError(String(err));
      }
    } finally {
      dispatchBatchWorkflow({
        type: "FIXED_INTAKE_COMPLETION_RECEIVED",
        identity,
        completion: { type: "PREPARE_FINISHED" },
      });
      release();
    }
  };

  return { pickStudentPapers, pickAnswer, submit };
}
