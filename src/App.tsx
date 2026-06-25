import { modules } from "./shell/moduleRegistry";
import Dashboard from "./pages/Dashboard";

export default function App() {
  return (
    <div className="app">
      <aside className="sidebar">
        <div className="logo">教辅系统</div>
        <nav>
          {modules.map((m) => (
            <a key={m.key} className={m.key === "recitation" ? "nav active" : "nav"}>
              <span className="nav-icon">{m.icon}</span>
              {m.name}
            </a>
          ))}
        </nav>
        <div className="sidebar-foot">v0.1 · 本地版</div>
      </aside>
      <main className="main">
        <Dashboard />
      </main>
    </div>
  );
}
