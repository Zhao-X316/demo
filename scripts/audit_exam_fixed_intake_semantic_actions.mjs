import { existsSync, readFileSync } from "node:fs";
import process from "node:process";
import ts from "typescript";

const root = "src/pages/exam";
const paths = {
  tab: `${root}/FixedIntakeTab.tsx`,
  form: `${root}/FixedIntakeUploadForm.tsx`,
  controller: `${root}/useFixedIntakeController.ts`,
  upload: `${root}/fixedIntakeUploadCommands.ts`,
};
const rawNames = [
  "setClassId",
  "setAssessmentVersionId",
  "setStudentPaths",
  "setAnswerPath",
  "setAnswerText",
  "setExpectedPages",
  "clearAnswer",
];
const externalSemanticNames = [
  "selectClass",
  "selectAssessment",
  "changeExpectedPages",
  "changeAnswerText",
  "clearAnswerSource",
];
const formSemanticNames = [
  ...externalSemanticNames,
  "pickStudentPapers",
  "pickAnswer",
];
const internalSemanticNames = ["selectStudentFiles", "selectAnswerFile"];
const reducerEventTokens = [
  "class_selected",
  "assessment_selected",
  "student_paths_selected",
  "answer_file_selected",
  "answer_text_changed",
  "expected_pages_changed",
  "answer_cleared",
];

function parse(path) {
  if (!existsSync(path)) return null;
  const text = readFileSync(path, "utf8");
  const source = ts.createSourceFile(
    path,
    text,
    ts.ScriptTarget.Latest,
    true,
    path.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS,
  );
  return { path, text, source };
}

function lineOf(parsed, node) {
  return parsed.source.getLineAndCharacterOfPosition(node.getStart(parsed.source)).line + 1;
}

function propertyName(node) {
  const name = node.name;
  if (!name) return null;
  if (ts.isIdentifier(name) || ts.isStringLiteral(name) || ts.isNumericLiteral(name)) {
    return name.text;
  }
  return null;
}

function analyze(parsed) {
  if (!parsed) return null;
  const facts = {
    identifiers: new Map(),
    interfaceProperties: new Set(),
    publicActionProperties: new Set(),
  };

  function addIdentifier(name, node) {
    const entries = facts.identifiers.get(name) ?? [];
    entries.push(lineOf(parsed, node));
    facts.identifiers.set(name, entries);
  }

  function visit(node) {
    if (ts.isIdentifier(node)) addIdentifier(node.text, node);
    if (ts.isPropertySignature(node)) {
      const name = propertyName(node);
      if (name) facts.interfaceProperties.add(name);
    }
    if (ts.isPropertyAssignment(node)
      && propertyName(node) === "actions"
      && ts.isObjectLiteralExpression(node.initializer)) {
      for (const property of node.initializer.properties) {
        if (ts.isShorthandPropertyAssignment(property)) {
          facts.publicActionProperties.add(property.name.text);
        } else if (ts.isPropertyAssignment(property)) {
          const name = propertyName(property);
          if (name) facts.publicActionProperties.add(name);
        }
      }
    }
    ts.forEachChild(node, visit);
  }
  visit(parsed.source);
  return facts;
}

const parsed = Object.fromEntries(
  Object.entries(paths).map(([key, path]) => [key, parse(path)]),
);
const facts = Object.fromEntries(
  Object.entries(parsed).map(([key, value]) => [key, analyze(value)]),
);

function occurrences(key, names) {
  return names.flatMap((name) =>
    (facts[key]?.identifiers.get(name) ?? []).map((line) => `${name}@${line}`)
  );
}

const checks = [];
function check(name, pass, detail) {
  checks.push({ name, pass, detail });
}

const existing = Object.values(parsed).filter(Boolean).length;
check("target_files_exist", existing === Object.keys(paths).length,
  `${existing}/${Object.keys(paths).length}`);

const controllerText = parsed.controller?.text ?? "";
const missingEvents = reducerEventTokens.filter((token) => !controllerText.includes(`\"${token}\"`));
check("reducer_event_inventory", missingEvents.length === 0,
  missingEvents.length ? missingEvents.join(",") : "7/7");

for (const key of ["form", "tab", "controller", "upload"]) {
  const found = occurrences(key, rawNames);
  check(`${key}_raw_setters_free`, found.length === 0,
    found.length ? found.join(",") : "0");
}

const formProperties = facts.form?.interfaceProperties ?? new Set();
const missingFormActions = formSemanticNames.filter((name) => !formProperties.has(name));
check("form_semantic_actions_complete", missingFormActions.length === 0,
  missingFormActions.length ? `missing=${missingFormActions.join(",")}` : `${formSemanticNames.length}/${formSemanticNames.length}`);

const missingTabActions = formSemanticNames.filter((name) =>
  (facts.tab?.identifiers.get(name) ?? []).length === 0
);
check("tab_semantic_actions_complete", missingTabActions.length === 0,
  missingTabActions.length ? `missing=${missingTabActions.join(",")}` : `${formSemanticNames.length}/${formSemanticNames.length}`);

const publicActions = facts.controller?.publicActionProperties ?? new Set();
const publicRaw = rawNames.filter((name) => publicActions.has(name));
const missingPublicSemantic = externalSemanticNames.filter((name) => !publicActions.has(name));
check("controller_public_actions_semantic", publicRaw.length === 0 && missingPublicSemantic.length === 0,
  `raw=${publicRaw.join(",") || "0"};missing=${missingPublicSemantic.join(",") || "0"}`);

const missingInternal = internalSemanticNames.filter((name) =>
  (facts.controller?.identifiers.get(name) ?? []).length === 0
  || (facts.upload?.identifiers.get(name) ?? []).length === 0
);
const leakedInternal = internalSemanticNames.filter((name) => publicActions.has(name));
check("internal_file_actions_private", missingInternal.length === 0 && leakedInternal.length === 0,
  `missing=${missingInternal.join(",") || "0"};public=${leakedInternal.join(",") || "0"}`);

console.log("R3-C2 fixed intake semantic action audit");
console.log("check\tstatus\tdetail");
for (const entry of checks) {
  console.log(`${entry.name}\t${entry.pass ? "PASS" : "FAIL"}\t${entry.detail}`);
}
const gaps = checks.filter((entry) => !entry.pass);
if (gaps.length) {
  console.log("semantic_action_gaps:");
  gaps.forEach((entry) => console.log(`semantic_action_gap\t${entry.name}\t${entry.detail}`));
}
console.log(`summary=checks ${checks.length - gaps.length}/${checks.length}`);
if (gaps.length) process.exitCode = 1;
