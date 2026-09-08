import { useEffect, useMemo, useState } from "react";
import type { IntakeResume } from "../../api/workspace";

import type { FixedIntakeOption } from "../../api/exam.ts";
import { createFixedIntakeAnswerSourceCommands } from "./fixedIntakeAnswerSourceCommands.ts";
import { createFixedIntakeAnswerSheetCommands } from "./fixedIntakeAnswerSheetCommands.ts";
import { useFixedIntakeControllerRuntime } from "./fixedIntakeControllerRuntime.ts";
import { createFixedIntakeDictationCommands } from "./fixedIntakeDictationCommands.ts";
import { createFixedIntakeGroupingQualityCommands } from "./fixedIntakeGroupingQualityCommands.ts";
import { createFixedIntakeOrdinaryCommands } from "./fixedIntakeOrdinaryCommands.ts";
import { createFixedIntakeUploadCommands } from "./fixedIntakeUploadCommands.ts";
import {
  createFixedIntakeViewModel,
  fixedIntakeAssessmentOptions,
  fixedIntakeClassOptions,
} from "./fixedIntakeViewModel.ts";

export function useFixedIntakeController(
  options: FixedIntakeOption[],
  onDone: (message: string) => void,
  onError: (message: string) => void,
  initialClass?: number,
  resume?: IntakeResume,
  onBatchSaved?: (batchId: number) => void,
  onDirtyChange?: (dirty:boolean)=>void,
) {
  const classOptions = useMemo(() => fixedIntakeClassOptions(options), [options]);
  const initialClassId = resume?.classId ?? (initialClass || classOptions[0]?.id || 0);
  const initialAssessmentVersionId = options.find(
    (option) => option.classId === initialClassId,
  )?.assessmentVersionId ?? 0;
  const runtime = useFixedIntakeControllerRuntime({
    classId: initialClassId,
    assessmentVersionId: resume?.assessmentVersionId ?? initialAssessmentVersionId,
  }, resume);
  const [progressRestored,setProgressRestored]=useState(!resume);
  const [restoringProgress,setRestoringProgress]=useState(false);
  const { state, dispatchScopeMutation } = runtime;
  const { classId, assessmentVersionId } = state.context;
  const assessmentOptions = fixedIntakeAssessmentOptions(options, classId);

  const selectClass = (nextClassId: number) => {
    dispatchScopeMutation({ type: "class_selected", classId: nextClassId });
  };

  const selectAssessment = (nextAssessmentVersionId: number) => {
    dispatchScopeMutation({
      type: "assessment_selected",
      assessmentVersionId: nextAssessmentVersionId,
    });
  };

  const selectStudentFiles = (studentPaths: string[]) => dispatchScopeMutation({
    type: "student_paths_selected",
    studentPaths,
  });

  const selectAnswerFile = (answerPath: string) => {
    dispatchScopeMutation({ type: "answer_file_selected", answerPath });
  };

  const changeAnswerText = (answerText: string) => {
    dispatchScopeMutation({ type: "answer_text_changed", answerText });
  };

  const changeExpectedPages = (expectedPages: string) => {
    dispatchScopeMutation({ type: "expected_pages_changed", expectedPages });
  };

  const clearAnswerSource = () => {
    dispatchScopeMutation({ type: "answer_cleared" });
  };

  useEffect(() => {
    if (!resume && !(initialClass && classId===initialClass) && !classOptions.some((option) => option.id === classId)) {
      selectClass(classOptions[0]?.id ?? 0);
    }
  }, [classId, classOptions]);

  useEffect(() => {
    if (!resume && !assessmentOptions.some(
      (option) => option.assessmentVersionId === assessmentVersionId,
    )) {
      selectAssessment(assessmentOptions[0]?.assessmentVersionId ?? 0);
    }
  }, [assessmentOptions, assessmentVersionId]);

  useEffect(() => { if (state.batch.result) onBatchSaved?.(state.batch.result.batchId); }, [state.batch.result?.batchId]);

  useEffect(()=>{
    onDirtyChange?.(!state.batch.result && Boolean(state.draft.studentPaths.length || state.draft.answerPath || state.draft.answerText.trim()));
  },[state.draft.studentPaths,state.draft.answerPath,state.draft.answerText,state.batch.result?.batchId]);

  const view = createFixedIntakeViewModel(options, state);
  const answerSourceCommands = createFixedIntakeAnswerSourceCommands({
    runtime,
    view,
    onError,
  });
  const uploadCommands = createFixedIntakeUploadCommands({
    runtime,
    view,
    selectStudentFiles,
    selectAnswerFile,
    analyzeAnswerSource: answerSourceCommands.analyzeAnswerSource,
    onError,
  });
  const ordinaryCommands = createFixedIntakeOrdinaryCommands({
    runtime,
    view,
    onDone,
    onError,
  });
  const {
    answerSheetTemplateScopeKey,
    startAnswerSheetTemplateStatusReadEffect,
    ...answerSheetCommands
  } = createFixedIntakeAnswerSheetCommands({
    runtime,
    view,
    onError,
  });
  const {
    dictationTemplateScopeKey,
    startDictationTemplateStatusReadEffect,
    ...dictationCommands
  } = createFixedIntakeDictationCommands({
    runtime,
    view,
    onError,
  });
  const {
    groupingEvidenceBatchId,
    startGroupingEvidenceReadEffect,
    ...groupingQualityCommands
  } = createFixedIntakeGroupingQualityCommands({
    runtime,
    view,
    analyzeOrdinaryPages: ordinaryCommands.analyzeOrdinaryPages,
    onError,
  });

  useEffect(
    () => startGroupingEvidenceReadEffect(),
    [groupingEvidenceBatchId],
  );

  useEffect(
    () => startAnswerSheetTemplateStatusReadEffect(),
    [answerSheetTemplateScopeKey],
  );

  useEffect(
    () => startDictationTemplateStatusReadEffect(),
    [dictationTemplateScopeKey],
  );

  return {
    hasOptions: options.length > 0,
    progressRestored,
    restoringProgress,
    continueSavedProcessing: async () => {
      if(restoringProgress)return;
      setRestoringProgress(true);
      try {
        // Existing idempotent domain commands reuse stored runs. This starts only on an explicit click.
        if(view.result?.answerDocumentCount) await answerSourceCommands.analyzeAnswerSource(view.result.batchId);
        if(view.result?.materialType==='ordinary_paper') await ordinaryCommands.analyzeOrdinaryPages();
        else if(view.result?.materialType==='answer_sheet' && view.answerSheetTemplateSetReady) await answerSheetCommands.processAnswerSheetPages();
        else if(view.result?.materialType==='dictation' && view.dictationTemplateStatus?.activeTemplate) await dictationCommands.processDictationPages();
        if(runtime.componentMountedRef.current)setProgressRestored(true);
      } finally {if(runtime.componentMountedRef.current)setRestoringProgress(false);}
    },
    runtime,
    view,
    actions: {
      selectClass,
      selectAssessment,
      changeAnswerText,
      changeExpectedPages,
      clearAnswerSource,
      ...uploadCommands,
      ...answerSourceCommands,
      ...groupingQualityCommands,
      ...ordinaryCommands,
      ...answerSheetCommands,
      ...dictationCommands,
    },
  };
}

export type FixedIntakeController = ReturnType<typeof useFixedIntakeController>;
