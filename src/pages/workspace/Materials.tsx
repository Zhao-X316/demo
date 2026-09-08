import { useState } from "react";
import QuestionBank from "../QuestionBank";
import Library from "../Library";
import Exam from "../Exam";

export default function Materials({ onOpenExam }: { onOpenExam: () => void }) {
  const [section, setSection] = useState<"questions" | "recitation" | "tools">(
    "questions",
  );
  const [visited, setVisited] = useState(new Set(["questions"]));
  const choose = (next: typeof section) => {
    setVisited((current) => new Set([...current, next]));
    setSection(next);
  };
  return (
    <>
      <div className="section-toolbar">
        <div className="tabs">
          <button
            className={section === "questions" ? "tab active" : "tab"}
            onClick={() => choose("questions")}
          >
            题目
          </button>
          <button
            className={section === "recitation" ? "tab active" : "tab"}
            onClick={() => choose("recitation")}
          >
            背诵内容
          </button>
        </div>
        <details>
          <summary>更多资料工具</summary>
          <button onClick={() => choose("tools")}>原有题库与知识点维护</button>
        </details>
      </div>
      {
        <div hidden={section !== "questions"}>
          <QuestionBank onOpenExam={onOpenExam} />
        </div>
      }
      {visited.has("recitation") && (
        <div hidden={section !== "recitation"}>
          <Library />
        </div>
      )}
      {visited.has("tools") && (
        <div hidden={section !== "tools"}>
          <Exam initialTool="questions" />
        </div>
      )}
    </>
  );
}
