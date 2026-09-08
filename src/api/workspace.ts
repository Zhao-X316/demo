import { call } from "./client";
import type { FixedIntakeResult } from "./exam/intake";
import type {
  ObjectiveWorkbench,
  SubjectiveWorkbench,
  DictationWorkbench,
} from "./exam";

export interface WorkspaceTask {
  kind: "exam_batch" | "exam_attempt" | "recitation";
  sourceId: number;
  title: string;
  classId: number | null;
  className: string | null;
  studentName: string | null;
  status: string;
  updatedAt: string;
  assessmentVersionId: number | null;
  attemptIds: number[];
  hasEvidence: boolean;
}

export interface IntakeResume {
  classId: number;
  assessmentVersionId: number;
  result: FixedIntakeResult;
  /** Pages already materialized; current review data is loaded from domain workbenches. */
  processedPageIds: number[];
  processingHistory: Array<{
    id: number;
    stage: string;
    status: string;
    message: string | null;
    updatedAt: string;
  }>;
}

export const workspaceTasks = () => call<WorkspaceTask[]>("workspace_tasks");
export const resumeIntake = (batchId: number) =>
  call<IntakeResume>("exam_fixed_intake_resume", { batchId });
export const workspaceExamReview = (
  kind: WorkspaceTask["kind"],
  sourceId: number,
) =>
  call<{
    task: WorkspaceTask;
    objective: ObjectiveWorkbench;
    subjective: SubjectiveWorkbench;
    dictation: DictationWorkbench;
  }>("workspace_exam_review", { kind, sourceId });
