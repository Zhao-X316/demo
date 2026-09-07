import { existsSync, readFileSync } from "node:fs";
import process from "node:process";
import ts from "typescript";

const sourcePaths = [
  "src/pages/exam/FixedIntakeTab.tsx",
  "src/pages/exam/fixedIntakeUploadCommands.ts",
  "src/pages/exam/fixedIntakeAnswerSourceCommands.ts",
  "src/pages/exam/fixedIntakeGroupingQualityCommands.ts",
  "src/pages/exam/fixedIntakeOrdinaryCommands.ts",
  "src/pages/exam/fixedIntakeAnswerSheetCommands.ts",
  "src/pages/exam/fixedIntakeDictationCommands.ts",
].filter((path) => existsSync(path));

const operationContracts = {
  page_cycle: { label: "页周期推断", keyPrefix: "page-cycle:" },
  prepare: { label: "批次准备", keyPrefix: "prepare:" },
  answer_source: { label: "答案资料分析", keyPrefix: "answer-source:" },
  answer_resolution: { label: "答案资料确认", keyPrefix: "answer-resolution:" },
  material: { label: "材料类型确认", keyPrefix: "material:" },
  grouping: { label: "学生归组确认", keyPrefix: "grouping:" },
  grouping_evidence: { label: "归组证据读取", keyPrefix: "grouping-evidence:" },
  quality: { label: "质量确认", keyPrefix: "quality:" },
  retake: { label: "单页重拍", keyPrefix: "retake:" },
  ordinary_analyze: { label: "普通卷分析", keyPrefix: "ordinary-analyze:" },
  ordinary_confirm: { label: "普通卷确认与识别", keyPrefix: "ordinary-confirm:" },
  answer_sheet_template_status: {
    label: "答题卡模板状态读取",
    keyPrefix: "answer-sheet-template-status:",
  },
  answer_sheet_template: { label: "答题卡模板操作", keyPrefix: "answer-sheet-template:" },
  answer_sheet_page: { label: "答题卡学生页处理", keyPrefix: "answer-sheet-page:" },
  dictation_template_status: {
    label: "默写模板状态读取",
    keyPrefix: "dictation-template-status:",
  },
  dictation_template: { label: "默写模板操作", keyPrefix: "dictation-template:" },
  dictation_page: { label: "默写学生页处理", keyPrefix: "dictation-page:" },
};

const ownerOperations = new Map([
  ["pickStudentPapers", "page_cycle"],
  ["submit", "prepare"],
  ["analyzeAnswerSource", "answer_source"],
  ["confirmMatchingAnswerSource", "answer_resolution"],
  ["keepCurrentBoundAnswers", "answer_resolution"],
  ["adoptAnswerSourceAsNewVersion", "answer_resolution"],
  ["confirmMaterialType", "material"],
  ["confirmGrouping", "grouping"],
  ["startGroupingEvidenceReadEffect", "grouping_evidence"],
  ["confirmGroupingQuality", "quality"],
  ["replaceRejectedPage", "retake"],
  ["analyzeOrdinaryPages", "ordinary_analyze"],
  ["confirmReadyOrdinaryPages", "ordinary_confirm"],
  ["startAnswerSheetTemplateStatusReadEffect", "answer_sheet_template_status"],
  ["pickAndAnalyzeAnswerSheetTemplate", "answer_sheet_template"],
  ["confirmAnswerSheetTemplate", "answer_sheet_template"],
  ["processAnswerSheetPages", "answer_sheet_page"],
  ["startDictationTemplateStatusReadEffect", "dictation_template_status"],
  ["pickAndAnalyzeDictationTemplate", "dictation_template"],
  ["confirmDictationTemplate", "dictation_template"],
  ["processDictationPages", "dictation_page"],
]);

const effectOperations = new Map([
  ["examFixedIntakeGroupingEvidence", "grouping_evidence"],
  ["examAnswerSheetTemplateStatus", "answer_sheet_template_status"],
  ["examDictationTemplateStatus", "dictation_template_status"],
]);

function namedOwner(node, sourceFile) {
  let current = node;
  while (current && current !== sourceFile) {
    if (ts.isFunctionDeclaration(current) && current.name) {
      return { name: current.name.text, node: current };
    }
    if (
      (ts.isArrowFunction(current) || ts.isFunctionExpression(current))
      && ts.isVariableDeclaration(current.parent)
      && ts.isIdentifier(current.parent.name)
    ) {
      return { name: current.parent.name.text, node: current };
    }
    if (
      (ts.isArrowFunction(current) || ts.isFunctionExpression(current))
      && ts.isCallExpression(current.parent)
      && ts.isIdentifier(current.parent.expression)
      && current.parent.expression.text === "useEffect"
    ) {
      return { name: "useEffect", node: current };
    }
    current = current.parent;
  }
  return null;
}

function lineOf(sourceFile, node) {
  return sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1;
}

