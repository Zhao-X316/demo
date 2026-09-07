---
status: "reader_test_passed_pending_explicit_code_authorization"
reader_test_status: "passed_same_model_fresh_context_2026-08-01"
code_authorized: false
source_commit: "6072360049b0"
source_exam_sha256: "2cdae5e967db845d864e36b9646a25bfd829bd98bcda6bea1d3162f68351b65d"
runtime_observed: "node_v24.15.0"
scope: "R1.1-test-enablement"
parent_contract: "./jiaofu-suite-r1-execution-contract-2026-07-31.md"
---

# 教辅系统 R1.1 前端纯函数测试启用合同

> 本文件是 R1.1 的**测试前置合同**，不是代码施工许可。2026-08-01 无上下文同模型读者复核初次发现脏文件保护、fail-fast、快照覆盖和 zsh 特殊变量问题，修正后复核无阻塞；该结果不替代项目修改记录的异模型审查。当前 `code_authorized=false`，只有另行取得明确施工许可后，才可执行第五节定义的 E1 小批次。
>
> R1-A 仍位于未提交验收工作树，修改记录异模型审查待处理。E1 即使以后自动化通过，也不得 tag、合并、推送或发布，更不得自动开始 `Exam.tsx` 抽取。

## 一、决策结论

R1.1 不直接从 3,833 行的 `src/pages/Exam.tsx` 搬函数。先建立一个零生产接线的测试前置层：

```text
E1 测试启用
  ├── 复制 6 个既有纯函数到未被生产引用的新模块
  ├── 用 Node 内置 test runner 固定当前行为
  └── 证明构建、工作树和生产入口未变化
        ↓ 另立合同、重新审查
R1.1 生产切换
  ├── Exam.tsx 改为导入已测试函数
  └── 删除原内联副本
```

E1 只证明“候选函数已有可直接运行的行为测试”。它不能证明：

- `Exam.tsx` 已变小或已完成重构；
- UI、状态编排、Tauri IPC 或老师流程发生改善；
- R2～R5 获得施工许可；
- 真实老师减负、可控试点或发布条件成立。

## 二、为什么选择 Node 内置测试

当前事实：

- 根 `package.json` 没有前端测试脚本，也没有 Vitest/Jest/tsx 依赖；
- `package.json` 已有其他批次的未提交修改，E1 不应与其重叠；
- 当前运行时为 Node `v24.15.0`，`npm run build` 已在现工作树通过；
- Node 24.12 起 type stripping 已为稳定能力，`.ts` 文件可直接执行，但 `.tsx` 不支持、导入必须带扩展名、只允许可擦除 TypeScript 语法；
- Node 自带稳定 test runner，无需增加依赖、lockfile 或 Vite 配置。

因此 E1 固定使用：

```text
node --test tests/frontend/examPure.test.ts
```

