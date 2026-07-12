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

export interface TaskGenerateResult {
  created: number;
  revived: number;
  skipped_open: number;
  skipped_completed: number;
  total_effective: number;
}

export const tasksGenerate = (pairs: [number, number][]) =>
  call<TaskGenerateResult>("tasks_generate", { pairs });

// 覆盖已布置：关掉这些学生今日未开始的新背任务，再按新内容重布。返回新建数。
export const tasksReassign = (student_ids: number[], content_ids: number[]) =>
  call<number>("tasks_reassign", { studentIds: student_ids, contentIds: content_ids });

// 删除已布置内容：关掉这些学生今日、指定内容的新背任务（含已开始的）。返回关闭数。
export const tasksRemove = (student_ids: number[], content_ids: number[]) =>
  call<number>("tasks_remove", { studentIds: student_ids, contentIds: content_ids });

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

// ── 班级（分班）──
export interface Class {
  id: number;
  name: string;
  term: string | null;
  textbook: string | null; // 绑定教材，如「道法8上」
}
export const classesList = () => call<Class[]>("classes_list");
export const classCreate = (name: string, textbook?: string, term?: string) =>
  call<Class>("class_create", { name, textbook: textbook ?? null, term: term ?? null });
export const classUpdate = (id: number, name: string, textbook?: string, term?: string) =>
  call<Class>("class_update", { id, name, textbook: textbook ?? null, term: term ?? null });
export const classDelete = (id: number) => call<void>("class_delete", { id });
// class_id=null 即移出班级
export const studentsSetClass = (ids: number[], class_id: number | null) =>
  call<number>("students_set_class", { ids, classId: class_id });

export const contentsImport = (
  rows: { content_no: string; title: string; answer_text: string }[],
) => call<BatchImport>("contents_import", { rows });
export const contentsSetEnabled = (ids: number[], enabled: boolean) =>
  call<number>("contents_set_enabled", { ids, enabled });
export const contentsDelete = (ids: number[]) =>
  call<DeleteResult>("contents_delete", { ids });

// ── 背诵清单解析 ──
export interface ParsedContent {
  content_no: string;
  title: string;
  answer_text: string;
  is_key: boolean;
  lesson_no: number;
  point_seq: number;
  original_no: string;
  lesson_title: string;
}
export const parseSyllabus = (text: string, prefix: string) =>
  call<ParsedContent[]>("parse_syllabus", { text, prefix });
