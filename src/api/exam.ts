import { call } from "./client";

export interface KnowledgePoint {
  id: number;
  subject_id: number | null;
  parent_id: number | null;
  code: string | null;
  name: string;
}

export interface Question {
  id: number;
  subject_id: number | null;
  question_no: string | null;
  qtype: "single" | "multi" | "judge" | "fill" | "subjective";
  stem: string;
  image_path: string | null;
  correct_answer: string | null;
  knowledge_point_id: number | null;
  difficulty: number | null;
  analysis: string | null;
  max_score: number;
  enabled: boolean;
}

export interface QuestionOptionInput {
  label: string;
  content: string;
  is_correct: boolean;
  knowledge_point_id: number | null;
  analysis: string | null;
  ord: number;
}

export interface QuestionInput {
  subject_id: number | null;
  question_no: string | null;
  qtype: Question["qtype"];
  stem: string;
  image_path: string | null;
  correct_answer: string | null;
  knowledge_point_id: number | null;
  difficulty: number | null;
  analysis: string | null;
  max_score: number;
  enabled: boolean;
  options: QuestionOptionInput[];
}

export interface AnswerDetail {
  id: number;
  student_id: number;
  student_name: string;
  question_id: number;
  question_no: string | null;
  question_stem: string;
  question_type: string;
  picked: string | null;
  correct_answer: string | null;
  machine_correct: boolean | null;
  human_correct: boolean | null;
  is_correct: boolean | null;
  score: number | null;
  max_score: number;
  knowledge_point_id: number | null;
  knowledge_point_name: string | null;
  status: "pending_review" | "confirmed";
  machine_note: string | null;
  human_note: string | null;
  created_at: string;
  decided_at: string | null;
}

export const kpList = () => call<KnowledgePoint[]>("kp_list");
export const kpCreate = (name: string, code: string | null, parent_id: number | null) =>
  call<KnowledgePoint>("kp_create", { subjectId: null, parentId: parent_id, code, name });

export const questionsList = () => call<Question[]>("questions_list");
export const questionCreate = (q: QuestionInput) => call<number>("question_create", { q });

export const examAnswersList = (limit = 50) =>
  call<AnswerDetail[]>("exam_answers_list", { limit });
export const examAnswerSuggest = (student_id: number, question_id: number, picked: string) =>
  call<AnswerDetail>("exam_answer_suggest", { studentId: student_id, questionId: question_id, picked });
export const examAnswerHumanDecide = (
  answer_id: number,
  is_correct: boolean,
  note: string | null,
) => call<AnswerDetail>("exam_answer_human_decide", { answerId: answer_id, isCorrect: is_correct, note });
