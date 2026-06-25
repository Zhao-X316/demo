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
  call<Student>("students_upsert", { student_no, name, enabled });

export const contentsUpsert = (
  content_no: string,
  title: string,
  answer_text: string,
  enabled: boolean,
) => call<RecContent>("contents_upsert", { content_no, title, answer_text, enabled });

export const tasksGenerate = (pairs: [number, number][]) =>
  call<number>("tasks_generate", { pairs });
