import { call } from "./client";

export interface Student {
  id: number;
  student_no: string;
  name: string;
  class_id: number | null;
  enabled: boolean;
}

export interface RecContent {
  id: number;
  content_no: string;
  title: string;
  answer_text: string;
  answer_version: number;
  subject_id: number | null;
  enabled: boolean;
}

export const studentsList = () => call<Student[]>("students_list");
export const contentsList = () => call<RecContent[]>("contents_list");

export const studentsUpsert = (student_no: string, name: string, enabled: boolean) =>
  call<Student>("students_upsert", { studentNo: student_no, name, enabled });

export const contentsUpsert = (
  content_no: string,
  title: string,
  answer_text: string,
  enabled: boolean,
) => call<RecContent>("contents_upsert", { contentNo: content_no, title, answerText: answer_text, enabled });

export const tasksGenerate = (pairs: [number, number][]) =>
  call<number>("tasks_generate", { pairs });

// ── 批量导入 / 删减 ──
export interface BatchImport {
  ok: number;
  failed: number;
  errors: string[];
}
export interface DeleteResult {
  deleted: number;
  blocked: string[]; // 有历史记录删不掉的（只能停用）
}

export const studentsImport = (rows: { student_no: string; name: string }[]) =>
  call<BatchImport>("students_import", { rows });
export const studentsSetEnabled = (ids: number[], enabled: boolean) =>
  call<number>("students_set_enabled", { ids, enabled });
export const studentsDelete = (ids: number[]) =>
  call<DeleteResult>("students_delete", { ids });

export const contentsImport = (
  rows: { content_no: string; title: string; answer_text: string }[],
) => call<BatchImport>("contents_import", { rows });
export const contentsSetEnabled = (ids: number[], enabled: boolean) =>
  call<number>("contents_set_enabled", { ids, enabled });
export const contentsDelete = (ids: number[]) =>
  call<DeleteResult>("contents_delete", { ids });
