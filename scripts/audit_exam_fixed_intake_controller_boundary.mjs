import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { basename } from "node:path";
import process from "node:process";
import ts from "typescript";

const root = "src/pages/exam";
const tabPath = `${root}/FixedIntakeTab.tsx`;
const rootControllerPath = `${root}/useFixedIntakeController.ts`;
const runtimePath = `${root}/fixedIntakeControllerRuntime.ts`;
const viewModelPath = `${root}/fixedIntakeViewModel.ts`;
const commandPaths = [
  `${root}/fixedIntakeUploadCommands.ts`,
  `${root}/fixedIntakeAnswerSourceCommands.ts`,
  `${root}/fixedIntakeGroupingQualityCommands.ts`,
  `${root}/fixedIntakeOrdinaryCommands.ts`,
  `${root}/fixedIntakeAnswerSheetCommands.ts`,
  `${root}/fixedIntakeDictationCommands.ts`,
];
const targetPaths = [
  tabPath,
  rootControllerPath,
  runtimePath,
  ...commandPaths,
  viewModelPath,
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
  const splitLines = text.split(/\r?\n/);
  const lines = text.endsWith("\n") ? splitLines.length - 1 : splitLines.length;
  return { path, text, source, lines };
}

function lineOf(parsed, node) {
  return parsed.source.getLineAndCharacterOfPosition(node.getStart(parsed.source)).line + 1;
}

function analyze(parsed) {
  if (!parsed) return null;
  const facts = {
    providerCalls: [],
    dialogImports: [],
    randomUuidCalls: [],
    reactHooks: [],
    dispatchCalls: [],
    rootControllerCalls: [],
    commandImports: [],
  };

  function visit(node) {
    if (ts.isImportDeclaration(node) && ts.isStringLiteral(node.moduleSpecifier)) {
      const specifier = node.moduleSpecifier.text;
      if (specifier === "@tauri-apps/plugin-dialog") {
        facts.dialogImports.push({ line: lineOf(parsed, node), specifier });
      }
      if (/Commands(?:\.[jt]s)?$/.test(specifier)) {
        facts.commandImports.push({ line: lineOf(parsed, node), specifier });
      }
      if (specifier === "react" && node.importClause?.namedBindings
        && ts.isNamedImports(node.importClause.namedBindings)) {
        for (const element of node.importClause.namedBindings.elements) {
          const name = element.name.text;
          if (["useEffect", "useReducer", "useRef"].includes(name)) {
            facts.reactHooks.push({ line: lineOf(parsed, element), name, kind: "import" });
          }
        }
      }
    }
    if (ts.isCallExpression(node)) {
      if (ts.isIdentifier(node.expression) && /^exam[A-Z]/.test(node.expression.text)) {
        facts.providerCalls.push({ line: lineOf(parsed, node), name: node.expression.text });
      }
      if (ts.isPropertyAccessExpression(node.expression)
        && ts.isIdentifier(node.expression.expression)
        && node.expression.expression.text === "crypto"
        && node.expression.name.text === "randomUUID") {
        facts.randomUuidCalls.push({ line: lineOf(parsed, node) });
      }
      if (ts.isIdentifier(node.expression)
        && ["useEffect", "useReducer", "useRef"].includes(node.expression.text)) {
        facts.reactHooks.push({
          line: lineOf(parsed, node),
          name: node.expression.text,
          kind: "call",
        });
      }
      if (ts.isIdentifier(node.expression) && /^dispatch[A-Z]/.test(node.expression.text)) {
        facts.dispatchCalls.push({ line: lineOf(parsed, node), name: node.expression.text });
      }
      if (ts.isIdentifier(node.expression) && node.expression.text === "useFixedIntakeController") {
        facts.rootControllerCalls.push({ line: lineOf(parsed, node) });
      }
    }
    ts.forEachChild(node, visit);
  }
  visit(parsed.source);
  return facts;
}

const parsedByPath = new Map(targetPaths.map((path) => [path, parse(path)]));
const factsByPath = new Map(
  targetPaths.map((path) => [path, analyze(parsedByPath.get(path))]),
);

