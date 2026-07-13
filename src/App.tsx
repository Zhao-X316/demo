import { useState } from "react";
import Today from "./pages/Today";
import GradingDesk from "./pages/GradingDesk";
import Library from "./pages/Library";
import Students from "./pages/Students";
import Records from "./pages/Records";
import Settings from "./pages/Settings";
import Exam from "./pages/Exam";

type Module = "recitation" | "exam";
type View = "today" | "desk" | "library" | "students" | "records" | "settings" | "exam";

const NAV: { key: View; icon: string; label: string }[] = [
  { key: "today", icon: "◎", label: "今日" },
  { key: "desk", icon: "◷", label: "批改台" },
  { key: "library", icon: "▤", label: "内容库" },
  { key: "students", icon: "◍", label: "学生" },
  { key: "records", icon: "≣", label: "记录" },
  { key: "settings", icon: "⚙", label: "设置" },
];

const EXAM_NAV: { key: View; icon: string; label: string }[] = [
  { key: "exam", icon: "✎", label: "题目批改" },
  { key: "students", icon: "◍", label: "学生" },
  { key: "settings", icon: "⚙", label: "设置" },
];

export default function App() {
  const [module, setModule] = useState<Module>("recitation");
  const [view, setView] = useState<View>("today");
  const nav = module === "recitation" ? NAV : EXAM_NAV;

  const switchModule = (next: Module) => {
    setModule(next);
    setView(next === "recitation" ? "today" : "exam");
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
        <div className={module === "recitation" ? "mod-row on" : "mod-row"} onClick={() => switchModule("recitation")}>
          <span className="di">📖</span>背诵批改
        </div>
        <div className={module === "exam" ? "mod-row on" : "mod-row"} onClick={() => switchModule("exam")}>
          <span className="di">✎</span>改作业
        </div>
        <div className="mod-row">
          <span className="di">⌗</span>错题本<span className="soon">即将</span>
        </div>
        <div className="navsec">{module === "recitation" ? "背诵" : "作业"}</div>
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
        {view === "today" && <Today />}
        {view === "desk" && <GradingDesk />}
        {view === "library" && <Library />}
        {view === "students" && <Students />}
        {view === "records" && <Records />}
        {view === "settings" && <Settings />}
        {view === "exam" && <Exam />}
      </main>
    </div>
  );
}
