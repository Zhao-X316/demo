import { useState } from "react";
import Dashboard from "./pages/Dashboard";
import Import from "./pages/Import";
import Manage from "./pages/Manage";
import Settings from "./pages/Settings";

type View = "dashboard" | "import" | "manage" | "settings";

const NAV: { key: View; icon: string; label: string }[] = [
  { key: "dashboard", icon: "📋", label: "今日看板" },
  { key: "import", icon: "🎙️", label: "导入录音" },
  { key: "manage", icon: "🗂️", label: "管理" },
  { key: "settings", icon: "⚙️", label: "设置" },
];

export default function App() {
  const [view, setView] = useState<View>("dashboard");
  return (
    <div className="app">
      <aside className="sidebar">
        <div className="logo">📖 背诵批改</div>
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
        <div className="sidebar-foot">v0.1 · 本地版</div>
      </aside>
      <main className="main">
        {view === "dashboard" && <Dashboard />}
        {view === "import" && <Import />}
        {view === "manage" && <Manage />}
        {view === "settings" && <Settings />}
      </main>
    </div>
  );
}
