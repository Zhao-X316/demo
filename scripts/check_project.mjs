// Run the existing checks from one entry, including the independent Tauri crate.
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const scope = process.argv[2] ?? "local";
const scopes = ["local", "frontend", "backend", "ui"];
if (!scopes.includes(scope) || process.argv.length > 4) {
  console.error("Usage: node scripts/check_project.mjs [local|frontend|backend|ui] [log-directory]");
  process.exit(2);
}
const output = process.argv[3]
  ? path.resolve(process.argv[3])
  : mkdtempSync(path.join(tmpdir(), "jiaofu-check-"));
mkdirSync(output, { recursive: true });
const files = (directory, matches) => readdirSync(path.join(root, directory))
  .filter(matches).sort().map((name) => `${directory}/${name}`);
const node = process.execPath;
const frontend = [
  ["frontend-tests", node, "--experimental-strip-types", "--test",
    ...files("tests/frontend", (name) => name.endsWith(".test.ts"))],
  ...files("scripts", (name) => name.startsWith("audit_exam_") && name.endsWith(".mjs"))
    .map((file) => [path.basename(file, ".mjs"), node, file]),
  ["build", "npm", "run", "build"],
];
const backend = [
  ["workspace-tests", "cargo", "test", "--workspace"],
  ["tauri-tests", "cargo", "test", "--manifest-path", "src-tauri/Cargo.toml"],
  ["workspace-clippy", "cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
  ["tauri-clippy", "cargo", "clippy", "--manifest-path", "src-tauri/Cargo.toml", "--all-targets", "--", "-D", "warnings"],
];
const ui = files("scripts", (name) => /^test_.*_ui\.py$/.test(name))
  .map((file) => [path.basename(file, ".py"), "python3", file]);
const checks = { local: [...frontend, ...backend], frontend, backend, ui }[scope];

if (scope === "ui") {
  // The browser scripts install their own synthetic Tauri mocks.
  try {
    const response = await fetch("http://127.0.0.1:4173", { signal: AbortSignal.timeout(3000) });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
  } catch {
    console.error("Start the local UI first: npm run dev -- --host 127.0.0.1 --port 4173 --strictPort");
    process.exit(2);
  }
}

console.log(`Checks: ${scope}; logs: ${output}`);
const results = [];
for (const [name, command, ...args] of checks) {
  console.log(`Running ${name}...`);
  const started = Date.now();
  const result = spawnSync(command, args, { cwd: root, encoding: "utf8", maxBuffer: 32 * 1024 * 1024 });
  const log = path.join(output, `${name}.log`);
  writeFileSync(log, (result.stdout ?? "") + (result.stderr ?? "") + (result.error ? `\n${result.error.message}\n` : ""));
  const passed = result.status === 0 && !result.error;
  results.push({ name, command: [command, ...args], exitCode: result.status,
    signal: result.signal, passed, seconds: (Date.now() - started) / 1000, log });
  writeFileSync(path.join(output, "results.json"), JSON.stringify(results, null, 2) + "\n");
  console.log(`${passed ? "PASS" : "FAIL"} ${name} (${results.at(-1).seconds}s)${passed ? "" : `; ${log}`}`);
  if (result.signal === "SIGINT" || result.signal === "SIGTERM") break;
}
const passed = results.length === checks.length && results.every((result) => result.passed);
console.log(`${passed ? "PASS" : "FAIL"}: ${results.filter((result) => result.passed).length}/${checks.length}; ${output}`);
process.exitCode = passed ? 0 : 1;