const callsites = [];
for (const sourcePath of sourcePaths) {
  const sourceText = readFileSync(sourcePath, "utf8");
  const sourceFile = ts.createSourceFile(
    sourcePath,
    sourceText,
    ts.ScriptTarget.Latest,
    true,
    sourcePath.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS,
  );

  function visit(node) {
    if (
      ts.isCallExpression(node)
      && ts.isIdentifier(node.expression)
      && /^exam[A-Z]/.test(node.expression.text)
    ) {
      const owner = namedOwner(node, sourceFile);
      if (!owner) {
        throw new Error(
          `无法定位 ${node.expression.text} 的所属函数，${sourcePath}:${lineOf(sourceFile, node)}`,
        );
      }
      const provider = node.expression.text;
      const operation = owner.name === "useEffect"
        ? effectOperations.get(provider)
        : ownerOperations.get(owner.name);
      if (!operation) {
        throw new Error(
          `未登记 provider 调用：${provider} @ ${sourcePath}:${owner.name}:${lineOf(sourceFile, node)}`,
        );
      }
      const contract = operationContracts[operation];
      const ownerText = owner.node.getText(sourceFile);
      const claimed = ownerText.includes("claimOperation")
        && ownerText.includes(contract.keyPrefix);
      const guardedByRunner = ownerText.includes("startFixedIntakeReadEffect")
        && ownerText.includes("isCurrent:");
      const guardedByCompletion = ownerText.includes("FIXED_INTAKE_COMPLETION_RECEIVED")
        || ownerText.includes("FIXED_INTAKE_PAGE_CYCLE_RECEIVED");
      const guarded = guardedByRunner
        || (guardedByCompletion && ownerText.includes("completionIsCurrent"));
      callsites.push({
        sourcePath,
        operation,
        provider,
        owner: owner.name,
        line: lineOf(sourceFile, node),
        claimed,
        guarded,
      });
    }
    ts.forEachChild(node, visit);
  }
  visit(sourceFile);
}

const expectedProviderCounts = new Map([
  ["examAnswerSheetAnalyzeTemplate", 1],
  ["examAnswerSheetConfirmTemplate", 1],
  ["examAnswerSheetProcessPage", 1],
  ["examAnswerSheetTemplateStatus", 2],
  ["examAnswerSourceAdoptNewVersion", 1],
  ["examAnswerSourceAnalyze", 1],
  ["examAnswerSourceConfirmMatches", 1],
  ["examAnswerSourceKeepBound", 1],
  ["examDictationAnalyzeTemplate", 1],
  ["examDictationConfirmTemplate", 1],
  ["examDictationProcessPage", 1],
  ["examDictationTemplateStatus", 1],
  ["examFixedIntakeConfirmGrouping", 1],
  ["examFixedIntakeConfirmGroupingQuality", 1],
  ["examFixedIntakeConfirmMaterialType", 1],
  ["examFixedIntakeGroupingEvidence", 3],
  ["examFixedIntakeInferPageCycle", 1],
  ["examFixedIntakePrepare", 1],
  ["examFixedIntakeReplaceRejectedPage", 1],
  ["examObjectiveRecognizeRegion", 1],
  ["examOrdinaryPaperAnalyzePage", 1],
  ["examOrdinaryPaperConfirmPageStructure", 1],
  ["examOrdinaryPaperSyncQuestions", 1],
]);

const actualProviderCounts = new Map();
for (const callsite of callsites) {
  actualProviderCounts.set(
    callsite.provider,
    (actualProviderCounts.get(callsite.provider) ?? 0) + 1,
  );
}

const inventoryErrors = [];
for (const [provider, expected] of expectedProviderCounts) {
  const actual = actualProviderCounts.get(provider) ?? 0;
  if (actual !== expected) inventoryErrors.push(`${provider}: expected ${expected}, actual ${actual}`);
}
for (const [provider, actual] of actualProviderCounts) {
  if (!expectedProviderCounts.has(provider)) inventoryErrors.push(`${provider}: unregistered ${actual}`);
}

const businessCommands = [...actualProviderCounts.keys()].sort();
const rows = [];
for (const [operation, contract] of Object.entries(operationContracts)) {
  const operationCalls = callsites.filter((callsite) => callsite.operation === operation);
  const protectedCalls = operationCalls.filter((callsite) => callsite.claimed && callsite.guarded);
  const status = protectedCalls.length === operationCalls.length && operationCalls.length > 0
    ? "FULL"
    : protectedCalls.length > 0
      ? "PARTIAL"
      : "MISSING";
  rows.push({ operation, ...contract, calls: operationCalls.length, protected: protectedCalls.length, status });
}

console.log("R3-H1 lifecycle coverage audit");
console.log(`sources=${sourcePaths.join(",")}`);
console.log(`business_commands=${businessCommands.length}`);
console.log(`provider_callsites=${callsites.length}`);
console.log("operation\tcalls\tprotected\tstatus\tkey");
for (const row of rows) {
  console.log(`${row.operation}\t${row.calls}\t${row.protected}\t${row.status}\t${row.keyPrefix}`);
}

const gaps = callsites.filter((callsite) => !callsite.claimed || !callsite.guarded);
if (gaps.length) {
  console.log("gaps:");
  for (const gap of gaps) {
    const missing = [
      gap.claimed ? null : "owner-claim",
      gap.guarded ? null : "stale-guard",
    ].filter(Boolean).join(",");
    console.log(
      `${gap.sourcePath}:${gap.line}\t${gap.operation}\t${gap.owner}\t${gap.provider}\tmissing=${missing}`,
    );
  }
}

const fullOperations = rows.filter((row) => row.status === "FULL").length;
const protectedCallsites = callsites.length - gaps.length;
console.log(
  `summary=operations ${fullOperations}/${rows.length}; callsites ${protectedCallsites}/${callsites.length}`,
);

if (
  inventoryErrors.length
  || businessCommands.length !== 23
  || callsites.length !== 26
  || fullOperations !== rows.length
  || gaps.length
) {
  if (inventoryErrors.length) {
    console.log("inventory_errors:");
    inventoryErrors.forEach((error) => console.log(error));
  }
  process.exitCode = 1;
}
