import { useState } from "react";
import Today from "./pages/Today";
import GradingDesk from "./pages/GradingDesk";
import Library from "./pages/Library";
import Students from "./pages/Students";
import Records from "./pages/Records";
import Settings from "./pages/Settings";

type View = "today" | "desk" | "library" | "students" | "records" | "settings";

const NAV: { key: View; icon: string; label: string }[] = [
  { key: "today", icon: "◎", label: "今日" },
  { key: "desk", icon: "◷", label: "批改台" },
  { key: "library", icon: "▤", label: "内容库" },
  { key: "students", icon: "◍", label: "学生" },
  { key: "records", icon: "≣", label: "记录" },
  { key: "settings", icon: "⚙", label: "设置" },
];

export default function App() {
  const [view, setView] = useState<View>("today");
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
        <div className="mod-row on">
          <span className="di">📖</span>背诵批改
        </div>
        <div className="mod-row">
          <span className="di">✎</span>改作业<span className="soon">即将</span>
        </div>
        <div className="mod-row">
          <span className="di">⌗</span>错题本<span className="soon">即将</span>
        </div>
        <div className="navsec">背诵</div>
        <nav>
          {NAV.map((n) => (
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
      </main>
    </div>
  );
}
