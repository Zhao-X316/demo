import { useState } from "react";
import Today from "./pages/Today";
import GradingDesk from "./pages/GradingDesk";
import Library from "./pages/Library";
import Students from "./pages/Students";
import Records from "./pages/Records";
import Settings from "./pages/Settings";
import Exam from "./pages/Exam";
import ClassDashboard from "./pages/ClassDashboard";
import LearningInsights from "./pages/LearningInsights";
import QuestionBank from "./pages/QuestionBank";
import { AppModule, DashboardTargetView } from "./api/classDashboard";

type Module = AppModule;
type View =
  | "dashboard"
  | "today"
  | "desk"
  | "library"
  | "students"
  | "records"
  | "settings"
  | "exam"
  | "questionBank"
  | "learning";

const NAV: { key: View; icon: string; label: string }[] = [
  { key: "dashboard", icon: "▦", label: "班级概览" },
  { key: "today", icon: "◎", label: "今日" },
  { key: "desk", icon: "◷", label: "批改台" },
  { key: "library", icon: "▤", label: "内容库" },
  { key: "students", icon: "◍", label: "学生" },
  { key: "records", icon: "≣", label: "记录" },
  { key: "settings", icon: "⚙", label: "设置" },
];

const EXAM_NAV: { key: View; icon: string; label: string }[] = [
  { key: "dashboard", icon: "▦", label: "班级概览" },
  { key: "exam", icon: "✎", label: "题目批改" },
  { key: "questionBank", icon: "▤", label: "题目与题库" },
  { key: "students", icon: "◍", label: "学生" },
  { key: "settings", icon: "⚙", label: "设置" },
];

export default function App() {
  const [module, setModule] = useState<Module>("recitation");
  const [view, setView] = useState<View>("dashboard");
  const nav = view === "learning" ? [] : module === "recitation" ? NAV : EXAM_NAV;

  const switchModule = (next: Module) => {
    setModule(next);
    setView("dashboard");
  };
  const navigate = (nextModule: AppModule, nextView: DashboardTargetView | "students") => {
    setModule(nextModule);
    setView(nextView);
  };
  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">
          <div className="logo">乐</div>
          <b>乐学教辅</b>
        </div>
        <div className="kbar">
          <span>🔍</span>
          <span>搜索 / 跳转</span>
          <span className="kbd">⌘K</span>
        </div>
        <div className="navsec">模块</div>
        <div className={module === "recitation" && view !== "learning" ? "mod-row on" : "mod-row"} onClick={() => switchModule("recitation")}>
          <span className="di">📖</span>背诵批改
        </div>
        <div className={module === "exam" && view !== "learning" ? "mod-row on" : "mod-row"} onClick={() => switchModule("exam")}>
          <span className="di">✎</span>改作业
        </div>
        <div className={view === "learning" ? "mod-row on" : "mod-row"} onClick={() => setView("learning")}>
          <span className="di">⌗</span>错题与掌握
        </div>
        {view !== "learning" && <div className="navsec">{module === "recitation" ? "背诵" : "作业"}</div>}
        <nav>
          {nav.map((n) => (
            <button
              key={n.key}
              className={n.key === view ? "nav active" : "nav"}
              onClick={() => setView(n.key)}
            >
              <span className="nav-icon">{n.icon}</span>
              {n.label}
            </button>
          ))}
        </nav>
        <div className="sidebar-foot">本机 · 数据不出门 · v0.3</div>
      </aside>
      <main className="main">
        {view === "dashboard" && (
          <ClassDashboard
            onNavigate={navigate}
            onOpenLearning={() => setView("learning")}
          />
        )}
        {view === "today" && <Today />}
        {view === "desk" && <GradingDesk />}
        {view === "library" && <Library />}
        {view === "students" && <Students />}
        {view === "records" && <Records />}
        {view === "settings" && <Settings />}
        {view === "exam" && <Exam />}
        {view === "questionBank" && <QuestionBank onOpenExam={() => setView("exam")} />}
        {view === "learning" && (
          <LearningInsights
            onOpenExam={() => {
              setModule("exam");
              setView("exam");
            }}
            onOpenStudents={() => {
              setModule("recitation");
              setView("students");
            }}
          />
        )}
      </main>
    </div>
  );
}
