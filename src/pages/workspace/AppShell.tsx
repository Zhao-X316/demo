import { useEffect, useState } from "react";
import { classesList, type Class } from "../../api/manage";
import Workspace, { type WorkRequest } from "./Workspace";
import Materials from "./Materials";
import Students from "../Students";
import ClassDashboard from "../ClassDashboard";
import LearningInsights from "../LearningInsights";
import Settings from "../Settings";
import "./workspace.css";

type Section = "work" | "materials" | "students" | "settings";
const NAV = [
  { id: "work", label: "工作台" },
  { id: "materials", label: "资料" },
  { id: "students", label: "学生" },
] as const;
const CLASS_KEY = "jiaofu.selected-class.v1";
function savedClass() {
  try {
    const value = Number(localStorage.getItem(CLASS_KEY));
    return Number.isSafeInteger(value) && value > 0 ? value : 0;
  } catch {
    return 0;
  }
}

export default function AppShell() {
  const [section, setSection] = useState<Section>("work");
  const [visited, setVisited] = useState<Set<Section>>(new Set(["work"]));
  const [classId, setClassId] = useState(savedClass);
  const [classes, setClasses] = useState<Class[]>([]);
  const [error, setError] = useState("");
  const [studentView, setStudentView] = useState<
    "roster" | "dashboard" | "learning"
  >("roster");
  const [studentId, setStudentId] = useState<number | null>(null);
  const [workRequest, setWorkRequest] = useState<WorkRequest>();
  function chooseClass(id: number) {
    if (id !== classId) setStudentId(null);
    setClassId(id);
    try {
      localStorage.setItem(CLASS_KEY, String(id));
    } catch {
      /* Optional navigation preference. */
    }
  }
  useEffect(() => {
    classesList()
      .then((rows) => {
        setClasses(rows);
        if (classId && !rows.some((row) => row.id === classId)) chooseClass(0);
      })
      .catch((reason) => setError(`班级读取失败：${String(reason)}`));
  }, []);
  function navigate(next: Section) {
    setVisited((current) => new Set([...current, next]));
    setSection(next);
  }
  function openWork(screen: WorkRequest["screen"] = "exam") {
    setWorkRequest({ id: Date.now(), screen });
    navigate("work");
  }
  return (
    <div className="app simplified-app">
      <aside className="sidebar">
        <div className="brand">
          <div className="logo">乐</div>
          <b>乐学教辅</b>
        </div>
        <nav aria-label="主要导航">
          {NAV.map((item) => (
            <button
              key={item.id}
              className={section === item.id ? "nav active" : "nav"}
              aria-current={section === item.id ? "page" : undefined}
              onClick={() => navigate(item.id)}
            >
              {item.label}
            </button>
          ))}
        </nav>
        <button
          className={
            section === "settings"
              ? "nav active settings-nav"
              : "nav settings-nav"
          }
          onClick={() => navigate("settings")}
        >
          设置
        </button>
        <div className="sidebar-foot">本地工作台 · 老师终审</div>
      </aside>
      <main className="main">
        {error && (
          <div className="error" role="alert">
            {error}
          </div>
        )}
        {/* Keep visited work mounted: switching sections must not discard an upload or a review draft. */}
        <div hidden={section !== "work"}>
          <Workspace
            active={section === "work"}
            request={workRequest}
            classes={classes}
            classId={classId}
            onClassChange={chooseClass}
            onOpenMaterials={() => navigate("materials")}
          />
        </div>
        {visited.has("materials") && (
          <div hidden={section !== "materials"}>
            <Materials onOpenExam={() => openWork()} />
          </div>
        )}
        {visited.has("students") && (
          <div hidden={section !== "students"}>
            <div className="section-toolbar">
              <div className="tabs">
                <button
                  className={studentView === "roster" ? "tab active" : "tab"}
                  onClick={() => setStudentView("roster")}
                >
                  学生名册
                </button>
                <button
                  className={studentView === "dashboard" ? "tab active" : "tab"}
                  onClick={() => setStudentView("dashboard")}
                >
                  班级学习情况
                </button>
                <button
                  className={studentView === "learning" ? "tab active" : "tab"}
                  onClick={() => setStudentView("learning")}
                >
                  错题与掌握
                </button>
              </div>
            </div>
            {studentView === "roster" && (
              <Students
                selectedClassId={classId}
                onClassChange={chooseClass}
                onClassesChanged={(rows) => {
                  setClasses(rows);
                  if (classId && !rows.some((row) => row.id === classId))
                    chooseClass(0);
                }}
                onOpenStudent={(student) => {
                  chooseClass(student.class_id ?? 0);
                  setStudentId(student.id);
                  setStudentView("learning");
                }}
              />
            )}
            {studentView === "dashboard" && (
              <ClassDashboard
                initialClassId={classId}
                onClassChange={chooseClass}
                onNavigate={(_module, view) => {
                  if (view === "students") setStudentView("roster");
                  else {
                    openWork(
                      view === "desk"
                        ? "recitation"
                        : view === "today"
                          ? "today"
                          : "exam",
                    );
                  }
                }}
                onOpenLearning={() => setStudentView("learning")}
              />
            )}
            {studentView === "learning" && (
              <LearningInsights
                initialClassId={classId}
                initialStudentId={studentId}
                onClassChange={chooseClass}
                onOpenExam={() => openWork()}
                onOpenStudents={() => setStudentView("roster")}
              />
            )}
          </div>
        )}
        {visited.has("settings") && (
          <div hidden={section !== "settings"}>
            <Settings />
          </div>
        )}
      </main>
    </div>
  );
}
