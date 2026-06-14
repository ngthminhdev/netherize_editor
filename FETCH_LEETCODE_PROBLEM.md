# Fetch LeetCode Problem

Go ten/ID/URL bai LeetCode → tu dong fetch code template + test cases, san sang F5 chay.

## Quick Start

1. `Cmd+Shift+P` → **"Fetch LeetCode Problem"**
2. Nhap `1` hoac `two-sum` hoac `https://leetcode.com/problems/two-sum/`
3. Chon ngon ngu (hoac auto-detect tu file dang mo)
4. File duoc tao voi code template + test cases co san
5. `F5` de chay

## Input Formats

| Format | Vi du |
|--------|-------|
| Problem ID | `1`, `15`, `42` |
| Title slug | `two-sum`, `3sum`, `trapping-rain-water` |
| Full URL | `https://leetcode.com/problems/two-sum/description/` |

## Language Selection

- Neu dang mo file `.py`, `.js`, `.ts`, `.go`, `.rs`, `.rb` → tu dong dung ngon ngu do
- Neu khong → hien language picker (Python / JavaScript / TypeScript / Go / Rust / Ruby)
- MRU-sorted, giong `NewLeetCodeFile`

## AI Enhancement (Settings)

Trong Settings tab, muc AI → toggle **LeetCode AI**:

| Setting | Behavior |
|---------|----------|
| **Off** (default) | Map co hoc: lay code snippet tu LeetCode API, wrap vao template `solve(data)`. Nhanh, khong can AI config. |
| **On** | Gui snippet + problem context cho AI provider (`config/ai.toml`) → AI adapt snippet ve format `solve()` chuan. Xu ly duoc class, ListNode, TreeNode... |

Config trong `config/ai.toml`:

```toml
[leetcode]
use_ai = false  # default: khong dung AI
```

Khi bat, tam thoi tai su dung AI provider cua inline completion:

```toml
[inline_completion.provider]
api_url = "http://localhost:20128/v1"
model = "mistral/devstral-2512"
api_key = "sk-..."
```

Sau nay co the tach provider AI manh hon de generate them test case, giai thich
bai va enrich problem context ma khong thay doi fetch workflow.

## Test Cases

Tu dong fill tu example cua bai tren LeetCode:

- **Input**: JSON object voi cac tham so tu `metaData` + `exampleTestcaseList`
- **Expected**: Parse tu HTML content (regex Input/Output blocks)
- Neu parse expected that bai → de `""` kem hint

Vi du twoSum:

| # | Input | Expected |
|---|-------|----------|
| 1 | `{"nums":[2,7,11,15],"target":9}` | `[0,1]` |
| 2 | `{"nums":[3,2,4],"target":6}` | `[1,2]` |
| 3 | `{"nums":[3,3],"target":6}` | `[0,1]` |

## Generated Code Template

### JavaScript (twoSum example)

```js
const { readFileSync } = require("fs");

function twoSum(nums, target) {
  // TODO: implement
}

function solve(data) {
  const { nums, target } = JSON.parse(data);
  return twoSum(nums, target);
}

function main() {
  const data = readFileSync(0, "utf8");
  const result = solve(data);
  process.stdout.write(JSON.stringify(result) + "\n");
}

main();
```

### Python

```python
import sys
from typing import List

def twoSum(nums: List[int], target: int) -> List[int]:
    # TODO: implement
    pass

def solve(data: str) -> str:
    import json
    params = json.loads(data)
    result = twoSum(params["nums"], params["target"])
    return json.dumps(result)

def main() -> None:
    print(solve(sys.stdin.read()).rstrip())

if __name__ == "__main__":
    main()
```

## Architecture

```
┌─────────────────────────────────────────────────┐
│ Command: FetchLeetCodeProblem                   │
│   → mo palette nhap ID/slug/URL                 │
└──────────────────┬──────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────┐
│ Command: FetchLeetCodeConfirmSelection          │
│   → gui LeetCodeFetchRequest qua mpsc           │
└──────────────────┬──────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────┐
│ Async Worker (tokio::spawn)                     │
│   1. resolve_slug(input) → titleSlug            │
│   2. reqwest → leetcode.com/graphql             │
│   3. parse response → LeetCodeProblem            │
│   4. extract_examples(content) → test cases      │
│   5. if use_ai: adapt_snippet_via_ai()          │
│      else:      adapt_snippet_mechanical()       │
│   6. send LeetCodeProblemResult via mpsc         │
└──────────────────┬──────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────┐
│ Result Handler                                  │
│   1. create file (leetcode temp dir)             │
│   2. write generated code                       │
│   3. populate test_runner_state.cases            │
│   4. open buffer + show toast                   │
└─────────────────────────────────────────────────┘
```

## New/Modified Files

| File | Purpose |
|------|---------|
| `src/runner/leetcode_api.rs` | LeetCode GraphQL types, query builder, response parser, HTML example extractor |
| `src/runner/leetcode_adapter.rs` | Code snippet → solve() template (mechanical + AI paths) |
| `src/async_runtime/scheduler/leetcode_fetch.rs` | Async worker: fetch LeetCode + optional AI call |
| `src/async_runtime/message.rs` | New payloads: `LeetCodeFetchRequest`, `LeetCodeProblemResult` |
| `src/app/event_loop/async_results/leetcode_fetch.rs` | Result handler: create file + fill cases |
| `src/core/commands.rs` | New variants: `FetchLeetCodeProblem`, `FetchLeetCodeConfirmSelection`, `ToggleLeetCodeAi` |
| `src/core/command_ids.rs` | New IDs: `FETCH_LEETCODE_PROBLEM`, `TOGGLE_LEETCODE_AI` |
| `src/app/command_palette.rs` | New palette mode: `LeetCodeProblemInput` |
| `src/app/app_state/palette.rs` | New: `open_leetcode_problem_input()` |
| `src/app/event_loop/commands_terminal.rs` | Handlers for fetch confirm, result processing |
| `src/app/event_loop/commands_settings_helpers.rs` | Toggle handler for LeetCode AI setting |
| `src/app/app_state/settings.rs` | New `SettingItem::LeetCodeAi(bool)` |
| `src/render/renderer/editor/settings.rs` | Render toggle row |
| `src/config/ai_config.rs` | New `LeetCodeConfig` struct |
| `config/ai.toml` | New `[leetcode]` section |
| `FETCH_LEETCODE_PROBLEM.md` | This file |

## Key Design Decisions

1. **Khong block main thread**: LeetCode API call chay trong async worker qua `tokio::spawn` + `mpsc`
2. **Anh huong toi workspace**: rescan sau khi tao file
3. **MRU cho ngon ngu**: Tai dung `PersistentState::recent_leetcode_languages`
4. **AI fallback**: Neu AI call fails, fallback ve mechanical adaptation
5. **Test case parsing**: Regex `<pre>` blocks tu HTML content, fallback ve empty neu parse fail
6. **File naming**: `solution.{ext}` hoac `solution-{n}.{ext}` neu trung ten, giong NewLeetCodeFile
