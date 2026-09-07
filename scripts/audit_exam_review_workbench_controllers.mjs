import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import process from "node:process";
import ts from "typescript";

const root = "src/pages/exam";
const workbenches = {
  objective: {
    tab: `${root}/ObjectiveReviewTab.tsx`,
    controller: `${root}/useObjectiveReviewController.ts`,
    hook: "useObjectiveReviewController",
    expectedCalls: 5,
    requiredSemantic: ["selectAssessment", "selectItem", "changeManualScore", "changeManualNote"],
  },
  dictation: {
    tab: `${root}/DictationReviewTab.tsx`,
    controller: `${root}/useDictationReviewController.ts`,
    hook: "useDictationReviewController",
    expectedCalls: 6,
    requiredSemantic: [
      "selectAssessment", "selectItem", "changeCorrection", "changeManualScore",
      "changeManualNote", "changeManualEvidence",
    ],
  },
  subjective: {
    tab: `${root}/SubjectiveReviewTab.tsx`,
    controller: `${root}/useSubjectiveReviewController.ts`,
    hook: "useSubjectiveReviewController",
    expectedCalls: 11,
    requiredSemantic: [
      "selectAssessment", "selectItem", "changeCorrection", "changeManualScore",
      "changeManualNote", "changeComponentScore", "changeComponentEvidence",
      "changeComponentNote",
    ],
  },
};
const editorPath = `${root}/SubjectiveComponentEditor.tsx`;
const examPath = "src/pages/Exam.tsx";
const expectedCommandCount = 20;
const expectedCallsiteCount = 22;
const expectedCommandHash = "ef7b4508e81cb5983816a19e9e1b612a2505806628bce232d602afefb899772f";
const rawPublicNames = [
  "setAssessmentVersionId", "setAssessmentItemId", "setCorrections",
  "setManualScores", "setManualNotes", "setManualEvidence",
  "setComponentScores", "setComponentEvidence", "setComponentNotes",
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

function propertyName(node) {
  const name = node.name;
  if (ts.isIdentifier(name) || ts.isStringLiteral(name) || ts.isNumericLiteral(name)) return name.text;
  return null;
}

function analyze(parsed) {
  if (!parsed) return null;
  const facts = {
    providerCalls: [],
    stateCalls: 0,
    effectCalls: 0,
    uuidCalls: 0,
    confirmCalls: 0,
    hookCalls: new Map(),
    imports: [],
    identifiers: new Set(),
    publicActions: new Set(),
    interfaceProperties: new Set(),
  };
  function visit(node) {
    if (ts.isIdentifier(node)) facts.identifiers.add(node.text);
    if (ts.isImportDeclaration(node) && ts.isStringLiteral(node.moduleSpecifier)) {
      facts.imports.push(node.moduleSpecifier.text);
    }
    if (ts.isPropertySignature(node)) {
      const name = propertyName(node);
      if (name) facts.interfaceProperties.add(name);
    }
    if (ts.isPropertyAssignment(node)
      && propertyName(node) === "actions"
      && ts.isObjectLiteralExpression(node.initializer)) {
      for (const property of node.initializer.properties) {
        if (ts.isShorthandPropertyAssignment(property)) facts.publicActions.add(property.name.text);
        else if (ts.isPropertyAssignment(property)) {
          const name = propertyName(property);
          if (name) facts.publicActions.add(name);
        }
      }
    }
    if (ts.isCallExpression(node)) {
      if (ts.isIdentifier(node.expression)) {
        const name = node.expression.text;
        if (/^exam[A-Z]/.test(name)) facts.providerCalls.push(name);
        if (name === "useState") facts.stateCalls += 1;
        if (name === "useEffect") facts.effectCalls += 1;
        if (/^use(?:Objective|Dictation|Subjective)ReviewController$/.test(name)) {
          facts.hookCalls.set(name, (facts.hookCalls.get(name) ?? 0) + 1);
        }
      }
      if (ts.isPropertyAccessExpression(node.expression)) {
        const owner = node.expression.expression;
        const name = node.expression.name.text;
        if (ts.isIdentifier(owner) && owner.text === "crypto" && name === "randomUUID") facts.uuidCalls += 1;
        if (ts.isIdentifier(owner) && owner.text === "window" && name === "confirm") facts.confirmCalls += 1;
      }
    }
    ts.forEachChild(node, visit);
  }
  visit(parsed.source);
  return facts;
}

const parsed = new Map();
for (const item of Object.values(workbenches)) {
  parsed.set(item.tab, parse(item.tab));
  parsed.set(item.controller, parse(item.controller));
}
parsed.set(editorPath, parse(editorPath));
parsed.set(examPath, parse(examPath));
const facts = new Map([...parsed].map(([path, value]) => [path, analyze(value)]));

const allWorkBenchPaths = Object.values(workbenches).flatMap((item) => [item.tab, item.controller]);
const allCalls = allWorkBenchPaths.flatMap((path) => facts.get(path)?.providerCalls ?? []);
const commands = [...new Set(allCalls)].sort();
const commandHash = createHash("sha256").update(`${commands.join("\n")}\n`).digest("hex");
const inventoryErrors = [];
if (commands.length !== expectedCommandCount) inventoryErrors.push(`commands expected ${expectedCommandCount}, actual ${commands.length}`);
if (allCalls.length !== expectedCallsiteCount) inventoryErrors.push(`calls expected ${expectedCallsiteCount}, actual ${allCalls.length}`);
if (commandHash !== expectedCommandHash) inventoryErrors.push(`hash expected ${expectedCommandHash}, actual ${commandHash}`);

const checks = [];
function check(name, pass, detail) {
  checks.push({ name, pass, detail });
}

const tabCount = Object.values(workbenches).filter((item) => parsed.get(item.tab)).length;
const controllerCount = Object.values(workbenches).filter((item) => parsed.get(item.controller)).length;
check("target_tabs_exist", tabCount === 3, `${tabCount}/3`);
check("target_controllers_exist", controllerCount === 3, `${controllerCount}/3`);
check("provider_inventory", inventoryErrors.length === 0,
  inventoryErrors.length ? inventoryErrors.join(";") : `commands=${commands.length};calls=${allCalls.length};hash=${commandHash}`);

for (const [key, item] of Object.entries(workbenches)) {
  const tabFacts = facts.get(item.tab);
  const controllerFacts = facts.get(item.controller);
  const totalForWorkbench = (tabFacts?.providerCalls.length ?? 0) + (controllerFacts?.providerCalls.length ?? 0);
  check(`${key}_provider_inventory`, totalForWorkbench === item.expectedCalls, `${totalForWorkbench}/${item.expectedCalls}`);
  check(`${key}_tab_provider_free`, tabFacts?.providerCalls.length === 0, `${tabFacts?.providerCalls.length ?? "missing"}`);
  check(`${key}_tab_state_effect_free`, Boolean(tabFacts && tabFacts.stateCalls === 0 && tabFacts.effectCalls === 0),
    tabFacts ? `state=${tabFacts.stateCalls};effect=${tabFacts.effectCalls}` : "missing");
  check(`${key}_tab_side_effect_primitives_free`, Boolean(tabFacts && tabFacts.uuidCalls === 0 && tabFacts.confirmCalls === 0),
    tabFacts ? `uuid=${tabFacts.uuidCalls};confirm=${tabFacts.confirmCalls}` : "missing");
  check(`${key}_controller_once`, tabFacts?.hookCalls.get(item.hook) === 1, `${tabFacts?.hookCalls.get(item.hook) ?? 0}/1`);
  check(`${key}_providers_owned_by_controller`, controllerFacts?.providerCalls.length === item.expectedCalls,
    `${controllerFacts?.providerCalls.length ?? "missing"}/${item.expectedCalls}`);
  const publicRaw = rawPublicNames.filter((name) => controllerFacts?.publicActions.has(name));
  const missingSemantic = item.requiredSemantic.filter((name) => !controllerFacts?.publicActions.has(name));
  check(`${key}_public_actions_semantic`, Boolean(controllerFacts && publicRaw.length === 0 && missingSemantic.length === 0),
    `raw=${publicRaw.join(",") || "0"};missing=${missingSemantic.join(",") || "0"}`);
}

const controllerImports = Object.values(workbenches).flatMap((item) => {
  const current = facts.get(item.controller);
  if (!current) return [];
  return current.imports.filter((specifier) => /use(?:Objective|Dictation|Subjective)ReviewController/.test(specifier));
});
check("controllers_do_not_import_each_other", controllerImports.length === 0,
  controllerImports.length ? controllerImports.join(",") : "0");

const editorFacts = facts.get(editorPath);
const editorRaw = rawPublicNames.filter((name) => editorFacts?.identifiers.has(name) || editorFacts?.interfaceProperties.has(name));
const editorSemantic = ["changeComponentScore", "changeComponentEvidence", "changeComponentNote", "changeManualNote"];
const editorMissing = editorSemantic.filter((name) => !editorFacts?.identifiers.has(name));
check("subjective_editor_semantic", Boolean(editorFacts && editorRaw.length === 0 && editorMissing.length === 0),
  `raw=${editorRaw.join(",") || "0"};missing=${editorMissing.join(",") || "0"}`);

const examFacts = facts.get(examPath);
const examControllerImports = examFacts?.imports.filter((specifier) => /ReviewController/.test(specifier)) ?? [];
check("exam_parent_controller_free", examControllerImports.length === 0,
  examControllerImports.length ? examControllerImports.join(",") : "0");

console.log("R3-W1 review workbench controller audit");
console.log(`provider_commands=${commands.length}`);
console.log(`provider_callsites=${allCalls.length}`);
console.log(`provider_command_hash=${commandHash}`);
console.log("workbench\ttab_providers\tcontroller_providers\tstate\teffect\tuuid\tconfirm");
for (const [key, item] of Object.entries(workbenches)) {
  const tabFacts = facts.get(item.tab);
  const controllerFacts = facts.get(item.controller);
  console.log(`${key}\t${tabFacts?.providerCalls.length ?? "-"}\t${controllerFacts?.providerCalls.length ?? "-"}\t${tabFacts?.stateCalls ?? "-"}\t${tabFacts?.effectCalls ?? "-"}\t${tabFacts?.uuidCalls ?? "-"}\t${tabFacts?.confirmCalls ?? "-"}`);
}
console.log("check\tstatus\tdetail");
for (const entry of checks) console.log(`${entry.name}\t${entry.pass ? "PASS" : "FAIL"}\t${entry.detail}`);
const gaps = checks.filter((entry) => !entry.pass);
if (gaps.length) {
  console.log("workbench_controller_gaps:");
  for (const entry of gaps) console.log(`workbench_controller_gap\t${entry.name}\t${entry.detail}`);
}
console.log(`summary=checks ${checks.length - gaps.length}/${checks.length}`);
if (inventoryErrors.length || gaps.length) process.exitCode = 1;