实现必须遵守 Node 的轻量 TypeScript 限制：测试和新模块都使用 `.ts`；相对导入显式写 `.ts`；不使用 enum、参数属性、运行时 namespace、路径别名或 `.tsx`。依据：[Node TypeScript 文档](https://nodejs.org/api/typescript.html)、[Node test runner 文档](https://nodejs.org/api/test.html)。

如果执行环境低于 Node 24.12，立即停止并重新审查运行时方案；不得临时安装 Vitest、Jest、tsx、Babel 或其他依赖绕过。

## 三、改前事实与风险

### 3.1 已验证事实

| 项目 | 当前事实 |
|---|---|
| 分支 / 改前提交 | `codex/t2-artifacts` / `6072360049b0` |
| `Exam.tsx` | 3,833 行；相对改前提交无 diff；SHA-256 见 frontmatter |
| Node / npm | `v24.15.0` / `11.12.1` |
| 前端构建 | `npm run build` 通过；Vite 58 modules transformed |
| Node test API 探针 | 1/1 通过 |
| 直接前端 test/spec | 未发现 |
| E1 两个目标文件 | 当前均不存在 |

这些是合同起草时的观察；真正施工前必须按第七节重跑，不能冒充未来基线。

### 3.2 脏工作树边界

当前工作树已有 R1-A 和其他批次的未提交内容，尤其 `package.json` 已修改。E1 必须：

- 不修改、暂存或格式化任何既有文件；
- 对关键保护文件保存施工前 SHA-256，并在施工后逐项校验；
- 对施工前所有已修改或未跟踪的既有普通文件保存 SHA-256，防止 porcelain 状态不变但文件内容继续变化；
- 保存完整 porcelain 状态，施工后排除两个获准新文件再逐字节比较；
- 只声称“E1 未改变受保护文件且未增加其他状态差异”，不反推更早批次的历史。

## 四、首批函数范围

E1 仅冻结下面 6 个无 React、无 Tauri、无浏览器状态的纯函数：

| 函数 | `6072360049b0:src/pages/Exam.tsx` 行 | 当前责任 |
|---|---:|---|
| `hasExtension` | 174–177 | 按允许后缀判断本地/URL 路径 |
| `fileName` | 179–181 | 从 Unix/Windows 路径取显示文件名 |
| `answerJsonLabel` | 183–203 | 把多种答案 JSON 变成老师可读摘要 |
| `shortAnswerRubricPoints` | 205–224 | 解析简答评分点并补保守默认值 |
| `fillAnswerSlots` | 226–256 | 解析填空槽位和标准答案 |
| `displayTime` | 377–379 | 把时间字符串压成分钟级显示文本 |

以下项目继续冻结，不得顺带复制或测试：

- `teacherComponentResults`、`rubricEvidencePromotions`、`initialRubricPointMappings`、`rubricPointMappingsReady`、`shortAnswerPointResults`；
- 任何标签常量、文件扩展名常量或 React 组件；
- `FixedIntakeTab`、工作台状态、hook、effect、请求编排；
- API/DTO、Tauri command、Rust、SQL、迁移和业务文案。

## 五、E1 唯一允许的未来代码改动

> 本节定义的动作仍需独立审查和明确施工许可，本文落盘本身不授权执行。

| 文件 | 状态 | 唯一允许动作 |
|---|---|---|
| `src/pages/exam/examPure.ts` | 新建 | 按固定基线原样复制第四节 6 个函数，仅在每个 `function` 前增加 `export` |
| `tests/frontend/examPure.test.ts` | 新建 | 使用 `node:test` 与 `node:assert/strict`，为 6 个函数各建立 1 个顶层测试 |

明确禁止修改：

- `src/pages/Exam.tsx`；
- `package.json`、`package-lock.json`；
- `tsconfig.json`、`tsconfig.node.json`、`vite.config.ts`；
- R1-A 两个 Rust 目标文件；
- 其他前端、脚本、文档、依赖或构建配置。

新模块在 E1 中不得被 `src/` 内任何生产文件导入。允许它被 `tests/frontend/examPure.test.ts` 直接导入；这保证测试先于生产切换存在。

## 六、固定行为夹具

测试文件固定为 6 个顶层测试；每个测试内部使用表驱动断言覆盖以下分支。

### 6.1 `hasExtension`

- 大小写后缀可识别；
- query/hash 不影响后缀；
- 不在清单内的后缀返回 false；
- 仅以相似字符结尾但没有点号时返回 false。

### 6.2 `fileName`

- Unix 路径；
- Windows 路径；
- 纯文件名；
- 末尾为分隔符时保持当前 fallback 行为。

### 6.3 `answerJsonLabel`

- `null`/空字符串返回“未识别到答案”；
- `correct_labels`、布尔 `correct`、`values`、`canonical_answers`；
- 多 `slots` 拼接及空槽 fallback；
- `reference_answer`；
- 未知但合法 JSON 和非法 JSON 都原样返回。

### 6.4 `shortAnswerRubricPoints`

- `null`、非法 JSON、缺少数组时返回空数组；
- 非对象项被过滤；
- 合法项保留 source/stable/order/text/score；
- 缺失字段使用当前的 `point-{index}`、“未识别评分点”和 0 默认值。

### 6.5 `fillAnswerSlots`

- `null`、非法 JSON、缺少数组时返回空数组；
- 非对象项被过滤；
- 内层 `canonical_answers_json` 只保留字符串答案；
- 内层 JSON 非法时保守回退；
- 缺失字段使用当前的 `slot-{index}`、“标准答案待核对”和 0 默认值。

### 6.6 `displayTime`

- ISO 字符串把 `T` 换为空格并截到分钟；
- 已是短文本时保持当前 replace/slice 行为；
- 不增加日期合法性推断或时区转换。

任何测试若暴露现有函数的可疑行为，先把它记录为 characterization；不得在 E1 中顺手修复。需要修复时另立产品行为变更合同。

## 七、施工前置与保护快照

执行 E1 前，在同一个 zsh 会话中按顺序运行：

```zsh
set -euo pipefail

E1_SNAPSHOT_DIR="$(mktemp -d /tmp/jiaofu-r1-1-e1.XXXXXX)"
test -d "$E1_SNAPSHOT_DIR"
export E1_SNAPSHOT_DIR

node -e '
  const [major, minor] = process.versions.node.split(".").map(Number);
  if (major < 24 || (major === 24 && minor < 12)) process.exit(1);
'

test "$(shasum -a 256 src/pages/Exam.tsx | awk '{print $1}')" = \
  "2cdae5e967db845d864e36b9646a25bfd829bd98bcda6bea1d3162f68351b65d"

test ! -e src/pages/exam/examPure.ts
test ! -e tests/frontend/examPure.test.ts

git status --porcelain=v1 --untracked-files=all \
  > "$E1_SNAPSHOT_DIR/status.before"

git ls-files --stage -z \
  > "$E1_SNAPSHOT_DIR/index.before"

shasum -a 256 \
  package.json package-lock.json tsconfig.json tsconfig.node.json vite.config.ts \
  src/pages/Exam.tsx \
  crates/module-exam/src/service/question_performance.rs \
  crates/module-exam/src/service/question_performance_tests.rs \
  > "$E1_SNAPSHOT_DIR/protected.before.sha256"

git diff --name-only -z HEAD -- \
  > "$E1_SNAPSHOT_DIR/tracked-dirty.before.paths"
git ls-files -o --exclude-standard -z \
  > "$E1_SNAPSHOT_DIR/untracked.before.paths"
cat \
  "$E1_SNAPSHOT_DIR/tracked-dirty.before.paths" \
  "$E1_SNAPSHOT_DIR/untracked.before.paths" \
  > "$E1_SNAPSHOT_DIR/dirty-files.before.paths"

: > "$E1_SNAPSHOT_DIR/dirty-files.before.sha256"
while IFS= read -r -d '' file_path; do
  if test -f "$file_path"; then
    shasum -a 256 "$file_path" >> "$E1_SNAPSHOT_DIR/dirty-files.before.sha256"
  fi
done < "$E1_SNAPSHOT_DIR/dirty-files.before.paths"

npm run build
```

`mktemp -d` 必须生成本批唯一快照目录，从而不覆盖任何旧验收快照；验收回执记录该目录。任一命令失败，E1 保持未开始。后续第九节所有命令必须继续在同一个已导出 `E1_SNAPSHOT_DIR` 的 zsh 会话运行；若会话丢失，停止并从本节重新开始，不手工猜测快照路径。

## 八、E1 实施顺序

1. 只新建 `src/pages/exam/examPure.ts`，从固定提交复制 6 个函数并增加 `export`。
2. 用第九节逐字节命令证明新模块与固定基线只相差导出关键字。
3. 新建 `tests/frontend/examPure.test.ts`，实现第六节 6 个顶层测试。
4. 运行直接测试；失败只允许修改新测试或新模块，使其回到既有行为，不得改 `Exam.tsx`。
5. 运行前端生产构建、生产未接线检查、保护文件哈希和全工作树状态比较。
6. 全部代码检查通过并完成工作树比较后，停止代码施工；再把 E1 验收回执和修改记录写入仓库外的权威笔记目录 `/Users/zzx/agent-shared/笔记/教辅系统/`，状态最多为“测试前置自动化通过”。不得为写回执在本仓库增加第三个文件或修改既有文件。
7. 停止。不得在同一批继续修改 `Exam.tsx`。

## 九、改后必跑验收

### 9.1 原样复制不变量

```zsh
set -euo pipefail

git show 6072360049b0:src/pages/Exam.tsx \
  | sed -n '174,257p;377,379p' \
  | sed 's/^function /export function /' \
  | cmp - src/pages/exam/examPure.ts
```

退出码必须为 0。除增加 6 个 `export` 外，不允许改名、重排、格式化或修复函数内容。

### 9.2 直接行为测试

```zsh
set -euo pipefail

node --test tests/frontend/examPure.test.ts
```

通过条件：6 tests、6 pass、0 fail、0 skipped、0 todo。

### 9.3 生产构建和未接线

```zsh
set -euo pipefail

npm run build

if wiring_output="$(rg -n 'examPure' src --glob '!examPure.ts' 2>&1)"; then
  print -r -- "$wiring_output"
  exit 1
else
  wiring_status=$?
  test "$wiring_status" -eq 1
  test -z "$wiring_output"
fi
```

通过条件：TypeScript/Vite 构建成功；除新模块自身外，`src/` 中没有生产引用。

### 9.4 受保护文件和工作树

```zsh
set -euo pipefail

test -n "${E1_SNAPSHOT_DIR:-}"
test -d "$E1_SNAPSHOT_DIR"

shasum -a 256 -c "$E1_SNAPSHOT_DIR/protected.before.sha256"

if test -s "$E1_SNAPSHOT_DIR/dirty-files.before.sha256"; then
  shasum -a 256 -c "$E1_SNAPSHOT_DIR/dirty-files.before.sha256"
fi

git status --porcelain=v1 --untracked-files=all \
  | sed \
      -e '\#^?? src/pages/exam/examPure.ts$#d' \
      -e '\#^?? tests/frontend/examPure.test.ts$#d' \
  > "$E1_SNAPSHOT_DIR/status.after-filtered"

git ls-files --stage -z \
  > "$E1_SNAPSHOT_DIR/index.after"

cmp \
  "$E1_SNAPSHOT_DIR/status.before" \
  "$E1_SNAPSHOT_DIR/status.after-filtered"

cmp \
  "$E1_SNAPSHOT_DIR/index.before" \
  "$E1_SNAPSHOT_DIR/index.after"
```

通过条件：关键保护文件哈希全部一致；施工前已经处于 staged/unstaged/untracked 的既有普通文件内容也全部一致；排除两个获准新文件后，porcelain 状态逐字节一致；完整 Git index 条目逐字节一致。四项合并后才能证明 E1 没有在既有脏路径或暂存区中藏入内容变化，也没有新增其他工作树状态。

### 9.5 新文件 whitespace

```zsh
set -euo pipefail

for file_path in \
  src/pages/exam/examPure.ts \
  tests/frontend/examPure.test.ts
do
  set +e
  output="$(git diff --no-index --check -- /dev/null "$file_path" 2>&1)"
  diff_status=$?
  set -e
  test "$diff_status" -eq 0 -o "$diff_status" -eq 1
  test -z "$output"
done
```

通过条件：两个新文件均无 whitespace 诊断，且 `git diff --no-index` 只返回预期的 0/1，不允许真实执行错误被吞掉。no-index 因文件内容不同通常返回 1。

## 十、停止条件

出现以下任一情况立即停止，不扩大范围：

- 当前 Node 低于 24.12 或 TypeScript type stripping 出现兼容问题；
- 必须修改 `package.json`、lockfile、tsconfig 或 Vite 配置才能测试；
- 必须导入 React、Tauri、DOM 或 `.tsx` 才能测试；
- 新模块无法与固定基线通过逐字节比较；
- 需要修改 `Exam.tsx` 才能让 E1 通过；
- 6 个固定测试、前端 build、保护哈希或状态比较任一失败；
- 发现候选函数存在需要业务决策的真实缺陷；
- E1 与工作树其他未提交修改发生无法安全隔离的重叠。

停止后将状态记为“合同待修订/测试前置未完成”，不得安装临时依赖或把失败测试删掉。

## 十一、完成状态与后续

E1 完成必须同时满足：

- 只新增两个获准文件；
- 原样复制、6 个直接测试、生产 build、未接线、保护哈希、状态比较和 whitespace 全部通过；
- 验收回执与修改记录完整；
- 独立代码审查无阻塞项。

允许表述：

> R1.1 纯函数测试前置已建立；6 个候选函数已有可直接运行的 characterization tests，生产仍未切换。

禁止表述：

- R1.1 已完成；
- `Exam.tsx` 已安全拆分；
- 前端架构或老师体验已改善；
- 可以直接继续 R2。

E1 完成后，若要切换生产引用，必须另立 `R1.1 纯函数外移执行合同`，重新冻结允许修改的 `Exam.tsx` 精确行段、import、改前/改后 6 个测试、构建和 UI 冒烟；不得从本合同自动继承代码许可。