const allProviderCalls = [];
for (const path of targetPaths) {
  const facts = factsByPath.get(path);
  if (!facts) continue;
  for (const call of facts.providerCalls) allProviderCalls.push({ path, ...call });
}
const businessCommands = [...new Set(allProviderCalls.map((call) => call.name))].sort();
const businessCommandHash = createHash("sha256")
  .update(`${businessCommands.join("\n")}\n`)
  .digest("hex");

const expectedCommandCount = 23;
const expectedCallsiteCount = 26;
const expectedCommandHash = "375a66bfade8b24a2d2ffb180bb934494754e4d5cff25401bd04b8e7e4358ba4";
const inventoryErrors = [];
if (businessCommands.length !== expectedCommandCount) {
  inventoryErrors.push(`business_commands expected ${expectedCommandCount}, actual ${businessCommands.length}`);
}
if (allProviderCalls.length !== expectedCallsiteCount) {
  inventoryErrors.push(`provider_callsites expected ${expectedCallsiteCount}, actual ${allProviderCalls.length}`);
}
if (businessCommandHash !== expectedCommandHash) {
  inventoryErrors.push(`business_command_hash expected ${expectedCommandHash}, actual ${businessCommandHash}`);
}

const tab = parsedByPath.get(tabPath);
const tabFacts = factsByPath.get(tabPath);
const rootController = parsedByPath.get(rootControllerPath);
const rootFacts = factsByPath.get(rootControllerPath);
const runtime = parsedByPath.get(runtimePath);
const runtimeFacts = factsByPath.get(runtimePath);
const viewModel = parsedByPath.get(viewModelPath);
const viewFacts = factsByPath.get(viewModelPath);
const existingTargets = targetPaths.filter((path) => parsedByPath.get(path));
const allowedDialogFiles = new Set([
  `${root}/fixedIntakeUploadCommands.ts`,
  `${root}/fixedIntakeGroupingQualityCommands.ts`,
  `${root}/fixedIntakeAnswerSheetCommands.ts`,
  `${root}/fixedIntakeDictationCommands.ts`,
]);

const checks = [];
function check(name, pass, detail) {
  checks.push({ name, pass, detail });
}

check("target_files_exist", existingTargets.length === targetPaths.length,
  `${existingTargets.length}/${targetPaths.length}`);
check("tab_line_budget", Boolean(tab && tab.lines <= 350), tab ? `${tab.lines}/350` : "missing");
check("tab_provider_free", tabFacts?.providerCalls.length === 0,
  `${tabFacts?.providerCalls.length ?? "missing"}`);
check("tab_dialog_free", tabFacts?.dialogImports.length === 0,
  `${tabFacts?.dialogImports.length ?? "missing"}`);
check("tab_uuid_free", tabFacts?.randomUuidCalls.length === 0,
  `${tabFacts?.randomUuidCalls.length ?? "missing"}`);
check("tab_runtime_hooks_free", tabFacts?.reactHooks.length === 0,
  `${tabFacts?.reactHooks.length ?? "missing"}`);
check("tab_dispatch_free", tabFacts?.dispatchCalls.length === 0,
  `${tabFacts?.dispatchCalls.length ?? "missing"}`);
check("tab_root_controller_once", tabFacts?.rootControllerCalls.length === 1,
  `${tabFacts?.rootControllerCalls.length ?? "missing"}`);
check("root_controller_line_budget", Boolean(rootController && rootController.lines <= 350),
  rootController ? `${rootController.lines}/350` : "missing");
check("root_controller_provider_free", rootFacts?.providerCalls.length === 0,
  `${rootFacts?.providerCalls.length ?? "missing"}`);
check("runtime_line_budget", Boolean(runtime && runtime.lines <= 300),
  runtime ? `${runtime.lines}/300` : "missing");
check("runtime_provider_free", runtimeFacts?.providerCalls.length === 0,
  `${runtimeFacts?.providerCalls.length ?? "missing"}`);
