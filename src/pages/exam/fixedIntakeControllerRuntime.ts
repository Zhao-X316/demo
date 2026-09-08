import { useEffect, useReducer, useRef } from "react";
import type { IntakeResume } from "../../api/workspace";

import {
  createFixedIntakeOperationRegistry,
  type FixedIntakeCompletionIdentity,
} from "./fixedIntakeLifecycle.ts";
import {
  createInitialFixedIntakeState,
  fixedIntakeCompletionIdentity,
  fixedIntakeReducer,
  isFixedIntakeStateCompletionCurrent,
  type FixedIntakeAction,
  type FixedIntakeScopeMutation,
} from "./fixedIntakeRootState.ts";
import type { FixedIntakeContextSlice } from "./fixedIntakeState.ts";

export function useFixedIntakeControllerRuntime(
  initialContext: FixedIntakeContextSlice,
  resume?: IntakeResume,
) {
  const [state, dispatchFixedIntake] = useReducer(
    fixedIntakeReducer,
    initialContext,
    (context) => {
      const state = createInitialFixedIntakeState(context);
      if (resume) {
        state.batch.result = resume.result;
        state.draft.expectedPages = String(resume.result.expectedPagesPerAttempt);
        state.grouping.startNo = resume.result.groupingFirstStudentNo ?? "";
      }
      return state;
    },
  );
  const stateRef = useRef(state);
  stateRef.current = state;

  const componentMountedRef = useRef(false);
  useEffect(() => {
    componentMountedRef.current = true;
    return () => {
      componentMountedRef.current = false;
    };
  }, []);

  const operationRegistryRef = useRef<
    ReturnType<typeof createFixedIntakeOperationRegistry> | null
  >(null);
  if (!operationRegistryRef.current) {
    operationRegistryRef.current = createFixedIntakeOperationRegistry();
  }
  const operationRegistry = operationRegistryRef.current;

  const dispatchScopeMutation = (
    mutation: FixedIntakeScopeMutation,
  ): FixedIntakeCompletionIdentity => {
    const action = { type: "FIXED_INTAKE_SCOPE_INVALIDATED" as const, mutation };
    const next = fixedIntakeReducer(stateRef.current, action);
    stateRef.current = next;
    dispatchFixedIntake(action);
    return fixedIntakeCompletionIdentity(next);
  };

  const dispatchBatchWorkflowBeforeContinuation = (action: FixedIntakeAction) => {
    const next = fixedIntakeReducer(stateRef.current, action);
    stateRef.current = next;
    dispatchFixedIntake(action);
  };

  const completionIsCurrent = (identity: FixedIntakeCompletionIdentity) =>
    componentMountedRef.current
    && isFixedIntakeStateCompletionCurrent(stateRef.current, identity);

  const projectActiveOperations = () => {
    dispatchFixedIntake({
      type: "FIXED_INTAKE_ACTIVE_OPERATIONS_CHANGED",
      activeOperations: operationRegistry.activeKeys(),
    });
  };

  const claimOperation = (key: string) => {
    const release = operationRegistry.claim(key);
    if (!release) return null;
    projectActiveOperations();
    return () => {
      release();
      if (componentMountedRef.current) {
        projectActiveOperations();
      }
    };
  };

  return {
    resume,
    state,
    stateRef,
    componentMountedRef,
    dispatchFixedIntake,
    dispatchAnswerSource: dispatchFixedIntake,
    dispatchBatchWorkflow: dispatchFixedIntake,
    dispatchScopeMutation,
    dispatchBatchWorkflowBeforeContinuation,
    completionIsCurrent,
    claimOperation,
  };
}

export type FixedIntakeControllerRuntime = ReturnType<
  typeof useFixedIntakeControllerRuntime
>;
