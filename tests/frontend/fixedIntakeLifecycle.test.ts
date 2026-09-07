import assert from "node:assert/strict";
import test from "node:test";

import {
  createFixedIntakeOperationRegistry,
  isFixedIntakeCompletionCurrent,
  type FixedIntakeCompletionIdentity,
} from "../../src/pages/exam/fixedIntakeLifecycle.ts";

test("operation registry claims the same key synchronously only once", () => {
  const registry = createFixedIntakeOperationRegistry();
  const release = registry.claim("prepare:request-1");

  assert.equal(typeof release, "function");
  assert.equal(registry.claim("prepare:request-1"), null);
  assert.deepEqual(registry.activeKeys(), ["prepare:request-1"]);
});

test("operation registry allows a new owner after the current owner releases", () => {
  const registry = createFixedIntakeOperationRegistry();
  const releaseFirst = registry.claim("cycle:draft-1");
  assert.ok(releaseFirst);

  releaseFirst();
  const releaseSecond = registry.claim("cycle:draft-1");
  assert.ok(releaseSecond);
  assert.deepEqual(registry.activeKeys(), ["cycle:draft-1"]);

  releaseSecond();
  assert.deepEqual(registry.activeKeys(), []);
});

test("a stale release cannot release a later owner of the same operation key", () => {
  const registry = createFixedIntakeOperationRegistry();
  const releaseFirst = registry.claim("grouping-evidence:101");
  assert.ok(releaseFirst);
  releaseFirst();

  const releaseSecond = registry.claim("grouping-evidence:101");
  assert.ok(releaseSecond);
  releaseFirst();

  assert.equal(registry.claim("grouping-evidence:101"), null);
  releaseSecond();
  assert.ok(registry.claim("grouping-evidence:101"));
});

test("StrictMode cleanup does not make an in-flight read claim available", () => {
  const registry = createFixedIntakeOperationRegistry();
  const releaseWhenPromiseSettles = registry.claim("answer-sheet-template-status:101:301");
  assert.ok(releaseWhenPromiseSettles);

  // Cleanup must neither release nor invalidate the owner. Business identity
  // decides whether its eventual completion is still allowed to write.
  assert.equal(registry.claim("answer-sheet-template-status:101:301"), null);

  releaseWhenPromiseSettles();
  assert.ok(registry.claim("answer-sheet-template-status:101:301"));
});

type ReadEffectRunner = <T>(options: {
  claim: () => (() => void) | null;
  load: () => Promise<T>;
  isCurrent: () => boolean;
  onStarted: () => void;
  onSucceeded: (value: T) => void;
  onFailed: (error: unknown) => void;
  onFinished?: () => void;
}) => () => void;

async function readEffectRunner(): Promise<ReadEffectRunner> {
  const module = await import("../../src/pages/exam/fixedIntakeLifecycle.ts");
  const candidate = (module as unknown as Record<string, unknown>)[
    "startFixedIntakeReadEffect"
  ];
  assert.equal(typeof candidate, "function", "read effect runner must exist in production");
  return candidate as ReadEffectRunner;
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

const nextTurn = () => new Promise<void>((resolve) => setImmediate(resolve));

test("StrictMode cleanup and replay share one provider call and one continuation", async () => {
  const startRead = await readEffectRunner();
  const registry = createFixedIntakeOperationRegistry();
  const provider = deferred<string>();
  let providerCalls = 0;
  let started = 0;
  const succeeded: string[] = [];
  let finished = 0;
  const setup = () => startRead({
    claim: () => registry.claim("grouping-evidence:101"),
    load: () => {
      providerCalls += 1;
      return provider.promise;
    },
    isCurrent: () => true,
    onStarted: () => {
      started += 1;
    },
    onSucceeded: (value) => succeeded.push(value),
    onFailed: (error) => assert.fail(`unexpected read failure: ${String(error)}`),
    onFinished: () => {
      finished += 1;
    },
  });

  const cleanupFirstSetup = setup();
  cleanupFirstSetup();
  const cleanupReplay = setup();

  assert.equal(providerCalls, 1);
  assert.equal(started, 1);
  assert.deepEqual(registry.activeKeys(), ["grouping-evidence:101"]);

  provider.resolve("evidence-ready");
  await provider.promise;
  await nextTurn();

  assert.deepEqual(succeeded, ["evidence-ready"]);
  assert.equal(finished, 1);
  assert.deepEqual(registry.activeKeys(), []);
  cleanupReplay();
});

test("stale read completion is silent but still releases its owner", async () => {
  const startRead = await readEffectRunner();
  const registry = createFixedIntakeOperationRegistry();
  const provider = deferred<string>();
  let current = true;
  let succeeded = 0;
  let failed = 0;
  let finished = 0;

  startRead({
    claim: () => registry.claim("dictation-template-status:dictation:101:301"),
    load: () => provider.promise,
    isCurrent: () => current,
    onStarted: () => undefined,
    onSucceeded: () => {
      succeeded += 1;
    },
    onFailed: () => {
      failed += 1;
    },
    onFinished: () => {
      finished += 1;
    },
  });

  current = false;
  provider.resolve("stale-status");
  await provider.promise;
  await nextTurn();

  assert.equal(succeeded, 0);
  assert.equal(failed, 0);
  assert.equal(finished, 0);
  assert.deepEqual(registry.activeKeys(), []);
});

const current: FixedIntakeCompletionIdentity = {
  scopeRevision: 7,
  draftRevision: 4,
  batchId: 101,
  pageId: 301,
  runId: 901,
};

test("completion is current only when revision and supplied business identities match", () => {
  assert.equal(isFixedIntakeCompletionCurrent(current, current), true);
  assert.equal(
    isFixedIntakeCompletionCurrent(
      { scopeRevision: 7, batchId: 101 },
      current,
    ),
    true,
  );
  assert.equal(
    isFixedIntakeCompletionCurrent(
      { scopeRevision: 6, batchId: 101 },
      current,
    ),
    false,
  );
  assert.equal(
    isFixedIntakeCompletionCurrent(
      { scopeRevision: 7, draftRevision: 3 },
      current,
    ),
    false,
  );
  assert.equal(
    isFixedIntakeCompletionCurrent(
      { scopeRevision: 7, batchId: 102 },
      current,
    ),
    false,
  );
  assert.equal(
    isFixedIntakeCompletionCurrent(
      { scopeRevision: 7, pageId: 302 },
      current,
    ),
    false,
  );
  assert.equal(
    isFixedIntakeCompletionCurrent(
      { scopeRevision: 7, runId: 902 },
      current,
    ),
    false,
  );
});
