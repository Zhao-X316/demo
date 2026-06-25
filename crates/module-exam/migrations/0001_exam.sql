-- M2 改作业模块表（前缀 exam_）。通用表（students/submissions/tasks/verdicts）复用 core。

-- 知识点（板块树：parent_id 自指；板块→章节→知识点）
CREATE TABLE IF NOT EXISTS exam_knowledge_points (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  subject_id INTEGER,
  parent_id INTEGER,
  code TEXT,
  name TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_exam_kp_parent ON exam_knowledge_points(parent_id);

-- 题目
CREATE TABLE IF NOT EXISTS exam_questions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  subject_id INTEGER,
  question_no TEXT,
  qtype TEXT NOT NULL,              -- single|multi|judge|fill|subjective
  stem TEXT NOT NULL,
  image_path TEXT,
  correct_answer TEXT,             -- 客观题正确项("B"/"AC")或填空/判断答案
  knowledge_point_id INTEGER,      -- 主知识点
  difficulty INTEGER,              -- 1-5
  analysis TEXT,                   -- 整体解析
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_exam_q_no ON exam_questions(question_no) WHERE question_no IS NOT NULL;

-- 选项（逐选项解析 + 知识点：这是"错在哪个知识点"的来源）
CREATE TABLE IF NOT EXISTS exam_question_options (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  question_id INTEGER NOT NULL,
  label TEXT NOT NULL,             -- A/B/C/D
  content TEXT NOT NULL,
  is_correct INTEGER NOT NULL DEFAULT 0,
  knowledge_point_id INTEGER,      -- 该选项考查/迷惑的知识点
  analysis TEXT,                   -- 选它说明哪个知识点不清晰
  ord INTEGER NOT NULL DEFAULT 0,
  FOREIGN KEY(question_id) REFERENCES exam_questions(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_exam_opt_q ON exam_question_options(question_id);

-- 试卷/作业（题目集合）
CREATE TABLE IF NOT EXISTS exam_papers (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  title TEXT NOT NULL,
  subject_id INTEGER,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS exam_paper_questions (
  paper_id INTEGER NOT NULL,
  question_id INTEGER NOT NULL,
  ord INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY(paper_id, question_id)
);

-- 学生作答（每题一行）
CREATE TABLE IF NOT EXISTS exam_student_answers (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  submission_id INTEGER,           -- 关联 core submissions（作业图片）
  student_id INTEGER NOT NULL,
  question_id INTEGER NOT NULL,
  picked TEXT,                     -- 识别出的作答
  picked_option_id INTEGER,
  is_correct INTEGER,
  score REAL,
  knowledge_point_id INTEGER,      -- 命中的知识点（错项的或题目的）
  note TEXT,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_exam_sa_student ON exam_student_answers(student_id);
CREATE INDEX IF NOT EXISTS idx_exam_sa_q ON exam_student_answers(question_id);

-- 错题本
CREATE TABLE IF NOT EXISTS exam_wrong_items (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  student_id INTEGER NOT NULL,
  question_id INTEGER NOT NULL,
  knowledge_point_id INTEGER,
  times_wrong INTEGER NOT NULL DEFAULT 1,
  status TEXT NOT NULL DEFAULT 'open',   -- open|mastered
  first_seen TEXT NOT NULL DEFAULT (datetime('now')),
  last_seen TEXT NOT NULL DEFAULT (datetime('now')),
  UNIQUE(student_id, question_id)
);

-- 知识点掌握度聚合（看板用）
CREATE TABLE IF NOT EXISTS exam_knowledge_mastery (
  student_id INTEGER NOT NULL,
  knowledge_point_id INTEGER NOT NULL,
  total INTEGER NOT NULL DEFAULT 0,
  correct INTEGER NOT NULL DEFAULT 0,
  updated_at TEXT NOT NULL DEFAULT (datetime('now')),
  PRIMARY KEY(student_id, knowledge_point_id)
);
