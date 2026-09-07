export type FixedIntakeOperationRelease = () => void;

export interface FixedIntakeOperationRegistry {
  claim(key: string): FixedIntakeOperationRelease | null;
  activeKeys(): string[];
}

export function createFixedIntakeOperationRegistry(): FixedIntakeOperationRegistry {
  const owners = new Map<string, symbol>();

  return {
    claim(key) {
      if (owners.has(key)) {
        return null;
      }

      const owner = Symbol(key);
      owners.set(key, owner);
      let released = false;

      return () => {
        if (released) {
          return;
        }

        released = true;
        if (owners.get(key) === owner) {
          owners.delete(key);
        }
      };
    },

    activeKeys() {
      return Array.from(owners.keys());
    },
  };
}

export interface FixedIntakeReadEffectOptions<T> {
  claim: () => FixedIntakeOperationRelease | null;
  load: () => Promise<T>;
  isCurrent: () => boolean;
  onStarted: () => void;
  onSucceeded: (value: T) => void;
  onFailed: (error: unknown) => void;
  onFinished?: () => void;
}

const noOpReadEffectCleanup = () => undefined;

export function startFixedIntakeReadEffect<T>(
  options: FixedIntakeReadEffectOptions<T>,
): () => void {
  const release = options.claim();
  if (!release) {
    return noOpReadEffectCleanup;
  }

  options.onStarted();
  let request: Promise<T>;
  try {
    request = options.load();
  } catch (error) {
    if (options.isCurrent()) {
      options.onFailed(error);
      options.onFinished?.();
    }
    release();
    return noOpReadEffectCleanup;
  }

  void request
    .then(
      (value) => {
        if (options.isCurrent()) {
          options.onSucceeded(value);
        }
      },
      (error) => {
        if (options.isCurrent()) {
          options.onFailed(error);
        }
      },
    )
    .finally(() => {
      if (options.isCurrent()) {
        options.onFinished?.();
      }
      release();
    });

  // React StrictMode may replay setup immediately after cleanup. The in-flight
  // owner stays claimed until the provider settles; current identity and mount
  // state decide whether its single continuation may write.
  return noOpReadEffectCleanup;
}

export interface FixedIntakeCompletionIdentity {
  scopeRevision: number;
  draftRevision?: number;
  batchId?: number;
  pageId?: number;
  rejectedPageId?: number;
  ordinaryRunId?: number;
  answerSheetTemplateRunId?: number;
  dictationTemplateRunId?: number;
  runId?: number;
  scopeKey?: string;
}

const OPTIONAL_COMPLETION_IDENTITY_KEYS = [
  "draftRevision",
  "batchId",
  "pageId",
  "rejectedPageId",
  "ordinaryRunId",
  "answerSheetTemplateRunId",
  "dictationTemplateRunId",
  "runId",
  "scopeKey",
] as const;

export function isFixedIntakeCompletionCurrent(
  expected: FixedIntakeCompletionIdentity,
  current: FixedIntakeCompletionIdentity,
): boolean {
  if (expected.scopeRevision !== current.scopeRevision) {
    return false;
  }

  return OPTIONAL_COMPLETION_IDENTITY_KEYS.every(
    (key) => expected[key] === undefined || expected[key] === current[key],
  );
}