check("view_model_line_budget", Boolean(viewModel && viewModel.lines <= 300),
  viewModel ? `${viewModel.lines}/300` : "missing");
check("view_model_pure", Boolean(viewFacts
  && viewFacts.providerCalls.length === 0
  && viewFacts.reactHooks.length === 0
  && viewFacts.dispatchCalls.length === 0),
  viewFacts
    ? `provider=${viewFacts.providerCalls.length},hooks=${viewFacts.reactHooks.length},dispatch=${viewFacts.dispatchCalls.length}`
    : "missing");

const oversizedCommands = commandPaths.flatMap((path) => {
  const parsed = parsedByPath.get(path);
  return parsed && parsed.lines > 400 ? [`${basename(path)}:${parsed.lines}`] : [];
});
check("command_line_budgets", commandPaths.every((path) => {
  const parsed = parsedByPath.get(path);
  return Boolean(parsed && parsed.lines <= 400);
}), oversizedCommands.length ? oversizedCommands.join(",") : "missing-or-ok");

const nonCommandProviderCalls = allProviderCalls.filter((call) => !commandPaths.includes(call.path));
check("providers_owned_by_commands", nonCommandProviderCalls.length === 0,
  `${nonCommandProviderCalls.length}`);

const illegalDialogs = targetPaths.flatMap((path) => {
  const facts = factsByPath.get(path);
  if (!facts || allowedDialogFiles.has(path)) return [];
  return facts.dialogImports.map((entry) => `${path}:${entry.line}`);
});
check("dialogs_owned_by_allowed_commands", illegalDialogs.length === 0,
  illegalDialogs.length ? illegalDialogs.join(",") : "0");

const commandCycles = commandPaths.flatMap((path) => {
  const facts = factsByPath.get(path);
  if (!facts) return [];
  return facts.commandImports.map((entry) => `${path}:${entry.line}->${entry.specifier}`);
});
check("command_modules_do_not_import_each_other", commandCycles.length === 0,
  commandCycles.length ? commandCycles.join(",") : "0");

const requiredRootImports = [
  "fixedIntakeControllerRuntime",
  "fixedIntakeUploadCommands",
  "fixedIntakeAnswerSourceCommands",
  "fixedIntakeGroupingQualityCommands",
  "fixedIntakeOrdinaryCommands",
  "fixedIntakeAnswerSheetCommands",
  "fixedIntakeDictationCommands",
  "fixedIntakeViewModel",
];
const rootText = rootController?.text ?? "";
const missingRootImports = requiredRootImports.filter((name) => !rootText.includes(name));
check("root_controller_composes_all_modules", missingRootImports.length === 0,
  missingRootImports.length ? missingRootImports.join(",") : "all");

console.log("R3-C1 fixed intake controller boundary audit");
console.log(`business_commands=${businessCommands.length}`);
console.log(`provider_callsites=${allProviderCalls.length}`);
console.log(`business_command_hash=${businessCommandHash}`);
console.log("file\tstatus\tlines\tproviders");
for (const path of targetPaths) {
  const parsed = parsedByPath.get(path);
  const facts = factsByPath.get(path);
  console.log(`${path}\t${parsed ? "present" : "missing"}\t${parsed?.lines ?? "-"}\t${facts?.providerCalls.length ?? "-"}`);
}

console.log("check\tstatus\tdetail");
for (const entry of checks) {
  console.log(`${entry.name}\t${entry.pass ? "PASS" : "FAIL"}\t${entry.detail}`);
}

if (inventoryErrors.length) {
  console.log("inventory_errors:");
  inventoryErrors.forEach((error) => console.log(`inventory_error\t${error}`));
}

const gaps = checks.filter((entry) => !entry.pass);
if (gaps.length) {
  console.log("boundary_gaps:");
  gaps.forEach((entry) => console.log(`boundary_gap\t${entry.name}\t${entry.detail}`));
}
const passed = checks.length - gaps.length;
console.log(`summary=checks ${passed}/${checks.length}`);

if (inventoryErrors.length || gaps.length) process.exitCode = 1;
