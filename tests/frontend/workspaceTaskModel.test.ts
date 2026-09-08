import test from "node:test";
import assert from "node:assert/strict";
import {
  parseTaskLocator,
  scopeWorkbench,
  taskFilter,
} from "../../src/pages/workspace/taskModel.ts";
import type { WorkspaceTask } from "../../src/api/workspace.ts";

const task: WorkspaceTask = {
  kind: "exam_batch",
  sourceId: 1,
  title: "作业",
  classId: 1,
  className: "一班",
  studentName: null,
  status: "ready_to_publish",
  updatedAt: "",
  assessmentVersionId: 10,
  attemptIds: [20],
  hasEvidence: true,
};
test("teacher confirmation remains actionable until publication", () => {
  assert.equal(taskFilter(task), "todo");
  assert.equal(taskFilter({ ...task, status: "published" }), "done");
  assert.equal(taskFilter({ ...task, status: "incomplete" }), "todo");
});
test("review scope excludes other batches and fails closed when no attempts exist", () => {
  const rows = [
    { assessment_version_id: 10, attempt_id: 20 },
    { assessment_version_id: 10, attempt_id: 21 },
    { assessment_version_id: 11, attempt_id: 20 },
  ];
  assert.deepEqual(scopeWorkbench(rows, task), [rows[0]]);
  assert.deepEqual(scopeWorkbench(rows, { ...task, attemptIds: [] }), []);
});
test("saved locations contain only validated domain identifiers", () => {
  assert.deepEqual(parseTaskLocator("exam_batch:12"), {
    kind: "exam_batch",
    sourceId: 12,
  });
  for (const value of [
    null,
    "exam_batch:0",
    "exam_batch:-1",
    "exam_batch:1e2",
    "unknown:1",
    "recitation:9999999999999999999",
  ])
    assert.equal(parseTaskLocator(value), null);
});
