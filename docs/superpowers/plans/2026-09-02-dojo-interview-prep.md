# Dojo (Interview-Prep) + AI Interviewer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A right-dock "Dojo" tab that lets the user pick any NeetCode 150 problem (grouped by pattern) or system-design case, runs a hard 25'/45' timed session with an approach gate, writes an error-notebook entry, schedules spaced redos, shows progress/streak, and launches a Claude "Interviewer" agent.

**Architecture:** Pure logic lives in a new `src/dojo/` module (plan/problem parsing, state + SRS, session timer, notebook formatting, panel row model) with unit tests and no UI. Editor integration follows the golden data flow: `g o` keymap → `Command::DojoOpen` → `command_dispatch` passthrough → `commands_dojo.rs` handlers on `AppShell`; panel keys are routed by `route_dojo_input` (same pattern as Test Runner/Outline). Rendering reuses the `test_runner` surface (like Outline). Fetch reuses the existing LeetCode fetch job; small state files use the existing sync `atomic_write` (state.toml precedent); notebook/current.md writes go through a new fire-and-forget worker job.

**Tech Stack:** Rust, serde + toml (existing), regex (existing), chrono 0.4 (new, for local dates), winit/wgpu renderer helpers (existing).

**Spec:** `docs/superpowers/specs/2026-09-02-dojo-interview-prep-design.md`

## Global Constraints

- **Never commit.** The human commits. Every task ends with "leave uncommitted" (user rule overrides the skill template).
- No `.unwrap()` / `.expect()` outside `#[cfg(test)]` (AGENTS.md). Use `Result<_, String>` like `runner/leetcode_api.rs`.
- No file I/O in the render loop; worker for notebook/current.md/SD template writes; `dojo.toml` saves are sync `atomic_write` (same precedent as `state.toml`).
- Input routing hooks must be added in BOTH `route_normalized_input` and `route_repeated_normalized_input` (lessons #113).
- Use `/usr/bin/grep` in the shell — the `rtk grep` hook rewrite returns false "0 matches" (AGENTS.md).
- Run `cargo test --lib <filter>` per task; full `cargo test --lib` + `cargo clippy --all-targets` (no NEW warnings) before handing to the user.
- New dependency: `chrono = "0.4"` (default features). Nothing else.
- UI strings are Vietnamese exactly as written here (they match the spec).
- Structure changes → update `README.md` (layout tree, Where To Fix What, Quick Status) and run `npx gitnexus analyze` (AGENTS.md), done in Task 16.

---

## File map

| File | Responsibility |
|---|---|
| `config/dojo/neetcode150.toml` (new) | 150 problems: id, slug, title, category, difficulty |
| `config/dojo/plan.toml` (new) | groups, sd_cases, notebook path, timer budgets |
| `src/dojo/mod.rs` (new) | module root |
| `src/dojo/problems.rs` (new) | `Problem`, `Problems` parse/load/lookup |
| `src/dojo/plan.rs` (new) | `Plan`, `Group`, `SdCase`, `Page`, page order, group membership |
| `src/dojo/state.rs` (new) | `DojoState`, `Attempt`, `ProblemProgress`, `ActiveSession`, SRS, streak, progress, load/save |
| `src/dojo/session.rs` (new) | `SessionKind`, `phase_at`, elapsed/remaining/expired |
| `src/dojo/notebook.rs` (new) | `html_to_text`, `format_block`, `format_sd_block`, `mm_ss` |
| `src/dojo/files.rs` (new) | `expand_tilde`, `dojo_dir`, `current_md`, `sd_template`, `INTERVIEWER_PROMPT` |
| `src/dojo/interviewer_prompt.md` (new) | default interviewer system prompt (include_str) |
| `src/dojo/view.rs` (new) | `DojoRow`, `DojoHeader`, `list_rows`, `header`, `suggested_next`, `wrap_text` |
| `src/app/event_loop/commands_dojo.rs` (new) | `DojoRuntime` on AppShell + all Dojo handlers |
| `src/app/event_loop/async_results/dojo.rs` (new) | `WriteTextFilesResult` handler |
| `src/workbench/panel_state.rs` | `PanelTabId::Dojo` |
| `src/app/input_map/mod.rs` | `InputFocusContext::Dojo` |
| `src/app/input/handler.rs` | `route_dojo_input` + 2 hook sites |
| `src/app/event_loop/setup.rs` | focus mapping, `DojoRuntime` init |
| `src/core/commands.rs`, `src/core/command_ids.rs`, `src/core/command_dispatch/mod.rs` | `Command::Dojo*`, `dojo.open`, passthrough |
| `config/keymaps/default.toml`, `src/app/command_palette.rs` | `g o`, palette entry, `DojoApproach`/`DojoNote` modes |
| `src/core/command_dispatch/editing.rs` | paste allow-list for the new prompt modes |
| `src/app/event_loop/commands_palette.rs` | confirm branches for `DojoApproach`/`DojoNote` |
| `src/app/event_loop/commands_terminal.rs` | `submit_leetcode_fetch` visibility |
| `src/app/event_loop/async_results/leetcode_fetch.rs` | dojo branch on fetch result |
| `src/app/event_loop/async_results/runner.rs` | all-passed hook |
| `src/async_runtime/message.rs`, `src/async_runtime/scheduler/dispatch.rs` | `WriteTextFiles` job |
| `src/render/renderer/ui/test_runner.rs` | `build_dojo_content` + `update_right_dock_panel` param |
| `src/render/renderer/ui/statusbar.rs` | timer chip param |
| `src/render/renderer/ui/welcome.rs` | Dojo card param |
| `src/app/event_loop/application.rs` | right-dock model, statusbar/welcome call sites, `about_to_wait` tick |
| `src/app/ai_agents.rs` | `interviewer` agent |
| `README.md`, `docs/project-knowledge/lessons.md` | docs |

---

### Task 1: Data files + `dojo::problems`

**Files:**
- Modify: `Cargo.toml` (add `chrono = "0.4"` under `[dependencies]`)
- Create: `config/dojo/neetcode150.toml`
- Create: `src/dojo/mod.rs`, `src/dojo/problems.rs`
- Modify: `src/lib.rs` (add `pub mod dojo;` next to `pub mod runner;`)

**Interfaces:**
- Produces: `dojo::problems::{Problem, Problems}`, `Problems::{parse, bundled, load, by_slug, len, is_empty}`, `BUNDLED_PROBLEMS`.

- [ ] **Step 1: Generate the problem list** (data, not code). Run this script; it joins the public NeetCode 150 index with LeetCode's public id map. All 150 slugs resolved to ids at plan-writing time (no misses).

```bash
cd "$(mktemp -d)"
curl -sL -o nc.json https://raw.githubusercontent.com/krmanik/Anki-NeetCode/main/neetcode-150-list.json
curl -sL -A "Mozilla/5.0" -o lc.json https://leetcode.com/api/problems/algorithms/
python3 - <<'EOF'
import json
nc=json.load(open('nc.json')); lc=json.load(open('lc.json'))
idmap={p['stat']['question__title_slug']:p['stat']['frontend_question_id'] for p in lc['stat_status_pairs']}
keymap={"Arrays & Hashing":"arrays_hashing","Two Pointers":"two_pointers","Sliding Window":"sliding_window","Stack":"stack","Binary Search":"binary_search","Linked List":"linked_list","Trees":"trees","Tries":"tries","Heap / Priority Queue":"heap","Backtracking":"backtracking","Graphs":"graphs","Advanced Graphs":"advanced_graphs","1-D Dynamic Programming":"dp_1d","2-D Dynamic Programming":"dp_2d","Greedy":"greedy","Intervals":"intervals","Math & Geometry":"math_geometry","Bit Manipulation":"bit_manipulation"}
out=[]
for cat,probs in nc.items():
    for title,p in probs.items():
        slug=p['url'].rstrip('/').split('/')[-1]
        out.append((idmap[slug],slug,title,keymap[cat],p['difficulty'].lower()))
assert len(out)==150
with open('neetcode150.toml','w') as f:
    f.write("# NeetCode 150 — shipped with Netherize Dojo. Edit freely; the app reloads on start.\n# category keys: arrays_hashing two_pointers sliding_window stack binary_search linked_list trees tries heap backtracking graphs advanced_graphs dp_1d dp_2d greedy intervals math_geometry bit_manipulation\n\n")
    for pid,slug,title,k,d in out:
        f.write(f'[[problem]]\nid = {pid}\nslug = "{slug}"\ntitle = "{title.replace(chr(34), chr(92)+chr(34))}"\ncategory = "{k}"\ndifficulty = "{d}"\n\n')
EOF
mkdir -p /Users/qc-bright/Project/netherize_editor/config/dojo
cp neetcode150.toml /Users/qc-bright/Project/netherize_editor/config/dojo/neetcode150.toml
```

Expected category sizes (assert in the test): trees 15, graphs 13, dp_1d 12, linked_list 11, dp_2d 11, backtracking 10, arrays_hashing 9, greedy 8, math_geometry 8, binary_search 7, heap 7, bit_manipulation 7, sliding_window 6, stack 6, advanced_graphs 6, intervals 6, two_pointers 5, tries 3 = 150.

- [ ] **Step 2: Add chrono + module root**

`Cargo.toml` `[dependencies]`: `chrono = "0.4"`.

`src/lib.rs`: add `pub mod dojo;` (alphabetical, after `pub mod core;` or wherever the list keeps order).

`src/dojo/mod.rs`:
```rust
//! Dojo — interview-prep practice loop (problem menu, timed sessions, error
//! notebook, spaced redo). Pure logic only; editor wiring lives in
//! `app/event_loop/commands_dojo.rs`.
pub mod files;
pub mod notebook;
pub mod plan;
pub mod problems;
pub mod session;
pub mod state;
pub mod view;
```
(Only `problems` exists after this task — add the other `pub mod` lines as each task creates its file, or create empty files now. Creating them now with a `//!` doc line each is fine.)

- [ ] **Step 3: Write the failing tests** in `src/dojo/problems.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_problem_table() {
        let p = Problems::parse(
            "[[problem]]\nid = 1\nslug = \"two-sum\"\ntitle = \"Two Sum\"\ncategory = \"arrays_hashing\"\ndifficulty = \"easy\"\n",
        )
        .expect("parse");
        assert_eq!(p.len(), 1);
        assert_eq!(p.by_slug("two-sum").map(|x| x.id), Some(1));
        assert!(p.by_slug("nope").is_none());
    }

    #[test]
    fn empty_list_is_an_error() {
        assert!(Problems::parse("").is_err());
    }

    #[test]
    fn bundled_list_is_150_unique_and_categorised() {
        let p = Problems::bundled();
        assert_eq!(p.len(), 150);
        let mut slugs: Vec<&str> = p.problems.iter().map(|x| x.slug.as_str()).collect();
        slugs.sort_unstable();
        slugs.dedup();
        assert_eq!(slugs.len(), 150, "duplicate slug");
        assert!(p.problems.iter().all(|x| x.id > 0 && !x.title.is_empty()));
        let known = [
            "arrays_hashing", "two_pointers", "sliding_window", "stack", "binary_search",
            "linked_list", "trees", "tries", "heap", "backtracking", "graphs",
            "advanced_graphs", "dp_1d", "dp_2d", "greedy", "intervals", "math_geometry",
            "bit_manipulation",
        ];
        for x in &p.problems {
            assert!(known.contains(&x.category.as_str()), "unknown category {}", x.category);
        }
        let count = |c: &str| p.problems.iter().filter(|x| x.category == c).count();
        assert_eq!(count("arrays_hashing"), 9);
        assert_eq!(count("two_pointers"), 5);
        assert_eq!(count("trees"), 15);
        assert_eq!(count("tries"), 3);
    }

    #[test]
    fn load_prefers_a_valid_override_and_falls_back_otherwise() {
        let dir = std::env::temp_dir().join(format!("dojo_problems_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("neetcode150.toml");
        assert_eq!(Problems::load(&path).len(), 150, "missing file → bundled");
        std::fs::write(&path, "[[problem]]\nid = 7\nslug = \"x\"\ntitle = \"X\"\ncategory = \"stack\"\n").expect("write");
        assert_eq!(Problems::load(&path).len(), 1, "override wins");
        std::fs::write(&path, "not toml [[").expect("write");
        assert_eq!(Problems::load(&path).len(), 150, "broken override → bundled");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

- [ ] **Step 4: Run to verify they fail**

Run: `cargo test --lib dojo::problems`
Expected: compile error (module/types missing).

- [ ] **Step 5: Implement `src/dojo/problems.rs`**

```rust
//! NeetCode 150 problem list (`config/dojo/neetcode150.toml`, user override in
//! `~/.config/netherize/dojo/neetcode150.toml`).
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Problem {
    pub id: u32,
    pub slug: String,
    pub title: String,
    pub category: String,
    #[serde(default)]
    pub difficulty: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Problems {
    #[serde(default, rename = "problem")]
    pub problems: Vec<Problem>,
}

pub const BUNDLED_PROBLEMS: &str = include_str!("../../config/dojo/neetcode150.toml");

impl Problems {
    pub fn parse(text: &str) -> Result<Self, String> {
        let parsed: Self =
            toml::from_str(text).map_err(|err| format!("invalid problem list: {err}"))?;
        if parsed.problems.is_empty() {
            return Err("problem list is empty".to_string());
        }
        Ok(parsed)
    }

    pub fn bundled() -> Self {
        Self::parse(BUNDLED_PROBLEMS).unwrap_or_else(|err| {
            eprintln!("[dojo] bundled problem list is broken: {err}");
            Self::default()
        })
    }

    /// A user override wins when it exists and parses; otherwise the bundled list.
    pub fn load(user_override: &Path) -> Self {
        match std::fs::read_to_string(user_override) {
            Ok(text) => Self::parse(&text).unwrap_or_else(|err| {
                eprintln!("[dojo] {}: {err}", user_override.display());
                Self::bundled()
            }),
            Err(_) => Self::bundled(),
        }
    }

    pub fn by_slug(&self, slug: &str) -> Option<&Problem> {
        self.problems.iter().find(|p| p.slug == slug)
    }

    pub fn len(&self) -> usize {
        self.problems.len()
    }

    pub fn is_empty(&self) -> bool {
        self.problems.is_empty()
    }
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test --lib dojo::problems`
Expected: 4 passed.

- [ ] **Step 7: Leave uncommitted** (human commits).

---

### Task 2: `config/dojo/plan.toml` + `dojo::plan`

**Files:**
- Create: `config/dojo/plan.toml`, `src/dojo/plan.rs`

**Interfaces:**
- Consumes: `Problems`, `Problem`.
- Produces: `Plan { notebook, sd_dir, dsa_minutes, dsa_phases: Vec<(String, u32)>, sd_minutes, groups: Vec<Group>, sd_cases: Vec<SdCase> }`, `Group { key, label, categories, note }`, `SdCase { key, label, topic }`, `Page::{Group(usize), Sd}`, `Plan::{parse, bundled, load, pages, group_for_category, group_problems, page_by_key, page_key, page_label, dsa_budget_s, sd_budget_s}`.

- [ ] **Step 1: Write `config/dojo/plan.toml`**

```toml
# Netherize Dojo plan. Pages, not a calendar — open any group any time.
notebook = "~/Work/docs/interview-notes.md"
sd_dir = "~/Work/docs/sd"
dsa_minutes = 25
dsa_phases = [["THINK", 3], ["CODE", 15], ["TEST", 5], ["REVIEW", 2]]
sd_minutes = 45

[[group]]
key = "hash_two_pointers"
label = "Array/Hash Map, Two Pointers"
categories = ["arrays_hashing", "two_pointers"]
note = "Đổi O(n²)→O(n) bằng hash; template two pointer."

[[group]]
key = "sliding_stack"
label = "Sliding Window, Stack"
categories = ["sliding_window", "stack"]
note = "Window co giãn; monotonic stack."

[[group]]
key = "bsearch_list"
label = "Binary Search, Linked List"
categories = ["binary_search", "linked_list"]
note = "Bounds lo/hi không off-by-one; fast/slow pointer."

[[group]]
key = "tree_trie"
label = "Tree (DFS/BFS), Trie"
categories = ["trees", "tries"]
note = "Đệ quy trên cây; BFS theo tầng."

[[group]]
key = "heap_bt_interval"
label = "Heap, Backtracking, Interval"
categories = ["heap", "backtracking", "intervals"]
note = "Top-K bằng heap; cắt nhánh; merge interval."

[[group]]
key = "graph"
label = "Graph (BFS/DFS/Topo/Union-Find)"
categories = ["graphs", "advanced_graphs"]
note = "Topological sort; detect cycle; DSU. CHECKPOINT: 2 medium / 45'."

[[group]]
key = "extra"
label = "Extra: DP, Greedy, Math, Bit"
categories = ["dp_1d", "dp_2d", "greedy", "math_geometry", "bit_manipulation"]
note = "Sau 6 nhóm chính. Không bắt buộc."

[[sd_case]]
key = "wallet_v1"
label = "Ví điện tử / chuyển tiền (bản 1)"
topic = "Scaling: LB, replication, sharding, CAP, consistency levels"

[[sd_case]]
key = "url_shortener"
label = "Rút gọn URL"
topic = "Cache-aside vs write-through, invalidation, queue"

[[sd_case]]
key = "rate_limiter"
label = "Rate limiter phân tán"
topic = "Storage: SQL vs NoSQL, index, LSM vs B-tree, hot partition"

[[sd_case]]
key = "leaderboard"
label = "Bảng xếp hạng realtime"
topic = "Reliability: idempotency, outbox, saga, circuit breaker, backpressure"

[[sd_case]]
key = "feed"
label = "Feed mạng xã hội"
topic = "Fan-out on write vs read"

[[sd_case]]
key = "chat_notification"
label = "Chat / notification"
topic = "WebSocket, delivery guarantee"

[[sd_case]]
key = "log_metrics"
label = "Thu thập & xử lý log/metrics"
topic = "Streaming, at-least-once + idempotent consumer"

[[sd_case]]
key = "wallet_v2"
label = "Ví điện tử (bản 2) — so với bản 1"
topic = "Vận hành: observability, rate limit, deploy an toàn, feature flag"
```

- [ ] **Step 2: Write the failing tests** (`src/dojo/plan.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::dojo::problems::Problems;

    #[test]
    fn bundled_plan_has_seven_groups_eight_cases_and_defaults() {
        let plan = Plan::bundled();
        assert_eq!(plan.groups.len(), 7);
        assert_eq!(plan.sd_cases.len(), 8);
        assert_eq!(plan.dsa_minutes, 25);
        assert_eq!(plan.dsa_phases.len(), 4);
        assert_eq!(plan.dsa_budget_s(), 25 * 60);
        assert_eq!(plan.sd_budget_s(), 45 * 60);
        assert_eq!(plan.pages().len(), 8);
        assert_eq!(plan.pages()[7], Page::Sd);
    }

    #[test]
    fn missing_fields_take_defaults() {
        let plan = Plan::parse("[[group]]\nkey = \"a\"\nlabel = \"A\"\ncategories = [\"stack\"]\n").expect("parse");
        assert_eq!(plan.dsa_minutes, 25);
        assert_eq!(plan.sd_minutes, 45);
        assert_eq!(plan.dsa_phases[0], ("THINK".to_string(), 3));
        assert_eq!(plan.notebook, "~/Work/docs/interview-notes.md");
        assert_eq!(plan.pages(), vec![Page::Group(0)], "no sd_cases → no Sd page");
    }

    #[test]
    fn every_bundled_problem_lands_in_exactly_one_group() {
        let plan = Plan::bundled();
        let problems = Problems::bundled();
        let mut total = 0;
        for idx in 0..plan.groups.len() {
            total += plan.group_problems(idx, &problems).len();
        }
        assert_eq!(total, 150);
        assert_eq!(plan.group_problems(0, &problems).len(), 14);
        assert_eq!(plan.group_for_category("dp_2d"), Some(6));
        assert_eq!(plan.group_for_category("made_up"), Some(6), "unknown → last group");
    }

    #[test]
    fn page_keys_round_trip() {
        let plan = Plan::bundled();
        assert_eq!(plan.page_key(Page::Sd), "sd");
        assert_eq!(plan.page_key(Page::Group(1)), "sliding_stack");
        assert_eq!(plan.page_by_key("sliding_stack"), Some(Page::Group(1)));
        assert_eq!(plan.page_by_key("sd"), Some(Page::Sd));
        assert_eq!(plan.page_by_key("zzz"), None);
        assert_eq!(plan.page_label(Page::Group(1)), "Sliding Window, Stack");
        assert_eq!(plan.page_label(Page::Sd), "System Design");
    }
}
```

- [ ] **Step 3: Run to verify failure**: `cargo test --lib dojo::plan` → compile error.

- [ ] **Step 4: Implement `src/dojo/plan.rs`**

```rust
//! Dojo plan: pattern groups (pages) + system-design cases + timer budgets.
use std::path::Path;

use serde::Deserialize;

use super::problems::{Problem, Problems};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Group {
    pub key: String,
    pub label: String,
    pub categories: Vec<String>,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SdCase {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub topic: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Plan {
    #[serde(default = "default_notebook")]
    pub notebook: String,
    #[serde(default = "default_sd_dir")]
    pub sd_dir: String,
    #[serde(default = "default_dsa_minutes")]
    pub dsa_minutes: u32,
    #[serde(default = "default_phases")]
    pub dsa_phases: Vec<(String, u32)>,
    #[serde(default = "default_sd_minutes")]
    pub sd_minutes: u32,
    #[serde(default, rename = "group")]
    pub groups: Vec<Group>,
    #[serde(default, rename = "sd_case")]
    pub sd_cases: Vec<SdCase>,
}

fn default_notebook() -> String { "~/Work/docs/interview-notes.md".to_string() }
fn default_sd_dir() -> String { "~/Work/docs/sd".to_string() }
fn default_dsa_minutes() -> u32 { 25 }
fn default_sd_minutes() -> u32 { 45 }
fn default_phases() -> Vec<(String, u32)> {
    vec![
        ("THINK".to_string(), 3),
        ("CODE".to_string(), 15),
        ("TEST".to_string(), 5),
        ("REVIEW".to_string(), 2),
    ]
}

/// One browsable page of the Dojo list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Group(usize),
    Sd,
}

pub const BUNDLED_PLAN: &str = include_str!("../../config/dojo/plan.toml");

impl Plan {
    pub fn parse(text: &str) -> Result<Self, String> {
        let plan: Self = toml::from_str(text).map_err(|err| format!("invalid plan: {err}"))?;
        if plan.groups.is_empty() {
            return Err("plan has no [[group]]".to_string());
        }
        Ok(plan)
    }

    pub fn bundled() -> Self {
        Self::parse(BUNDLED_PLAN).unwrap_or_else(|err| {
            eprintln!("[dojo] bundled plan is broken: {err}");
            Self {
                notebook: default_notebook(),
                sd_dir: default_sd_dir(),
                dsa_minutes: 25,
                dsa_phases: default_phases(),
                sd_minutes: 45,
                groups: Vec::new(),
                sd_cases: Vec::new(),
            }
        })
    }

    pub fn load(user_override: &Path) -> Self {
        match std::fs::read_to_string(user_override) {
            Ok(text) => Self::parse(&text).unwrap_or_else(|err| {
                eprintln!("[dojo] {}: {err}", user_override.display());
                Self::bundled()
            }),
            Err(_) => Self::bundled(),
        }
    }

    /// Groups in file order, then the SD page when there are cases.
    pub fn pages(&self) -> Vec<Page> {
        let mut pages: Vec<Page> = (0..self.groups.len()).map(Page::Group).collect();
        if !self.sd_cases.is_empty() {
            pages.push(Page::Sd);
        }
        pages
    }

    /// First group listing `category`; unmatched categories fall into the last group.
    pub fn group_for_category(&self, category: &str) -> Option<usize> {
        self.groups
            .iter()
            .position(|g| g.categories.iter().any(|c| c == category))
            .or_else(|| self.groups.len().checked_sub(1))
    }

    pub fn group_problems<'a>(&self, idx: usize, problems: &'a Problems) -> Vec<&'a Problem> {
        problems
            .problems
            .iter()
            .filter(|p| self.group_for_category(&p.category) == Some(idx))
            .collect()
    }

    pub fn page_key(&self, page: Page) -> String {
        match page {
            Page::Sd => "sd".to_string(),
            Page::Group(i) => self.groups.get(i).map(|g| g.key.clone()).unwrap_or_default(),
        }
    }

    pub fn page_by_key(&self, key: &str) -> Option<Page> {
        if key == "sd" {
            return (!self.sd_cases.is_empty()).then_some(Page::Sd);
        }
        self.groups.iter().position(|g| g.key == key).map(Page::Group)
    }

    pub fn page_label(&self, page: Page) -> String {
        match page {
            Page::Sd => "System Design".to_string(),
            Page::Group(i) => self.groups.get(i).map(|g| g.label.clone()).unwrap_or_default(),
        }
    }

    pub fn dsa_budget_s(&self) -> u64 {
        u64::from(self.dsa_minutes) * 60
    }

    pub fn sd_budget_s(&self) -> u64 {
        u64::from(self.sd_minutes) * 60
    }
}
```

- [ ] **Step 5: Run**: `cargo test --lib dojo::plan` → 4 passed.
- [ ] **Step 6: Leave uncommitted.**

---

### Task 3: `dojo::state` — progress, SRS, streak, persistence

**Files:**
- Create: `src/dojo/state.rs`

**Interfaces:**
- Consumes: `crate::app::persistence::atomic_write` (already `pub(crate)`), `crate::config::paths::user_config_root`, chrono.
- Produces:
  - `Outcome::{Pass, Fail, Timeout, Giveup}` (serde lowercase), `Status::{Todo, Redo, Done}`, `SessionKind::{Dsa, Sd}` (lives in `session.rs`, Task 4 — for this task define it in `state.rs` and `pub use` it from `session.rs` later, OR create `session.rs` first with just the enum. Do the latter: Task 3 Step 0 creates `src/dojo/session.rs` containing only `SessionKind`.)
  - `Attempt { slug, kind, started_unix, ended_unix, outcome, elapsed_s, approach }`
  - `ProblemProgress { status, redo_at: Option<String>, passes }`
  - `ActiveSession { kind, slug, started_unix, budget_s, approach: Option<String>, file: PathBuf, title: String }`
  - `DojoState { last_group: Option<String>, active_session: Option<ActiveSession>, attempts: Vec<Attempt>, problem: BTreeMap<String, ProblemProgress> }`
  - `DojoState::{load, save, state_path, status_of, progress_of, record_attempt, apply_outcome (free fn), is_due, redo_due_slugs, done_count, streak, best_pass_secs}`
  - date helpers: `today_local() -> NaiveDate`, `now_unix() -> u64`, `date_str(NaiveDate) -> String`, `parse_date(&str) -> Option<NaiveDate>`, `unix_to_local_date(u64) -> Option<NaiveDate>`.

- [ ] **Step 0:** create `src/dojo/session.rs` with:
```rust
//! Timed practice session (phases, budget). Pure: every fn takes `now`.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionKind {
    #[default]
    Dsa,
    Sd,
}
```

- [ ] **Step 1: Failing tests** (`src/dojo/state.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        parse_date(s).expect("date")
    }

    #[test]
    fn srs_table() {
        let today = d("2026-09-02");
        let mut p = ProblemProgress::default();
        apply_outcome(&mut p, Outcome::Pass, today);
        assert_eq!((p.status, p.passes, p.redo_at.as_deref()), (Status::Done, 1, None));

        let mut p = ProblemProgress::default();
        apply_outcome(&mut p, Outcome::Timeout, today);
        assert_eq!((p.status, p.redo_at.as_deref()), (Status::Redo, Some("2026-09-05")));
        apply_outcome(&mut p, Outcome::Pass, d("2026-09-05"));
        assert_eq!((p.status, p.passes, p.redo_at.as_deref()), (Status::Redo, 1, Some("2026-09-19")));
        apply_outcome(&mut p, Outcome::Giveup, d("2026-09-19"));
        assert_eq!((p.status, p.redo_at.as_deref()), (Status::Redo, Some("2026-09-22")));
        apply_outcome(&mut p, Outcome::Pass, d("2026-09-22"));
        apply_outcome(&mut p, Outcome::Pass, d("2026-10-06"));
        assert_eq!((p.status, p.passes, p.redo_at.as_deref()), (Status::Done, 3, None));

        let mut p = ProblemProgress { status: Status::Done, redo_at: None, passes: 1 };
        apply_outcome(&mut p, Outcome::Fail, today);
        assert_eq!(p.status, Status::Done, "done stays done");
    }

    #[test]
    fn record_attempt_updates_progress_and_due_list() {
        let mut s = DojoState::default();
        let today = d("2026-09-02");
        s.record_attempt(attempt("two-sum", Outcome::Timeout, 1_788_400_000), today);
        assert_eq!(s.status_of("two-sum"), Status::Redo);
        assert!(!s.is_due("two-sum", today));
        assert!(s.is_due("two-sum", d("2026-09-05")));
        assert_eq!(s.redo_due_slugs(d("2026-09-06")), vec!["two-sum".to_string()]);
        assert_eq!(s.done_count(&["two-sum", "x"]), 0);
        s.record_attempt(attempt("x", Outcome::Pass, 1_788_400_100), today);
        assert_eq!(s.done_count(&["two-sum", "x"]), 1);
        assert_eq!(s.best_pass_secs("x"), Some(600));
        assert_eq!(s.best_pass_secs("two-sum"), None);
    }

    fn attempt(slug: &str, outcome: Outcome, ended: u64) -> Attempt {
        Attempt {
            slug: slug.to_string(),
            kind: SessionKind::Dsa,
            started_unix: ended - 600,
            ended_unix: ended,
            outcome,
            elapsed_s: 600,
            approach: String::new(),
        }
    }

    #[test]
    fn streak_counts_back_from_today_or_yesterday() {
        let today = d("2026-09-10");
        let dates = [d("2026-09-07"), d("2026-09-08"), d("2026-09-09")];
        assert_eq!(streak_from_dates(&dates, today), 3, "no session yet today → from yesterday");
        let dates = [d("2026-09-08"), d("2026-09-09"), d("2026-09-10")];
        assert_eq!(streak_from_dates(&dates, today), 3);
        let dates = [d("2026-09-01"), d("2026-09-09"), d("2026-09-09")];
        assert_eq!(streak_from_dates(&dates, today), 1, "gap breaks; duplicates ignored");
        assert_eq!(streak_from_dates(&[d("2026-09-01")], today), 0);
        assert_eq!(streak_from_dates(&[], today), 0);
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = std::env::temp_dir().join(format!("dojo_state_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("dojo.toml");
        assert_eq!(DojoState::load(&path), DojoState::default(), "missing → default");
        let mut s = DojoState { last_group: Some("graph".to_string()), ..Default::default() };
        s.active_session = Some(ActiveSession {
            kind: SessionKind::Dsa,
            slug: "two-sum".to_string(),
            title: "Two Sum".to_string(),
            started_unix: 10,
            budget_s: 1500,
            approach: None,
            file: std::path::PathBuf::from("/tmp/solution.js"),
        });
        s.record_attempt(attempt("two-sum", Outcome::Fail, 1_788_400_000), d("2026-09-02"));
        s.save(&path).expect("save");
        assert_eq!(DojoState::load(&path), s);
        std::fs::write(&path, "garbage [[").expect("write");
        assert_eq!(DojoState::load(&path), DojoState::default(), "broken → default");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

- [ ] **Step 2: Run** `cargo test --lib dojo::state` → compile error.

- [ ] **Step 3: Implement `src/dojo/state.rs`**

```rust
//! Persistent Dojo state (`~/.config/netherize/dojo.toml`): attempts, per-problem
//! status + spaced-redo dates, the active session. Derived values (streak,
//! progress, due list) are computed, never stored.
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::{Days, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};

use super::session::SessionKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Pass,
    Fail,
    Timeout,
    Giveup,
}

impl Outcome {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Timeout => "timeout",
            Self::Giveup => "giveup",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    #[default]
    Todo,
    Redo,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attempt {
    pub slug: String,
    #[serde(default)]
    pub kind: SessionKind,
    pub started_unix: u64,
    pub ended_unix: u64,
    pub outcome: Outcome,
    pub elapsed_s: u64,
    #[serde(default)]
    pub approach: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ProblemProgress {
    #[serde(default)]
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redo_at: Option<String>,
    #[serde(default)]
    pub passes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveSession {
    pub kind: SessionKind,
    pub slug: String,
    #[serde(default)]
    pub title: String,
    pub started_unix: u64,
    pub budget_s: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approach: Option<String>,
    pub file: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DojoState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_session: Option<ActiveSession>,
    #[serde(default, rename = "attempt")]
    pub attempts: Vec<Attempt>,
    #[serde(default)]
    pub problem: BTreeMap<String, ProblemProgress>,
}

// ── dates ─────────────────────────────────────────────────────────────────────

pub fn today_local() -> NaiveDate {
    chrono::Local::now().date_naive()
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn date_str(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

pub fn parse_date(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d").ok()
}

pub fn unix_to_local_date(secs: u64) -> Option<NaiveDate> {
    chrono::Local
        .timestamp_opt(i64::try_from(secs).ok()?, 0)
        .single()
        .map(|dt| dt.date_naive())
}

fn plus_days(date: NaiveDate, days: u64) -> NaiveDate {
    date.checked_add_days(Days::new(days)).unwrap_or(date)
}

// ── spaced repetition (spec §8) ───────────────────────────────────────────────

pub fn apply_outcome(p: &mut ProblemProgress, outcome: Outcome, today: NaiveDate) {
    match (p.status, outcome) {
        (Status::Done, _) => {}
        (Status::Todo, Outcome::Pass) => {
            p.status = Status::Done;
            p.passes = 1;
            p.redo_at = None;
        }
        (Status::Redo, Outcome::Pass) => {
            p.passes += 1;
            if p.passes >= 2 {
                p.status = Status::Done;
                p.redo_at = None;
            } else {
                p.redo_at = Some(date_str(plus_days(today, 14)));
            }
        }
        (Status::Todo | Status::Redo, _) => {
            p.status = Status::Redo;
            p.redo_at = Some(date_str(plus_days(today, 3)));
        }
    }
}

/// Consecutive days with a session, counted back from today (if today has one)
/// or yesterday. `dates` may be unsorted and contain duplicates.
pub fn streak_from_dates(dates: &[NaiveDate], today: NaiveDate) -> u32 {
    let mut days: Vec<NaiveDate> = dates.to_vec();
    days.sort_unstable();
    days.dedup();
    let mut cursor = if days.last() == Some(&today) {
        today
    } else {
        match today.checked_sub_days(Days::new(1)) {
            Some(y) => y,
            None => return 0,
        }
    };
    let mut streak = 0;
    while days.binary_search(&cursor).is_ok() {
        streak += 1;
        match cursor.checked_sub_days(Days::new(1)) {
            Some(prev) => cursor = prev,
            None => break,
        }
    }
    streak
}

impl DojoState {
    pub fn state_path() -> PathBuf {
        crate::config::paths::user_config_root().join("dojo.toml")
    }

    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|err| {
                eprintln!("[dojo] {} unreadable, starting fresh: {err}", path.display());
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        crate::app::persistence::atomic_write(path, text).map_err(|e| e.to_string())
    }

    pub fn status_of(&self, slug: &str) -> Status {
        self.problem.get(slug).map(|p| p.status).unwrap_or_default()
    }

    pub fn progress_of(&self, slug: &str) -> ProblemProgress {
        self.problem.get(slug).cloned().unwrap_or_default()
    }

    pub fn record_attempt(&mut self, attempt: Attempt, today: NaiveDate) {
        let entry = self.problem.entry(attempt.slug.clone()).or_default();
        apply_outcome(entry, attempt.outcome, today);
        self.attempts.push(attempt);
    }

    pub fn is_due(&self, slug: &str, today: NaiveDate) -> bool {
        self.problem.get(slug).is_some_and(|p| {
            p.status == Status::Redo
                && p.redo_at.as_deref().and_then(parse_date).is_some_and(|d| d <= today)
        })
    }

    /// Slugs due for redo, in BTreeMap (alphabetical) order; callers re-sort.
    pub fn redo_due_slugs(&self, today: NaiveDate) -> Vec<String> {
        self.problem
            .keys()
            .filter(|slug| self.is_due(slug, today))
            .cloned()
            .collect()
    }

    pub fn done_count(&self, slugs: &[&str]) -> usize {
        slugs.iter().filter(|s| self.status_of(s) == Status::Done).count()
    }

    pub fn streak(&self, today: NaiveDate) -> u32 {
        let dates: Vec<NaiveDate> = self
            .attempts
            .iter()
            .filter_map(|a| unix_to_local_date(a.ended_unix))
            .collect();
        streak_from_dates(&dates, today)
    }

    pub fn best_pass_secs(&self, slug: &str) -> Option<u64> {
        self.attempts
            .iter()
            .filter(|a| a.slug == slug && a.outcome == Outcome::Pass)
            .map(|a| a.elapsed_s)
            .min()
    }
}
```

- [ ] **Step 4: Run** `cargo test --lib dojo::state` → 4 passed. If `toml::to_string_pretty` complains about ordering, keep struct field order as written (scalars → tables → arrays → map).
- [ ] **Step 5: Leave uncommitted.**

---

### Task 4: `dojo::session` — phases and expiry

**Files:**
- Modify: `src/dojo/session.rs`

**Interfaces:**
- Produces: `Phase { name: String, index: usize, remaining_s: u64 }`, `phase_at(phases: &[(String, u32)], elapsed_s: u64) -> Option<Phase>`, `total_secs(phases) -> u64`, `ActiveSession::{elapsed_s(now), remaining_s(now), is_expired(now)}` (impl in this file on `state::ActiveSession`), `single_phase(name, secs) -> Vec<(String, u32)>`.

- [ ] **Step 1: Failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::dojo::state::ActiveSession;

    fn phases() -> Vec<(String, u32)> {
        vec![("THINK".into(), 3), ("CODE".into(), 15), ("TEST".into(), 5), ("REVIEW".into(), 2)]
    }

    #[test]
    fn phase_boundaries() {
        let p = phases();
        assert_eq!(total_secs(&p), 1500);
        let at = |e| phase_at(&p, e).map(|ph| (ph.index, ph.name.clone(), ph.remaining_s));
        assert_eq!(at(0), Some((0, "THINK".into(), 180)));
        assert_eq!(at(179), Some((0, "THINK".into(), 1)));
        assert_eq!(at(180), Some((1, "CODE".into(), 900)));
        assert_eq!(at(1499), Some((3, "REVIEW".into(), 1)));
        assert_eq!(at(1500), None);
        assert_eq!(phase_at(&[], 0), None);
    }

    #[test]
    fn session_clock() {
        let s = ActiveSession {
            kind: SessionKind::Sd,
            slug: "wallet_v1".into(),
            title: String::new(),
            started_unix: 1000,
            budget_s: 60,
            approach: None,
            file: "/tmp/x.md".into(),
        };
        assert_eq!(s.elapsed_s(999), 0, "clock skew clamps to 0");
        assert_eq!(s.elapsed_s(1030), 30);
        assert_eq!(s.remaining_s(1030), 30);
        assert!(!s.is_expired(1059));
        assert!(s.is_expired(1060));
        assert_eq!(single_phase("SD", 45), vec![("SD".to_string(), 45)]);
    }
}
```

- [ ] **Step 2: Run** `cargo test --lib dojo::session` → fails.
- [ ] **Step 3: Implement** (append to `session.rs`)

```rust
use super::state::ActiveSession;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phase {
    pub name: String,
    pub index: usize,
    pub remaining_s: u64,
}

pub fn total_secs(phases: &[(String, u32)]) -> u64 {
    phases.iter().map(|(_, m)| u64::from(*m) * 60).sum()
}

/// Phase containing `elapsed_s`, or `None` once the budget is spent.
pub fn phase_at(phases: &[(String, u32)], elapsed_s: u64) -> Option<Phase> {
    let mut start = 0u64;
    for (index, (name, minutes)) in phases.iter().enumerate() {
        let end = start + u64::from(*minutes) * 60;
        if elapsed_s < end {
            return Some(Phase { name: name.clone(), index, remaining_s: end - elapsed_s });
        }
        start = end;
    }
    None
}

pub fn single_phase(name: &str, minutes: u32) -> Vec<(String, u32)> {
    vec![(name.to_string(), minutes)]
}

impl ActiveSession {
    pub fn elapsed_s(&self, now_unix: u64) -> u64 {
        now_unix.saturating_sub(self.started_unix)
    }

    pub fn remaining_s(&self, now_unix: u64) -> u64 {
        self.budget_s.saturating_sub(self.elapsed_s(now_unix))
    }

    pub fn is_expired(&self, now_unix: u64) -> bool {
        self.elapsed_s(now_unix) >= self.budget_s
    }
}
```

- [ ] **Step 4: Run** → 2 passed. **Step 5: Leave uncommitted.**

---

### Task 5: `dojo::notebook` + `dojo::files`

**Files:**
- Create: `src/dojo/notebook.rs`, `src/dojo/files.rs`, `src/dojo/interviewer_prompt.md`

**Interfaces:**
- Produces (notebook): `html_to_text(&str) -> String`, `mm_ss(u64) -> String`, `NOTEBOOK_HEADER`, `format_block(date, id, title, outcome, elapsed_s, redo, approach, answers: &[(&str, &str)]) -> String`, `format_sd_block(date, label, elapsed_s, note) -> String`.
- Produces (files): `expand_tilde(&str) -> PathBuf`, `dojo_dir() -> PathBuf`, `current_md_path()`, `interviewer_md_path()`, `INTERVIEWER_PROMPT: &str`, `current_md(kind, id, title, statement, language, phases: &[(String,u32)], approach: Option<&str>) -> String`, `sd_template(label, date) -> String`.

- [ ] **Step 1: Failing tests (notebook)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::dojo::state::Outcome;

    #[test]
    fn strips_tags_and_decodes_entities() {
        let html = "<p>Given <code>nums</code> &amp; <strong>target</strong>.</p><pre>Input: nums = [2,7]\nOutput: [0,1]</pre><ul><li>1 &lt;= n</li></ul>";
        let text = html_to_text(html);
        assert_eq!(
            text,
            "Given nums & target.\nInput: nums = [2,7]\nOutput: [0,1]\n1 <= n"
        );
        assert_eq!(html_to_text("a<br/>b<br>c"), "a\nb\nc");
        assert_eq!(html_to_text("<p>x</p>\n\n\n\n<p>y</p>"), "x\n\ny");
    }

    #[test]
    fn formats_time_and_blocks() {
        assert_eq!(mm_ss(0), "00:00");
        assert_eq!(mm_ss(754), "12:34");
        let block = format_block(
            "2026-09-02", 3, "Longest Substring", Outcome::Timeout, 1500, true,
            "sliding window + set, O(n)",
            &[("Bí", "quên co cửa sổ"), ("Pattern", "window co giãn"), ("Dấu hiệu", "")],
        );
        assert_eq!(
            block,
            "## 2026-09-02 · #3 Longest Substring · timeout 25:00 · #redo\n- Hướng: sliding window + set, O(n)\n- Bí: quên co cửa sổ\n- Pattern: window co giãn\n\n"
        );
        let pass = format_block("2026-09-02", 1, "Two Sum", Outcome::Pass, 760, false, "", &[("Ghi chú", "")]);
        assert_eq!(pass, "## 2026-09-02 · #1 Two Sum · pass 12:40\n\n");
        assert_eq!(
            format_sd_block("2026-09-02", "Rút gọn URL", 2700, "ok"),
            "## 2026-09-02 · SD · Rút gọn URL · 45:00\n- Ghi chú: ok\n\n"
        );
    }
}
```

- [ ] **Step 2: Implement `src/dojo/notebook.rs`**

```rust
//! Error-notebook formatting (append-only markdown the user reads on Sundays).
use super::state::Outcome;

pub const NOTEBOOK_HEADER: &str = "# Sổ tay lỗi\n\n";

pub fn mm_ss(secs: u64) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

/// LeetCode statement HTML → plain text. Block tags become newlines, inline
/// tags vanish, common entities decode, 3+ blank lines collapse to one.
pub fn html_to_text(html: &str) -> String {
    let block = regex::Regex::new(r"(?i)<br\s*/?>|</p>|</li>|</pre>|</div>|</h[1-6]>|</tr>");
    let tags = regex::Regex::new(r"(?s)<[^>]+>");
    let many_newlines = regex::Regex::new(r"\n{3,}");
    let (Ok(block), Ok(tags), Ok(many_newlines)) = (block, tags, many_newlines) else {
        return html.to_string();
    };
    let text = block.replace_all(html, "\n");
    let text = tags.replace_all(&text, "");
    let text = text
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    let text = many_newlines.replace_all(&text, "\n\n");
    text.lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

pub fn format_block(
    date: &str,
    id: u32,
    title: &str,
    outcome: Outcome,
    elapsed_s: u64,
    redo: bool,
    approach: &str,
    answers: &[(&str, &str)],
) -> String {
    let mut out = format!("## {date} · #{id} {title} · {} {}", outcome.label(), mm_ss(elapsed_s));
    if redo {
        out.push_str(" · #redo");
    }
    out.push('\n');
    if !approach.trim().is_empty() {
        out.push_str(&format!("- Hướng: {}\n", approach.trim()));
    }
    for (label, answer) in answers {
        if !answer.trim().is_empty() {
            out.push_str(&format!("- {label}: {}\n", answer.trim()));
        }
    }
    out.push('\n');
    out
}

pub fn format_sd_block(date: &str, label: &str, elapsed_s: u64, note: &str) -> String {
    let mut out = format!("## {date} · SD · {label} · {}\n", mm_ss(elapsed_s));
    if !note.trim().is_empty() {
        out.push_str(&format!("- Ghi chú: {}\n", note.trim()));
    }
    out.push('\n');
    out
}
```

- [ ] **Step 3: Failing tests (files)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::dojo::session::SessionKind;

    #[test]
    fn tilde_expands_to_home() {
        let home = std::env::var("HOME").expect("HOME");
        assert_eq!(expand_tilde("~/Work/x.md"), std::path::PathBuf::from(format!("{home}/Work/x.md")));
        assert_eq!(expand_tilde("/abs/x"), std::path::PathBuf::from("/abs/x"));
        assert!(dojo_dir().ends_with("dojo"));
        assert!(current_md_path().ends_with("dojo/current.md"));
    }

    #[test]
    fn current_md_and_templates_carry_the_essentials() {
        let phases = vec![("THINK".to_string(), 3), ("CODE".to_string(), 15)];
        let md = current_md(SessionKind::Dsa, 1, "Two Sum", "Given nums…", "javascript", &phases, Some("hash map"));
        assert!(md.starts_with("# Dojo session\n"));
        assert!(md.contains("kind: dsa"));
        assert!(md.contains("#1 Two Sum"));
        assert!(md.contains("THINK 3'"));
        assert!(md.contains("Approach: hash map"));
        let sd = current_md(SessionKind::Sd, 0, "Rút gọn URL", "", "", &[], None);
        assert!(sd.contains("kind: sd"));
        assert!(!sd.contains("Approach:"));
        assert!(INTERVIEWER_PROMPT.contains("current.md"));
        let t = sd_template("Rút gọn URL", "2026-09-02");
        assert!(t.starts_with("# Rút gọn URL — 2026-09-02\n"));
        assert!(t.contains("## 6. Nút cổ chai + đánh đổi (5')"));
    }
}
```

- [ ] **Step 4: Implement `src/dojo/files.rs`**

```rust
//! Paths + text templates for the Dojo's side files (current.md for the AI
//! interviewer, the interviewer prompt, the SD outline template).
use std::path::PathBuf;

use super::session::SessionKind;

pub const INTERVIEWER_PROMPT: &str = include_str!("interviewer_prompt.md");

pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        return home.join(rest);
    }
    PathBuf::from(path)
}

pub fn dojo_dir() -> PathBuf {
    crate::config::paths::user_config_root().join("dojo")
}

pub fn current_md_path() -> PathBuf {
    dojo_dir().join("current.md")
}

pub fn interviewer_md_path() -> PathBuf {
    dojo_dir().join("interviewer.md")
}

pub fn current_md(
    kind: SessionKind,
    id: u32,
    title: &str,
    statement: &str,
    language: &str,
    phases: &[(String, u32)],
    approach: Option<&str>,
) -> String {
    let kind_label = match kind {
        SessionKind::Dsa => "dsa",
        SessionKind::Sd => "sd",
    };
    let mut out = format!("# Dojo session\nkind: {kind_label}\n");
    if id > 0 {
        out.push_str(&format!("problem: #{id} {title}\n"));
    } else {
        out.push_str(&format!("case: {title}\n"));
    }
    if !language.is_empty() {
        out.push_str(&format!("language: {language}\n"));
    }
    if !phases.is_empty() {
        let budget: Vec<String> = phases.iter().map(|(n, m)| format!("{n} {m}'")).collect();
        out.push_str(&format!("phases: {}\n", budget.join(" → ")));
    }
    if let Some(a) = approach.filter(|a| !a.trim().is_empty()) {
        out.push_str(&format!("Approach: {}\n", a.trim()));
    }
    if !statement.trim().is_empty() {
        out.push_str("\n## Statement\n");
        out.push_str(statement.trim());
        out.push('\n');
    }
    out
}

pub fn sd_template(label: &str, date: &str) -> String {
    format!(
        "# {label} — {date}\n\n## 1. Làm rõ yêu cầu (5')\n\n## 2. Ước lượng quy mô (5')\n\n## 3. API + mô hình dữ liệu (5')\n\n## 4. Kiến trúc mức cao (10')\n\n## 5. Đào sâu 1–2 điểm (15')\n\n## 6. Nút cổ chai + đánh đổi (5')\n\n> Câu hỏi bắt buộc: request này chết giữa chừng thì sao?\n"
    )
}
```

`src/dojo/interviewer_prompt.md`:
```markdown
You are a senior backend interviewer running a mock interview inside the Netherize editor.

First read `~/.config/netherize/dojo/current.md` (the candidate's current problem or system-design case, timer phases, and their stated approach). If it is missing, ask which problem they are working on.

Rules:
- For `kind: dsa`: before any code is discussed, make the candidate state (1) the approach in plain words, (2) time and space complexity, (3) one edge case. Push back on vague answers. Never write or paste a solution. Give a hint only when the candidate explicitly asks, one hint at a time, smallest hint first.
- For `kind: sd`: run the 45-minute framework — requirements (5'), scale estimate (5'), API + data model (5'), high-level design (10'), deep dive (15'), bottlenecks + trade-offs (5'). Keep asking "what happens when this request dies halfway?". Prefer money-safety topics: idempotency keys, reserve/commit/release, outbox + retry, reconciliation.
- Talk like an interviewer: short questions, no lectures, no praise padding.
- When the candidate says "done" or "xong", grade out of 5 each: correctness, complexity/scale reasoning, communication. Name the pattern the problem was testing and the single most important thing to fix. Vietnamese is fine if the candidate writes Vietnamese.
```

- [ ] **Step 5: Run** `cargo test --lib dojo::notebook dojo::files` → 4 passed. **Step 6: Leave uncommitted.**

---

### Task 6: `dojo::view` — row model, header, suggestion, wrapping (pure)

**Files:**
- Create: `src/dojo/view.rs`

**Interfaces:**
- Consumes: `Plan`, `Page`, `Problems`, `DojoState`, `Status`, `SessionKind`, `mm_ss`.
- Produces:
```rust
pub enum RowGlyph { RedoDue, RedoLater, Todo, Done }
pub struct DojoRow { pub slug: String, pub id: u32, pub title: String, pub glyph: RowGlyph, pub trailing: String, pub kind: SessionKind }
pub struct DojoHeader { pub page_label: String, pub page_index: usize, pub page_count: usize, pub page_done: usize, pub page_total: usize, pub overall_done: usize, pub overall_total: usize, pub streak: u32, pub redo_due: usize, pub note: String }
pub fn list_rows(plan, problems, state, page, redo_only, today) -> Vec<DojoRow>
pub fn header(plan, problems, state, page, today) -> DojoHeader
pub fn suggested_next(plan, problems, state, page, today) -> Option<DojoRow>
pub fn initial_page(plan, problems, state) -> Page   // last_group or first group with a todo
pub fn wrap_text(text: &str, max_chars: usize) -> Vec<String>
```

- [ ] **Step 1: Failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::dojo::{plan::Plan, problems::Problems, state::{Attempt, DojoState, Outcome, parse_date}};

    fn fixture() -> (Plan, Problems, DojoState, chrono::NaiveDate) {
        (Plan::bundled(), Problems::bundled(), DojoState::default(), parse_date("2026-09-10").expect("d"))
    }

    fn attempt(slug: &str, outcome: Outcome, elapsed: u64) -> Attempt {
        Attempt { slug: slug.into(), kind: SessionKind::Dsa, started_unix: 1_788_000_000, ended_unix: 1_788_000_000 + elapsed, outcome, elapsed_s: elapsed, approach: String::new() }
    }

    #[test]
    fn rows_put_due_redos_first_then_todo_then_later_then_done() {
        let (plan, problems, mut state, today) = fixture();
        // "valid-anagram" (group 0) fails on 2026-09-01 → due since 09-04.
        state.record_attempt(attempt("valid-anagram", Outcome::Timeout, 1500), parse_date("2026-09-01").expect("d"));
        // "min-stack" (group 1) fails today → redo later (09-13).
        state.record_attempt(attempt("min-stack", Outcome::Giveup, 100), today);
        // "two-sum" passes.
        state.record_attempt(attempt("two-sum", Outcome::Pass, 760), today);
        let rows = list_rows(&plan, &problems, &state, Page::Group(1), false, today);
        assert_eq!(rows[0].slug, "valid-anagram", "due redo from ANOTHER group leads");
        assert!(matches!(rows[0].glyph, RowGlyph::RedoDue));
        assert_eq!(rows[0].trailing, "redo hôm nay");
        let later = rows.iter().position(|r| r.slug == "min-stack").expect("min-stack");
        let last_todo = rows.iter().rposition(|r| matches!(r.glyph, RowGlyph::Todo)).expect("todo");
        assert!(later > last_todo, "redo-later sorts after todo");
        assert_eq!(rows[later].trailing, "redo 13/09");
        assert!(!rows.iter().any(|r| r.slug == "two-sum"), "other group's done row not on this page");
        let rows0 = list_rows(&plan, &problems, &state, Page::Group(0), false, today);
        let done = rows0.last().expect("rows");
        assert_eq!((done.slug.as_str(), done.trailing.as_str()), ("two-sum", "pass 12:40"));
        let only = list_rows(&plan, &problems, &state, Page::Group(0), true, today);
        assert_eq!(only.len(), 1);
    }

    #[test]
    fn sd_page_rows_and_header() {
        let (plan, problems, mut state, today) = fixture();
        state.attempts.push(Attempt { slug: "url_shortener".into(), kind: SessionKind::Sd, started_unix: 1, ended_unix: 2701, outcome: Outcome::Pass, elapsed_s: 2700, approach: String::new() });
        let rows = list_rows(&plan, &problems, &state, Page::Sd, false, today);
        assert_eq!(rows.len(), 8);
        assert!(rows.iter().all(|r| r.kind == SessionKind::Sd && r.id == 0));
        let done = rows.iter().find(|r| r.slug == "url_shortener").expect("row");
        assert!(matches!(done.glyph, RowGlyph::Done));
        let h = header(&plan, &problems, &state, Page::Sd, today);
        assert_eq!((h.page_label.as_str(), h.page_index, h.page_count, h.page_total), ("System Design", 8, 8, 8));
        assert_eq!(h.overall_total, 150);
    }

    #[test]
    fn header_and_suggestion() {
        let (plan, problems, mut state, today) = fixture();
        let h = header(&plan, &problems, &state, Page::Group(0), today);
        assert_eq!((h.page_index, h.page_count, h.page_done, h.page_total, h.overall_done, h.overall_total, h.streak, h.redo_due), (1, 8, 0, 14, 0, 150, 0, 0));
        assert_eq!(suggested_next(&plan, &problems, &state, Page::Group(0), today).map(|r| r.slug), Some("contains-duplicate".into()));
        assert_eq!(initial_page(&plan, &problems, &state), Page::Group(0));
        state.last_group = Some("graph".into());
        assert_eq!(initial_page(&plan, &problems, &state), Page::Group(5));
        state.record_attempt(attempt("min-stack", Outcome::Timeout, 1500), parse_date("2026-09-01").expect("d"));
        assert_eq!(suggested_next(&plan, &problems, &state, Page::Group(0), today).map(|r| r.slug), Some("min-stack".into()), "due redo wins");
        assert_eq!(header(&plan, &problems, &state, Page::Group(0), today).redo_due, 1);
    }

    #[test]
    fn wraps_words_and_hard_breaks_long_tokens() {
        assert_eq!(wrap_text("aaa bbb ccc", 7), vec!["aaa bbb", "ccc"]);
        assert_eq!(wrap_text("x\n\ny", 10), vec!["x", "", "y"]);
        assert_eq!(wrap_text("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
        assert_eq!(wrap_text("", 4), Vec::<String>::new());
    }
}
```

- [ ] **Step 2: Implement `src/dojo/view.rs`**

```rust
//! Row/header model for the Dojo panel. Pure so ordering rules are testable
//! without the renderer.
use chrono::NaiveDate;

use super::{
    notebook::mm_ss,
    plan::{Page, Plan},
    problems::Problems,
    session::SessionKind,
    state::{DojoState, Status, parse_date},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowGlyph {
    RedoDue,
    RedoLater,
    Todo,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DojoRow {
    pub slug: String,
    pub id: u32,
    pub title: String,
    pub glyph: RowGlyph,
    pub trailing: String,
    pub kind: SessionKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DojoHeader {
    pub page_label: String,
    pub page_index: usize,
    pub page_count: usize,
    pub page_done: usize,
    pub page_total: usize,
    pub overall_done: usize,
    pub overall_total: usize,
    pub streak: u32,
    pub redo_due: usize,
    pub note: String,
}

fn dsa_row(problems: &Problems, state: &DojoState, slug: &str, today: NaiveDate) -> Option<DojoRow> {
    let p = problems.by_slug(slug)?;
    let progress = state.progress_of(slug);
    let (glyph, trailing) = match progress.status {
        Status::Done => (
            RowGlyph::Done,
            state.best_pass_secs(slug).map(|s| format!("pass {}", mm_ss(s))).unwrap_or_default(),
        ),
        Status::Redo if state.is_due(slug, today) => (RowGlyph::RedoDue, "redo hôm nay".to_string()),
        Status::Redo => (
            RowGlyph::RedoLater,
            progress
                .redo_at
                .as_deref()
                .and_then(parse_date)
                .map(|d| format!("redo {}", d.format("%d/%m")))
                .unwrap_or_default(),
        ),
        Status::Todo => (RowGlyph::Todo, String::new()),
    };
    Some(DojoRow { slug: p.slug.clone(), id: p.id, title: p.title.clone(), glyph, trailing, kind: SessionKind::Dsa })
}

fn due_rows(problems: &Problems, state: &DojoState, today: NaiveDate) -> Vec<DojoRow> {
    // Problems-file order, not BTreeMap order.
    problems
        .problems
        .iter()
        .filter(|p| state.is_due(&p.slug, today))
        .filter_map(|p| dsa_row(problems, state, &p.slug, today))
        .collect()
}

pub fn list_rows(
    plan: &Plan,
    problems: &Problems,
    state: &DojoState,
    page: Page,
    redo_only: bool,
    today: NaiveDate,
) -> Vec<DojoRow> {
    let mut rows = due_rows(problems, state, today);
    if redo_only {
        return rows;
    }
    match page {
        Page::Sd => {
            for case in &plan.sd_cases {
                let done = state.attempts.iter().any(|a| a.kind == SessionKind::Sd && a.slug == case.key);
                rows.push(DojoRow {
                    slug: case.key.clone(),
                    id: 0,
                    title: case.label.clone(),
                    glyph: if done { RowGlyph::Done } else { RowGlyph::Todo },
                    trailing: case.topic.clone(),
                    kind: SessionKind::Sd,
                });
            }
        }
        Page::Group(idx) => {
            let mut todo = Vec::new();
            let mut later = Vec::new();
            let mut done = Vec::new();
            for p in plan.group_problems(idx, problems) {
                if state.is_due(&p.slug, today) {
                    continue; // already in the due block
                }
                if let Some(row) = dsa_row(problems, state, &p.slug, today) {
                    match row.glyph {
                        RowGlyph::Todo => todo.push(row),
                        RowGlyph::RedoLater => later.push(row),
                        _ => done.push(row),
                    }
                }
            }
            rows.extend(todo);
            rows.extend(later);
            rows.extend(done);
        }
    }
    rows
}

pub fn header(plan: &Plan, problems: &Problems, state: &DojoState, page: Page, today: NaiveDate) -> DojoHeader {
    let pages = plan.pages();
    let page_index = pages.iter().position(|p| *p == page).map(|i| i + 1).unwrap_or(0);
    let all: Vec<&str> = problems.problems.iter().map(|p| p.slug.as_str()).collect();
    let (page_done, page_total, note) = match page {
        Page::Sd => {
            let done = plan
                .sd_cases
                .iter()
                .filter(|c| state.attempts.iter().any(|a| a.kind == SessionKind::Sd && a.slug == c.key))
                .count();
            (done, plan.sd_cases.len(), String::new())
        }
        Page::Group(idx) => {
            let slugs: Vec<&str> = plan.group_problems(idx, problems).iter().map(|p| p.slug.as_str()).collect();
            (state.done_count(&slugs), slugs.len(), plan.groups.get(idx).map(|g| g.note.clone()).unwrap_or_default())
        }
    };
    DojoHeader {
        page_label: plan.page_label(page),
        page_index,
        page_count: pages.len(),
        page_done,
        page_total,
        overall_done: state.done_count(&all),
        overall_total: all.len(),
        streak: state.streak(today),
        redo_due: due_rows(problems, state, today).len(),
        note,
    }
}

/// First due redo, else first todo on `page`, else first todo anywhere.
pub fn suggested_next(plan: &Plan, problems: &Problems, state: &DojoState, page: Page, today: NaiveDate) -> Option<DojoRow> {
    if let Some(row) = due_rows(problems, state, today).into_iter().next() {
        return Some(row);
    }
    let on_page = list_rows(plan, problems, state, page, false, today)
        .into_iter()
        .find(|r| r.glyph == RowGlyph::Todo);
    if on_page.is_some() {
        return on_page;
    }
    problems
        .problems
        .iter()
        .find(|p| state.status_of(&p.slug) == Status::Todo)
        .and_then(|p| dsa_row(problems, state, &p.slug, today))
}

/// `last_group` if it still exists, else the first group with a todo, else the first page.
pub fn initial_page(plan: &Plan, problems: &Problems, state: &DojoState) -> Page {
    if let Some(page) = state.last_group.as_deref().and_then(|k| plan.page_by_key(k)) {
        return page;
    }
    (0..plan.groups.len())
        .map(Page::Group)
        .find(|page| match page {
            Page::Group(idx) => plan.group_problems(*idx, problems).iter().any(|p| state.status_of(&p.slug) == Status::Todo),
            Page::Sd => false,
        })
        .unwrap_or(Page::Group(0))
}

/// Greedy word wrap; blank lines preserved; over-long tokens hard-broken.
pub fn wrap_text(text: &str, max_chars: usize) -> Vec<String> {
    let max = max_chars.max(1);
    let mut out = Vec::new();
    if text.trim().is_empty() {
        return out;
    }
    for line in text.lines() {
        if line.trim().is_empty() {
            out.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in line.split_whitespace() {
            let mut word: Vec<char> = word.chars().collect();
            while word.len() > max {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
                out.push(word.drain(..max).collect());
            }
            let word: String = word.into_iter().collect();
            let needed = if current.is_empty() { word.chars().count() } else { current.chars().count() + 1 + word.chars().count() };
            if needed > max && !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(&word);
        }
        if !current.is_empty() {
            out.push(current);
        }
    }
    out
}
```

- [ ] **Step 3: Run** `cargo test --lib dojo::view` → 4 passed. Adjust the `min-stack` expectation date if `is_due` math differs (today 09-10 + 3 = 09-13). **Step 4: Leave uncommitted.**

---

### Task 7: Editor wiring — tab, focus context, commands, keymap, runtime, basic handlers

**Files:**
- Modify: `src/workbench/panel_state.rs` (enum + `label` + `icon_glyph` + default right tabs), `src/app/input_map/mod.rs` (`InputFocusContext::Dojo` + `as_str` + `allows_leader`), `src/core/commands.rs`, `src/core/command_ids.rs`, `src/core/command_dispatch/mod.rs`, `config/keymaps/default.toml`, `src/app/command_palette.rs` (catalog entry), `src/app/input/handler.rs`, `src/app/event_loop/setup.rs`, `src/app/event_loop/mod.rs`, `src/app/event_loop/commands.rs`
- Create: `src/app/event_loop/commands_dojo.rs`
- Test: `src/app/input/tests.rs`, `src/core/command_ids.rs` tests, `src/app/event_loop/commands_tests.rs`

**Interfaces:**
- Produces: `PanelTabId::Dojo`, `InputFocusContext::Dojo`, `Command::{DojoOpen, DojoSelectNext, DojoSelectPrev, DojoStart, DojoToggleRedo, DojoPageNext, DojoPagePrev, DojoInterviewer, DojoGiveUp, DojoScrollDown, DojoScrollUp, DojoUnfocus}`, id `dojo.open` (`DOJO_OPEN`), `AppShell.dojo: DojoRuntime`, `AppShell::handle_dojo_command(&Command) -> Option<bool>`.

```rust
// commands_dojo.rs — struct visible to mod.rs/setup.rs
pub(super) struct DojoRuntime {
    pub plan: crate::dojo::plan::Plan,
    pub problems: crate::dojo::problems::Problems,
    pub state: crate::dojo::state::DojoState,
    pub page: crate::dojo::plan::Page,
    pub selected: usize,
    pub redo_only: bool,
    pub scroll: usize,
    /// Fetch submitted by the Dojo; matched against the fetch result's slug.
    pub pending_start: Option<String>,
    /// Notebook block being collected through the DojoNote prompts.
    pub pending_note: Option<PendingNote>,
    pub last_phase: Option<usize>,
    pub last_tick_second: u64,
    /// Set when a session is started or resumed via `g o`; gates the tick.
    pub armed: bool,
}
pub(super) struct PendingNote { pub date: String, pub id: u32, pub title: String, pub outcome: Outcome, pub elapsed_s: u64, pub redo: bool, pub approach: String, pub kind: SessionKind, pub answers: Vec<(&'static str, String)> }
```

- [ ] **Step 1: Failing tests**

`src/core/command_ids.rs` tests (next to the `NEW_LEETCODE_FILE` assertions, ~line 938):
```rust
assert_eq!(DOJO_OPEN, "dojo.open");
assert_eq!(parse(DOJO_OPEN, None), Some(Command::DojoOpen));
assert!(ALL_IDS.contains(&DOJO_OPEN));
```

`src/app/input/tests.rs` (after `canvas_open_is_ge_and_gc_is_free_for_comment`):
```rust
#[test]
fn dojo_open_is_g_o() {
    let map = make_default_profile_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let mut handler = InputHandler::new();
    let now = Instant::now();
    let _ = handler.route_normalized_input(char_input('g', KeyCode::KeyG), &map, context, now);
    match handler.route_normalized_input(char_input('o', KeyCode::KeyO), &map, context, now + Duration::from_millis(1)) {
        Some(InputRouteOutcome::Dispatch(t)) => assert_eq!(t.command, Command::DojoOpen),
        other => panic!("expected g o -> DojoOpen, got {other:?}"),
    }
}

#[test]
fn dojo_panel_keys_route_to_dojo_commands() {
    let map = make_default_profile_map();
    let context = KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::Dojo);
    let mut handler = InputHandler::new();
    let now = Instant::now();
    let expect = |handler: &mut InputHandler, input, want: Command| match handler.route_normalized_input(input, &map, context, now) {
        Some(InputRouteOutcome::Dispatch(t)) => assert_eq!(t.command, want),
        other => panic!("expected {want:?}, got {other:?}"),
    };
    expect(&mut handler, char_input('j', KeyCode::KeyJ), Command::DojoSelectNext);
    expect(&mut handler, char_input('k', KeyCode::KeyK), Command::DojoSelectPrev);
    expect(&mut handler, char_input('r', KeyCode::KeyR), Command::DojoToggleRedo);
    expect(&mut handler, char_input(']', KeyCode::BracketRight), Command::DojoPageNext);
    expect(&mut handler, char_input('[', KeyCode::BracketLeft), Command::DojoPagePrev);
    expect(&mut handler, char_input('i', KeyCode::KeyI), Command::DojoInterviewer);
    expect(&mut handler, char_input('x', KeyCode::KeyX), Command::DojoGiveUp);
    expect(&mut handler, named_input(NamedKey::Enter, None), Command::DojoStart);
    expect(&mut handler, named_input(NamedKey::Escape, None), Command::DojoUnfocus);
}
```
(`char_input`, `named_input`, `make_default_profile_map` already exist in that test module — verify the exact helper names at the top of `src/app/input/tests.rs` and adjust.)

`src/app/event_loop/commands_tests.rs`:
```rust
#[test]
fn dojo_open_shows_the_dojo_tab_and_focuses_the_right_dock() {
    let mut shell = AppShell::new_for_tests().expect("shell");
    assert!(shell.handle_command(Command::DojoOpen));
    assert!(shell.panel_state.right.visible);
    assert_eq!(shell.panel_state.right.active_tab_id(), Some(PanelTabId::Dojo));
    assert_eq!(shell.focus_manager.current(), FocusTarget::RightSidebar);
    assert_eq!(shell.build_context().focus, InputFocusContext::Dojo);
    // Selection + paging are clamped and persisted to last_group.
    let _ = shell.handle_command(Command::DojoSelectNext);
    assert_eq!(shell.dojo.selected, 1);
    let _ = shell.handle_command(Command::DojoPageNext);
    assert_eq!(shell.dojo.page, crate::dojo::plan::Page::Group(1));
    assert_eq!(shell.dojo.state.last_group.as_deref(), Some("sliding_stack"));
    assert_eq!(shell.dojo.selected, 0, "page change resets selection");
    let _ = shell.handle_command(Command::DojoUnfocus);
    assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);
}
```
Note: `new_for_tests` must not write the real `~/.config/netherize/dojo.toml`; in tests `DojoRuntime::for_tests()` uses bundled plan/problems + default state and a `save_path: Option<PathBuf>` of `None` (saves are skipped when `None`). Add `save_path` to `DojoRuntime`.

- [ ] **Step 2: Run** → compile errors. Then implement, in this order:

**2a. `panel_state.rs`:** add `Dojo,` to `PanelTabId` (after `MarkdownPreview`), `Self::Dojo => "Dojo"` in `label`, `Self::Dojo => Some("built_in:flask")` in `icon_glyph` (reuse the flask icon; a dedicated one is a follow-up), and append `PanelTabId::Dojo` to the right-dock `tabs` vec in `WorkbenchPanelState::default()` (`vec![PanelTabId::AiChat, PanelTabId::TestRunner, PanelTabId::Dojo]`). Grep `PanelTabId::TestRunner` across `src/` for any other right-tab vec (setup.rs / commands_tests.rs fixtures) and leave fixtures alone unless a test asserts the count.

**2b. `input_map/mod.rs`:** add `/// Right sidebar Dojo tab — j/k/Enter/[ ]/r/i/x, Esc.` `Dojo,` to `InputFocusContext`, `Self::Dojo => "dojo"` in `as_str`, and `| Self::Dojo` in `allows_leader`.

**2c. `commands.rs`:** after `TestRunnerUnfocus,` add:
```rust
    // ── Dojo (interview-prep panel) ──────────────────────────────────────
    DojoOpen,
    DojoSelectNext,
    DojoSelectPrev,
    DojoStart,
    DojoToggleRedo,
    DojoPageNext,
    DojoPagePrev,
    DojoInterviewer,
    DojoGiveUp,
    DojoScrollDown,
    DojoScrollUp,
    DojoUnfocus,
```

**2d. `command_ids.rs`:** `pub const DOJO_OPEN: &str = "dojo.open";` next to `FETCH_LEETCODE_PROBLEM`; add `DOJO_OPEN,` to `ALL_IDS`; add `DOJO_OPEN => Some(Command::DojoOpen),` in `parse` next to `CANVAS_OPEN`.

**2e. `command_dispatch/mod.rs`:** add all twelve `| Command::Dojo*` variants to the shell-handled passthrough block right after `| Command::TestRunnerToggleField`.

**2f. `config/keymaps/default.toml`** after the `g e` block:
```toml
[[bindings]]
mode = "normal"
key = "g o"
command = "dojo.open"
```

**2g. `command_palette.rs`** catalog (next to `("runner.fetch_leetcode_problem", ...)`): `("dojo.open", "Dojo: Open"),`.

**2h. `handler.rs`:** add after `route_outline_input`:
```rust
    /// Dojo panel: list navigation + session keys. Text input never reaches
    /// the editor from here (Esc/q leave).
    fn route_dojo_input(
        &mut self,
        normalized: NormalizedInput,
        input_debug: String,
        context: KeybindingContext,
    ) -> Option<InputRouteOutcome> {
        let focus = context.focus.as_str();
        let make = |command: Command, reason: &str| {
            Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                input_debug.clone(),
                format!("focus={focus} -> dojo: {reason}"),
                command,
                1,
                false,
            )))
        };
        if normalized.named_key == Some(NamedKey::Escape) {
            return make(Command::DojoUnfocus, "leave (Esc)");
        }
        if normalized.named_key == Some(NamedKey::Enter) && !normalized.has_command_modifier() {
            return make(Command::DojoStart, "start (Enter)");
        }
        if normalized.named_key == Some(NamedKey::ArrowDown) {
            return make(Command::DojoSelectNext, "next (Down)");
        }
        if normalized.named_key == Some(NamedKey::ArrowUp) {
            return make(Command::DojoSelectPrev, "prev (Up)");
        }
        if normalized.modifiers.control_key() && !normalized.has_command_modifier() {
            match normalized.text.as_deref() {
                Some("d") => return make(Command::DojoScrollDown, "scroll (C-d)"),
                Some("u") => return make(Command::DojoScrollUp, "scroll (C-u)"),
                _ => {}
            }
        }
        if let Some(text) = normalized.text.as_deref()
            && !normalized.has_command_modifier()
        {
            match text {
                "j" => return make(Command::DojoSelectNext, "next (j)"),
                "k" => return make(Command::DojoSelectPrev, "prev (k)"),
                "r" => return make(Command::DojoToggleRedo, "redo filter (r)"),
                "]" => return make(Command::DojoPageNext, "next page (])"),
                "[" => return make(Command::DojoPagePrev, "prev page ([)"),
                "i" => return make(Command::DojoInterviewer, "interviewer (i)"),
                "x" => return make(Command::DojoGiveUp, "give up (x)"),
                "q" => return make(Command::DojoUnfocus, "leave (q)"),
                _ => {}
            }
        }
        None
    }
```
Hook it in BOTH paths, right after the Outline hooks (handler.rs ~269 and ~433):
```rust
        if context.focus == InputFocusContext::Dojo && !zen_mode_allows_leader && !dojo_allows_leader {
            return self.route_dojo_input(normalized, input_debug, context);
        }
```
In the press path define `let dojo_allows_leader = context.focus == InputFocusContext::Dojo && leader_sequence_input;` next to `outline_allows_leader` (so `Space` chords still work); in the repeat path use the simple `if context.focus == InputFocusContext::Dojo { return self.route_dojo_input(...) }`.

**2i. `setup.rs` `build_context`:** in the `FocusTarget::RightSidebar` match add `Some(PanelTabId::Dojo) => InputFocusContext::Dojo,` after the Outline arm.

**2j. `mod.rs` AppShell:** add field `pub(super) dojo: commands_dojo::DojoRuntime,` next to `outline_selected` and `mod commands_dojo;` with the other `mod commands_*` declarations. In `setup.rs` `new()` init: `dojo: commands_dojo::DojoRuntime::load(),` and in `new_for_tests`: `dojo: commands_dojo::DojoRuntime::for_tests(),` (grep for `outline_selected: None,` — there may be two init sites).

**2k. `commands.rs` router:** after the `handle_ai_agent_command` block add the same shape for `handle_dojo_command`.

**2l. Create `commands_dojo.rs`:**
```rust
//! Dojo panel handlers: open/navigate the problem menu. Session start/end,
//! timer and notebook live in the same file (later tasks append to it).
use std::path::PathBuf;

use crate::{
    core::commands::Command,
    dojo::{
        plan::{Page, Plan},
        problems::Problems,
        session::SessionKind,
        state::{DojoState, Outcome, today_local},
        view,
    },
    workbench::{focus_manager::FocusTarget, panel_state::PanelTabId},
};

use super::AppShell;

pub(super) struct PendingNote {
    pub date: String,
    pub id: u32,
    pub title: String,
    pub outcome: Outcome,
    pub elapsed_s: u64,
    pub redo: bool,
    pub approach: String,
    pub kind: SessionKind,
    pub answers: Vec<(&'static str, String)>,
}

pub(super) struct DojoRuntime {
    pub plan: Plan,
    pub problems: Problems,
    pub state: DojoState,
    pub save_path: Option<PathBuf>,
    pub page: Page,
    pub selected: usize,
    pub redo_only: bool,
    pub scroll: usize,
    pub pending_start: Option<String>,
    pub pending_note: Option<PendingNote>,
    pub last_phase: Option<usize>,
    pub last_tick_second: u64,
    pub armed: bool,
}

impl DojoRuntime {
    fn with(plan: Plan, problems: Problems, state: DojoState, save_path: Option<PathBuf>) -> Self {
        let page = view::initial_page(&plan, &problems, &state);
        Self {
            plan,
            problems,
            state,
            save_path,
            page,
            selected: 0,
            redo_only: false,
            scroll: 0,
            pending_start: None,
            pending_note: None,
            last_phase: None,
            last_tick_second: 0,
            armed: false,
        }
    }

    /// Startup load. Small TOML files; sync like `AppPersistentState::load`.
    pub fn load() -> Self {
        let dir = crate::dojo::files::dojo_dir();
        let plan = Plan::load(&dir.join("plan.toml"));
        let problems = Problems::load(&dir.join("neetcode150.toml"));
        let path = DojoState::state_path();
        let state = DojoState::load(&path);
        Self::with(plan, problems, state, Some(path))
    }

    #[cfg(test)]
    pub fn for_tests() -> Self {
        Self::with(Plan::bundled(), Problems::bundled(), DojoState::default(), None)
    }

    pub fn save(&self) {
        if let Some(path) = &self.save_path
            && let Err(err) = self.state.save(path)
        {
            eprintln!("[dojo] save failed: {err}");
        }
    }

    pub fn rows(&self) -> Vec<view::DojoRow> {
        view::list_rows(&self.plan, &self.problems, &self.state, self.page, self.redo_only, today_local())
    }

    pub fn header(&self) -> view::DojoHeader {
        view::header(&self.plan, &self.problems, &self.state, self.page, today_local())
    }

    pub fn selected_row(&self) -> Option<view::DojoRow> {
        self.rows().into_iter().nth(self.selected)
    }

    fn clamp_selection(&mut self) {
        let len = self.rows().len();
        self.selected = if len == 0 { 0 } else { self.selected.min(len - 1) };
    }
}

impl AppShell {
    pub(super) fn handle_dojo_command(&mut self, command: &Command) -> Option<bool> {
        match command {
            Command::DojoOpen => Some(self.dojo_open()),
            Command::DojoSelectNext => Some(self.dojo_move_selection(1)),
            Command::DojoSelectPrev => Some(self.dojo_move_selection(-1)),
            Command::DojoToggleRedo => {
                self.dojo.redo_only = !self.dojo.redo_only;
                self.dojo.selected = 0;
                Some(true)
            }
            Command::DojoPageNext => Some(self.dojo_turn_page(1)),
            Command::DojoPagePrev => Some(self.dojo_turn_page(-1)),
            Command::DojoScrollDown => {
                self.dojo.scroll = self.dojo.scroll.saturating_add(10);
                Some(true)
            }
            Command::DojoScrollUp => {
                self.dojo.scroll = self.dojo.scroll.saturating_sub(10);
                Some(true)
            }
            Command::DojoUnfocus => {
                let changed = self.focus_manager.set(FocusTarget::CenterEditor);
                if changed {
                    self.input_handler.clear_pending_prefix();
                }
                Some(true)
            }
            // Filled in by Tasks 10/12/14.
            Command::DojoStart | Command::DojoGiveUp | Command::DojoInterviewer => Some(false),
            _ => None,
        }
    }

    pub(super) fn dojo_open(&mut self) -> bool {
        let _ = self.release_focus_mode_to_editor();
        self.panel_state.right.visible = true;
        self.panel_state.right.switch_to_tab(PanelTabId::Dojo);
        let focus_changed = self.focus_manager.set(FocusTarget::RightSidebar);
        if focus_changed {
            self.input_handler.clear_pending_prefix();
        }
        let _ = self.dismiss_initial_launch_welcome_if_active();
        self.dojo.clamp_selection();
        self.sidebar_needs_layout = true;
        true
    }

    fn dojo_move_selection(&mut self, delta: i32) -> bool {
        let len = self.dojo.rows().len();
        if len == 0 {
            return false;
        }
        let next = (self.dojo.selected as i64 + i64::from(delta)).clamp(0, len as i64 - 1) as usize;
        let changed = next != self.dojo.selected;
        self.dojo.selected = next;
        changed
    }

    fn dojo_turn_page(&mut self, delta: i32) -> bool {
        let pages = self.dojo.plan.pages();
        let Some(idx) = pages.iter().position(|p| *p == self.dojo.page) else {
            return false;
        };
        let next = (idx as i64 + i64::from(delta)).clamp(0, pages.len() as i64 - 1) as usize;
        if next == idx {
            return false;
        }
        self.dojo.page = pages[next];
        self.dojo.selected = 0;
        self.dojo.scroll = 0;
        self.dojo.state.last_group = Some(self.dojo.plan.page_key(self.dojo.page));
        self.dojo.save();
        true
    }
}
```

- [ ] **Step 3: Run** `cargo test --lib dojo_ && cargo test --lib command_ids && cargo test --lib input::tests::dojo` → all green. Run `cargo test --lib` fully — a fixture that enumerates right-dock tabs (e.g. `commands_tests.rs:5058`) may need `PanelTabId::Dojo` appended if it asserts counts; fix only what fails.
- [ ] **Step 4: Leave uncommitted.**

---

### Task 8: Dojo panel renderer (list + session view)

**Files:**
- Modify: `src/render/renderer/ui/test_runner.rs` (new `build_dojo_content`, new `dojo` param on `update_right_dock_panel`), `src/app/event_loop/application.rs` (right-dock model near the `outline` computation ~3765; pass `dojo`), `src/app/event_loop/commands_dojo.rs` (`dojo_panel_model`)
- Create: `src/dojo/view.rs` additions — `DojoPanelModel`, `DojoSessionView`

**Interfaces:**
- Produces (in `dojo::view`):
```rust
pub struct DojoSessionView { pub title: String, pub phase: String, pub remaining: String, pub statement_lines: Vec<String>, pub approach: Option<String>, pub kind: SessionKind, pub expired: bool }
pub struct DojoPanelModel { pub header: DojoHeader, pub rows: Vec<DojoRow>, pub selected: usize, pub scroll: usize, pub redo_only: bool, pub session: Option<DojoSessionView>, pub focused: bool }
```
- `AppShell::dojo_panel_model(&self, focused: bool) -> DojoPanelModel` in `commands_dojo.rs`; when `state.active_session` is `Some` and `armed`, `session` is built from it: statement from `leetcode_cache::load_cache_in(&leetcode_cache::cache_dir(), &id.to_string())` → `html_to_text`, wrapped by the renderer (renderer knows char width) — so `statement_lines` holds paragraphs, and the renderer calls `wrap_text` per paragraph. Phase from `phase_at(plan.dsa_phases, elapsed)` (Dsa) or `single_phase("SD", sd_minutes)` (Sd).
- Renderer: `Renderer::build_dojo_content(&mut self, bounds, model: &DojoPanelModel, inner_padding) -> (Vec<RegionDrawInstance>, Vec<GlyphInstance>)`.

- [ ] **Step 1: `update_right_dock_panel`** gains `dojo: Option<&crate::dojo::view::DojoPanelModel>` after `agent_picker`, and a branch:
```rust
        } else if let Some(model) = dojo {
            let (cc, cg) = self.build_dojo_content(content_bounds, model, inner_padding_for_dojo);
            chrome.extend(cc);
            glyphs.extend(cg);
        }
```
(`inner_padding` isn't a parameter of `update_right_dock_panel`; pass it inside the tuple: `dojo: Option<(&DojoPanelModel, f32)>`.)

- [ ] **Step 2: `build_dojo_content`** (same helpers/tokens as `build_outline_content`; `mm_ss` from `crate::dojo::notebook`):

Layout (all sizes from `self.theme.ui.panel_font_size` / `panel_line_height`, `pad = inner_padding.max(8*scale)`):
1. **Header line**: `DOJO · {page_label} ({page_index}/{page_count})` in `fg`, right-aligned `{overall_done}/{overall_total} · streak {streak}` in `fg_dim`.
2. **Progress line**: a `RegionDrawInstance` bar `bounds.w * 0.45` wide, height `line_h*0.5`, track `border_color`, fill `accent` × `page_done/page_total`; text `{page_done}/{page_total}` after it; right-aligned `redo tới hạn: {n}` in `warning` if n>0 else `fg_ghost`.
3. Separator (1px `border_color`).
4. **Rows** (list view; skip when `model.session.is_some()`): from `model.scroll`, one per `line_h` until `bottom`. Selected row → `selection_bg` full-width rect when `model.focused`, else `with_alpha(selection_bg, 0.5)`. Columns: glyph (2 chars: `↻` `warning`, `·` `fg_ghost`, `○` `fg_dim`, `●` `success`), id right-aligned in 4 chars (`fg_ghost`, blank for `id == 0`), title (`fg` if selected else `fg_dim`, `clip_chars` to fit), trailing right-aligned in `fg_ghost` (clipped first when space is short).
5. **Footer** (last line): list → `[Enter] bắt đầu  [r] chỉ redo  [ ] ] nhóm  [i] interviewer  [Esc] editor` (`fg_ghost`); when `redo_only` prefix with `CHỈ REDO · `.
6. **Session view** (replaces rows 4–5 when `model.session` is `Some(s)`): line `#{id} {title}` (or the case label) in `fg`, right-aligned `{s.phase} {s.remaining}` colored by phase index (0 `info`, 1 `accent`, 2 `warning`, 3 `magenta`; SD `cyan`; when `remaining < 01:00` `error`); separator; statement: for each paragraph `wrap_text(p, max_chars)` lines from `scroll`, `fg_dim`; footer: `Hướng làm: {approach or "(chưa có)"}` + `  [Enter] nhập` when None else `  [Enter] test runner`, then `  [x] bỏ phiên  [i] interviewer` (`[x] kết thúc` for SD).

Empty-state: when `rows.is_empty()` show `Không có bài trên trang này.` (list) / `Chưa có đề (fetch đang chạy…)` (session without statement).

- [ ] **Step 3: `application.rs`** right-dock block: next to `let outline = if is_outline {…}` add
```rust
            let is_dojo = active_tab_id == Some(PanelTabId::Dojo);
            let dojo_model = is_dojo.then(|| self.dojo_panel_model(strip_focused));
            let dojo = dojo_model.as_ref().map(|m| (m, inner_padding));
```
and pass `dojo` into `update_right_dock_panel`. Ensure `is_test_runner`/`is_outline` are computed the same way (`active_tab_id == Some(...)`).

- [ ] **Step 4: `dojo_panel_model`** in `commands_dojo.rs`:
```rust
    pub(super) fn dojo_panel_model(&self, focused: bool) -> crate::dojo::view::DojoPanelModel {
        use crate::dojo::{notebook::{html_to_text, mm_ss}, session::{phase_at, single_phase}, state::now_unix, view::{DojoPanelModel, DojoSessionView}};
        let session = self.dojo.state.active_session.as_ref().filter(|_| self.dojo.armed).map(|s| {
            let now = now_unix();
            let phases = match s.kind {
                SessionKind::Dsa => self.dojo.plan.dsa_phases.clone(),
                SessionKind::Sd => single_phase("SD", self.dojo.plan.sd_minutes),
            };
            let phase = phase_at(&phases, s.elapsed_s(now));
            let statement = self.dojo.problems.by_slug(&s.slug).and_then(|p| {
                crate::runner::leetcode_cache::load_cache_in(&crate::runner::leetcode_cache::cache_dir(), &p.id.to_string())
            }).map(|c| html_to_text(&c.statement)).unwrap_or_default();
            let id = self.dojo.problems.by_slug(&s.slug).map(|p| p.id).unwrap_or(0);
            DojoSessionView {
                title: if id > 0 { format!("#{id} {}", s.title) } else { s.title.clone() },
                phase: phase.as_ref().map(|p| p.name.clone()).unwrap_or_else(|| "HẾT GIỜ".to_string()),
                phase_index: phase.as_ref().map(|p| p.index).unwrap_or(usize::MAX),
                remaining: mm_ss(s.remaining_s(now)),
                remaining_s: s.remaining_s(now),
                statement_lines: statement.split("\n\n").map(str::to_string).collect(),
                approach: s.approach.clone(),
                kind: s.kind,
                expired: s.is_expired(now),
            }
        });
        DojoPanelModel { header: self.dojo.header(), rows: self.dojo.rows(), selected: self.dojo.selected, scroll: self.dojo.scroll, redo_only: self.dojo.redo_only, session, focused }
    }
```
(Add `phase_index: usize` and `remaining_s: u64` to `DojoSessionView`.) `load_cache_in` is a sync disk read of a small JSON on the UI thread each frame — cache it: add `statement_cache: Option<(String, String)>` (slug, text) to `DojoRuntime`, filled on first miss; `dojo_panel_model` takes `&mut self` then. Do that.

- [ ] **Step 5: Verify** `cargo check` clean, `cargo test --lib` green, then `cargo run`: `g o` shows the list with real titles; `j/k`, `[`/`]`, `r` work; Esc returns. Report to the user for a visual check.
- [ ] **Step 6: Leave uncommitted.**

---

### Task 9: Worker job `WriteTextFiles`

**Files:**
- Modify: `src/async_runtime/message.rs` (request + result variants), `src/async_runtime/scheduler/dispatch.rs` (handler, next to `CopyFile`), `src/app/event_loop/async_results/mod.rs` (route), Create: `src/app/event_loop/async_results/dojo.rs`
- Test: `src/async_runtime/scheduler/dispatch.rs` tests (or a new `src/async_runtime/scheduler/text_files.rs` holding the pure `apply_text_ops` fn + tests — do this; dispatch just calls it).

**Interfaces:**
```rust
// message.rs
pub enum TextFileOp {
    /// Create or overwrite.
    Write { path: PathBuf, contents: String },
    /// Create with `header` if missing, then append `contents`.
    Append { path: PathBuf, header: String, contents: String },
    /// Write only when the file does not exist.
    WriteIfMissing { path: PathBuf, contents: String },
    Remove { path: PathBuf },
}
WorkerRequestPayload::WriteTextFiles { ops: Vec<TextFileOp> }      // topic FileOperation
WorkerResultPayload::TextFilesWritten { failures: Vec<(PathBuf, String)> }
// scheduler/text_files.rs
pub async fn apply_text_ops(ops: Vec<TextFileOp>) -> Vec<(PathBuf, String)>   // sequential, create_dir_all parents
```
- Handler `async_results/dojo.rs::handle_text_files_written`: for each failure `show_transient_toast_kind(format!("Dojo\nKhông ghi được {}: {err}", path.display()), ToastKind::Error)`.
- `AppShell::submit_text_file_ops(&mut self, ops: Vec<TextFileOp>)` helper in `commands_dojo.rs`: `self.submit(RequestSpec { revision_id: 0, topic: RequestTopic::FileOperation, payload: WorkerRequestPayload::WriteTextFiles { ops } });`

- [ ] **Step 1: Failing test** (`text_files.rs`):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ops_apply_in_order_and_report_failures() {
        let rt = tokio::runtime::Runtime::new().expect("rt");
        let dir = std::env::temp_dir().join(format!("dojo_ops_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let nb = dir.join("notes/interview-notes.md");
        let failures = rt.block_on(apply_text_ops(vec![
            TextFileOp::Append { path: nb.clone(), header: "# H\n\n".into(), contents: "a\n".into() },
            TextFileOp::Append { path: nb.clone(), header: "# H\n\n".into(), contents: "b\n".into() },
            TextFileOp::WriteIfMissing { path: dir.join("x.md"), contents: "1".into() },
            TextFileOp::WriteIfMissing { path: dir.join("x.md"), contents: "2".into() },
            TextFileOp::Write { path: dir.join("cur.md"), contents: "c".into() },
            TextFileOp::Remove { path: dir.join("cur.md") },
            TextFileOp::Remove { path: dir.join("missing.md") },
            TextFileOp::Write { path: dir.join("x.md").join("child"), contents: "boom".into() },
        ]));
        assert_eq!(std::fs::read_to_string(&nb).expect("nb"), "# H\n\na\nb\n");
        assert_eq!(std::fs::read_to_string(dir.join("x.md")).expect("x"), "1");
        assert!(!dir.join("cur.md").exists());
        assert_eq!(failures.len(), 1, "only the impossible path fails; missing remove is fine");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```
- [ ] **Step 2: Implement** `apply_text_ops` with `tokio::fs` (`create_dir_all` parent; `Append` = `OpenOptions::new().create(true).append(true)` after writing `header` when `!path.exists()`; `Remove` ignores `NotFound`). Dispatch arm mirrors `CopyFile` (`matches!` + `tokio::spawn` + `emit_message_and_wake` with `TextFilesWritten`). Route in `async_results/mod.rs`.
- [ ] **Step 3: Run** `cargo test --lib text_files` → 1 passed. `cargo check` clean. **Leave uncommitted.**

---

### Task 10: Session start — fetch, approach gate, current.md

**Files:**
- Modify: `src/app/command_palette.rs` (`CommandPaletteMode::DojoApproach`, `DojoNote(u8)` + prompt/hint/title arms + `Vec::new()` arm in `refresh_results`), `src/core/command_dispatch/editing.rs` (paste allow-list), `src/app/event_loop/commands_palette.rs` (confirm branches), `src/app/event_loop/commands_terminal.rs` (`submit_leetcode_fetch` → `pub(super)`), `src/app/event_loop/async_results/leetcode_fetch.rs` (dojo branch), `src/app/event_loop/commands_dojo.rs`
- Test: `src/app/event_loop/commands_tests.rs`

**Interfaces:**
- `CommandPaletteMode::DojoApproach` → prefix `"hướng làm> "`, placeholder `"hướng làm + độ phức tạp, rồi Enter"`, title `"DOJO · THINK"`.
- `CommandPaletteMode::DojoNote(step)` → prefix `"sổ lỗi> "`; placeholder by step: 0 `"ghi chú (Enter bỏ qua)"`, 1 `"bí ở đâu?"`, 2 `"pattern đúng là gì?"`, 3 `"dấu hiệu nhận biết lần sau?"`; title `"DOJO · SỔ LỖI"`. (`&'static str` per arm; match on the step.)
- `AppShell::dojo_start_selected(&mut self) -> bool` (DojoStart), `dojo_begin_dsa_session(slug, title, file, language)`, `confirm_dojo_approach(&mut self) -> bool`, `dojo_write_current_md(&mut self, approach: Option<&str>)`, `dojo_ensure_interviewer_prompt(&mut self)`.

- [ ] **Step 1: Failing test** (`commands_tests.rs`):
```rust
#[test]
fn dojo_fetch_result_creates_a_gated_session_instead_of_opening_the_file() {
    use crate::async_runtime::message::WorkerResultPayload;
    let mut shell = AppShell::new_for_tests().expect("shell");
    let _ = shell.handle_command(Command::DojoOpen);
    shell.dojo.pending_start = Some("two-sum".to_string());
    let file = std::env::temp_dir().join("netherize_dojo_test_solution.js");
    std::fs::write(&file, "// x\n").expect("write");
    super::async_results::leetcode_fetch::handle_leetcode_fetch_result(
        &mut shell,
        WorkerResultPayload::LeetCodeProblemFetched {
            title: "Two Sum".into(),
            title_slug: "two-sum".into(),
            language_key: "javascript".into(),
            file_path: file.clone(),
            cases: vec![],
        },
    );
    let session = shell.dojo.state.active_session.clone().expect("session");
    assert_eq!((session.slug.as_str(), session.approach.as_deref(), session.budget_s), ("two-sum", None, 1500));
    assert!(shell.dojo.armed);
    assert_ne!(shell.app_state.active_file(), Some(file.as_path()), "file stays closed until approach");
    assert_eq!(shell.app_state.command_palette_mode(), Some(CommandPaletteMode::DojoApproach));
    // Empty approach is refused.
    let _ = shell.app_state.set_command_palette_query("   ");
    assert!(shell.confirm_dojo_approach());
    assert!(shell.dojo.state.active_session.as_ref().and_then(|s| s.approach.clone()).is_none());
    // Real approach opens the file and moves to the Test Runner.
    let _ = shell.app_state.set_command_palette_query("hash map, O(n)");
    assert!(shell.confirm_dojo_approach());
    assert_eq!(shell.dojo.state.active_session.as_ref().and_then(|s| s.approach.clone()).as_deref(), Some("hash map, O(n)"));
    assert_eq!(shell.app_state.active_file(), Some(file.as_path()));
    assert_eq!(shell.panel_state.right.active_tab_id(), Some(PanelTabId::TestRunner));
    let _ = std::fs::remove_file(&file);
}
```
(`handle_leetcode_fetch_result` is `pub(super)` inside `async_results`; make it `pub(in crate::app::event_loop)` so the test module can call it.)

- [ ] **Step 2: Implement**

`leetcode_fetch.rs` — right after destructuring the payload, before `OpenFile`:
```rust
    if app.dojo.pending_start.as_deref() == Some(title_slug.as_str()) {
        app.dojo.pending_start = None;
        app.app_state.test_runner.cases = cases.into_iter().map(|c| crate::runner::TestCase::new(c.input, c.expected)).collect();
        app.app_state.test_runner.selected = (!app.app_state.test_runner.cases.is_empty()).then_some(0);
        app.app_state.test_runner.focused_field = crate::runner::TestField::Input;
        app.app_state.test_runner.is_running = false;
        app.app_state.test_runner.launch_error = None;
        app.dojo_begin_dsa_session(title_slug, title, file_path, language_key);
        app.request_redraw();
        return;
    }
```
Also in the failure arm: `app.dojo.pending_start = None;`.

`commands_dojo.rs` additions:
```rust
    pub(super) fn dojo_start_selected(&mut self) -> bool {
        if self.dojo.state.active_session.is_some() && self.dojo.armed {
            // A session is running: Enter reopens the approach prompt or jumps to the runner.
            let needs_approach = self.dojo.state.active_session.as_ref().is_some_and(|s| s.approach.is_none() && s.kind == SessionKind::Dsa);
            if needs_approach {
                return self.open_prompt_overlay(crate::app::command_palette::CommandPaletteMode::DojoApproach);
            }
            return self.handle_test_runner_focus_public();
        }
        let Some(row) = self.dojo.selected_row() else { return false; };
        match row.kind {
            SessionKind::Sd => self.dojo_begin_sd_session(&row.slug, &row.title), // Task 13
            SessionKind::Dsa => {
                let language = self.persistent_state.recent_leetcode_languages.first().cloned().unwrap_or_else(|| "javascript".to_string());
                self.dojo.pending_start = Some(row.slug.clone());
                self.submit_leetcode_fetch(row.slug.clone(), language);
                self.show_transient_toast_kind(format!("Dojo\nĐang tải #{} {}…", row.id, row.title), ToastKind::Info);
                true
            }
        }
    }

    pub(super) fn dojo_begin_dsa_session(&mut self, slug: String, title: String, file: PathBuf, _language: String) {
        let budget_s = self.dojo.plan.dsa_budget_s();
        self.dojo.state.active_session = Some(ActiveSession { kind: SessionKind::Dsa, slug, title, started_unix: now_unix(), budget_s, approach: None, file });
        self.dojo.armed = true;
        self.dojo.last_phase = Some(0);
        self.dojo.statement_cache = None;
        self.dojo.save();
        self.dojo_ensure_interviewer_prompt();
        self.dojo_write_current_md(None);
        self.dojo_open();
        let minutes = self.dojo.plan.dsa_phases.first().map(|p| p.1).unwrap_or(3);
        self.show_transient_toast_kind(format!("THINK · {minutes} phút\nĐọc đề, gõ hướng làm + độ phức tạp."), ToastKind::Info);
        if !self.open_prompt_overlay(CommandPaletteMode::DojoApproach) {
            self.show_transient_toast("Dojo\nEnter trong panel để nhập hướng làm.");
        }
    }

    pub(super) fn confirm_dojo_approach(&mut self) -> bool {
        let text = self.app_state.command_palette_query_text().trim().to_string();
        if text.is_empty() {
            self.show_transient_toast_kind("Dojo\nGõ hướng làm trước đã (Esc = để sau).", ToastKind::Warning);
            return true;
        }
        let Some(file) = self.dojo.state.active_session.as_mut().map(|s| { s.approach = Some(text.clone()); s.file.clone() }) else { return false; };
        self.dojo.save();
        self.dojo_write_current_md(Some(&text));
        let _ = self.app_state.close_command_palette();
        if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::ExitFocus) { let _ = result; }
        let report = dispatch_command(&mut self.app_state, Command::OpenFile(file.clone()));
        if report.success {
            self.clear_highlight_layers();
            self.submit_workspace_rescan();
            self.submit_active_buffer_git_baseline_refresh();
            self.submit_parse_for_active_buffer(true);
            self.submit_lsp_did_open_for_active_file();
            self.explorer_reveal_file(&file);
            self.submit_lsp_check_for_path(file);
        }
        self.panel_state.right.visible = true;
        self.panel_state.right.switch_to_tab(PanelTabId::TestRunner);
        self.focus_manager.set(FocusTarget::CenterEditor);
        self.input_handler.clear_pending_prefix();
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        let code_minutes = self.dojo.plan.dsa_phases.get(1).map(|p| p.1).unwrap_or(15);
        self.show_transient_toast_kind(format!("CODE · {code_minutes} phút\nF5 chạy case. Pass hết = xong."), ToastKind::Success);
        true
    }

    pub(super) fn dojo_write_current_md(&mut self, approach: Option<&str>) {
        use crate::dojo::files::{current_md, current_md_path};
        let Some(s) = self.dojo.state.active_session.clone() else { return; };
        let (id, statement) = self.dojo_problem_context(&s.slug);
        let phases = match s.kind { SessionKind::Dsa => self.dojo.plan.dsa_phases.clone(), SessionKind::Sd => single_phase("SD", self.dojo.plan.sd_minutes) };
        let language = self.persistent_state.recent_leetcode_languages.first().cloned().unwrap_or_default();
        let text = current_md(s.kind, id, &s.title, &statement, &language, &phases, approach.or(s.approach.as_deref()));
        self.submit_text_file_ops(vec![TextFileOp::Write { path: current_md_path(), contents: text }]);
    }

    /// (id, plain statement) for a slug — statement from the LeetCode cache when present.
    fn dojo_problem_context(&mut self, slug: &str) -> (u32, String) { /* uses statement_cache like dojo_panel_model */ }

    pub(super) fn dojo_ensure_interviewer_prompt(&mut self) {
        use crate::dojo::files::{INTERVIEWER_PROMPT, interviewer_md_path};
        self.submit_text_file_ops(vec![TextFileOp::WriteIfMissing { path: interviewer_md_path(), contents: INTERVIEWER_PROMPT.to_string() }]);
    }
```
`handle_test_runner_focus` is private in `commands_terminal.rs` — make it `pub(super)` and call it directly (drop the `_public` name).

`commands_palette.rs`: add confirm branches next to the `LeetCodeProblemInput` one:
```rust
                if matches!(command, Command::FilePickerConfirmSelection)
                    && matches!(self.app_state.command_palette_mode(), Some(CommandPaletteMode::DojoApproach))
                {
                    return Some(self.confirm_dojo_approach());
                }
                if matches!(command, Command::FilePickerConfirmSelection)
                    && let Some(CommandPaletteMode::DojoNote(step)) = self.app_state.command_palette_mode()
                {
                    return Some(self.confirm_dojo_note(step)); // Task 12
                }
```
And in the "keep palette focus" block (~687) add `DojoApproach` / `DojoNote(_)` alongside `LeetCodeProblemInput` so opening from the palette keeps overlay focus.

`editing.rs` paste allow-list: add `| CommandPaletteMode::DojoApproach | CommandPaletteMode::DojoNote(_)`.

`command_palette.rs`: add the two variants + arms (`prompt_prefix`, placeholder, title) + `CommandPaletteMode::DojoApproach | CommandPaletteMode::DojoNote(_) => Vec::new(),` in `refresh_results`. `CommandPaletteMode` derives `Copy, Eq` — `DojoNote(u8)` is fine. Grep for every exhaustive `match` on `CommandPaletteMode` (`is_complex_picker`, renderer title arms) and add arms; the compiler lists them.

Wire `Command::DojoStart => Some(self.dojo_start_selected())` in `handle_dojo_command`.

- [ ] **Step 3: Run** the test + full `cargo test --lib`. `cargo run`: `g o`, Enter on a row → toast "Đang tải", approach prompt appears, statement visible in the panel, Enter with text opens `solution.js` + Test Runner. **Leave uncommitted.**

---

### Task 11: Timer — tick, statusbar chip, phase toasts, auto-stop

**Files:**
- Modify: `src/app/event_loop/application.rs` (`about_to_wait`: tick + deadline; statusbar call site), `src/render/renderer/ui/statusbar.rs` (`dojo_chip` param + layout key + right item), `src/app/event_loop/commands_dojo.rs`
- Test: `commands_tests.rs`

**Interfaces:**
- `AppShell::dojo_tick(&mut self) -> bool` (needs redraw), `AppShell::dojo_statusbar_chip(&self) -> Option<(String, [f32; 4])>`, `AppShell::dojo_end_session(&mut self, outcome: Outcome)` (Task 12 fills the body; here it only clears the session, records the attempt and saves).
- `update_statusbar_content(..., go_version, dojo_chip: Option<(&str, [f32; 4])>, bounds)`; `StatusbarLayoutKey.dojo_chip: Option<(String, [f32; 4])>`; item pushed right before the `AI inline completion chip` block.

- [ ] **Step 1: Failing test**
```rust
#[test]
fn dojo_tick_reports_phase_changes_and_expires_the_session() {
    let mut shell = AppShell::new_for_tests().expect("shell");
    let now = crate::dojo::state::now_unix();
    shell.dojo.state.active_session = Some(crate::dojo::state::ActiveSession {
        kind: SessionKind::Dsa, slug: "two-sum".into(), title: "Two Sum".into(),
        started_unix: now - 181, budget_s: 1500, approach: Some("x".into()), file: "/tmp/none.js".into(),
    });
    shell.dojo.armed = true;
    shell.dojo.last_phase = Some(0);
    assert!(shell.dojo_tick(), "second changed → redraw");
    assert_eq!(shell.dojo.last_phase, Some(1), "THINK → CODE at 180s");
    let chip = shell.dojo_statusbar_chip().expect("chip");
    assert!(chip.0.starts_with("⏱ CODE "), "{}", chip.0);
    shell.dojo.state.active_session.as_mut().expect("s").started_unix = now - 1500;
    assert!(shell.dojo_tick());
    assert!(shell.dojo.state.active_session.is_none(), "expired → ended");
    assert_eq!(shell.dojo.state.attempts.last().map(|a| a.outcome), Some(Outcome::Timeout));
    assert!(shell.dojo_statusbar_chip().is_none());
    assert!(!shell.dojo_tick(), "idle → no redraw");
}
```
- [ ] **Step 2: Implement**
```rust
    /// Once per event-loop turn. Returns true when the statusbar/panel must redraw.
    pub(super) fn dojo_tick(&mut self) -> bool {
        self.dojo_flush_abandoned_note(); // Task 12 (no-op until then)
        let Some(session) = self.dojo.state.active_session.clone().filter(|_| self.dojo.armed) else { return false; };
        let now = now_unix();
        if session.is_expired(now) {
            self.dojo_end_session(Outcome::Timeout);
            return true;
        }
        let phases = match session.kind { SessionKind::Dsa => self.dojo.plan.dsa_phases.clone(), SessionKind::Sd => single_phase("SD", self.dojo.plan.sd_minutes) };
        if let Some(phase) = phase_at(&phases, session.elapsed_s(now)) && self.dojo.last_phase != Some(phase.index) {
            self.dojo.last_phase = Some(phase.index);
            let minutes = phases.get(phase.index).map(|p| p.1).unwrap_or(0);
            let hint = match phase.name.as_str() { "CODE" => "Gõ đi. F5 để chạy case.", "TEST" => "Tự test: rỗng, 1 phần tử, trùng, âm, tràn.", "REVIEW" => "Xem lời giải tối ưu, ghi sổ nếu lệch.", _ => "" };
            self.show_transient_toast_kind(format!("{} · {minutes} phút\n{hint}", phase.name), ToastKind::Info);
            self.dojo.last_tick_second = now;
            return true;
        }
        if self.dojo.last_tick_second != now { self.dojo.last_tick_second = now; return true; }
        false
    }

    pub(super) fn dojo_statusbar_chip(&self) -> Option<(String, [f32; 4])> {
        let s = self.dojo.state.active_session.as_ref().filter(|_| self.dojo.armed)?;
        let now = now_unix();
        let phases = match s.kind { SessionKind::Dsa => self.dojo.plan.dsa_phases.clone(), SessionKind::Sd => single_phase("SD", self.dojo.plan.sd_minutes) };
        let phase = phase_at(&phases, s.elapsed_s(now));
        let name = phase.as_ref().map(|p| p.name.clone()).unwrap_or_else(|| "HẾT".into());
        let remaining = s.remaining_s(now);
        let ui = &self.theme.ui;
        let color = if remaining < 60 { ui.error.as_f32() } else { match (s.kind, phase.map(|p| p.index)) { (SessionKind::Sd, _) => ui.cyan.as_f32(), (_, Some(0)) => ui.info.as_f32(), (_, Some(1)) => ui.accent.as_f32(), (_, Some(2)) => ui.warning.as_f32(), _ => ui.magenta.as_f32() } };
        Some((format!("⏱ {name} {}", mm_ss(remaining)), color))
    }
```
`about_to_wait` (application.rs:2373, after the recovery snapshot block): `if self.dojo_tick() { self.request_redraw(); }` and in the deadline section: `if self.dojo.state.active_session.is_some() && self.dojo.armed { let ms = 1000 - u64::from(Instant::now().elapsed_millis_hack) … }` — concretely: `let tick = Instant::now() + Duration::from_millis(1000 - (crate::dojo::state::now_millis() % 1000));` — add `pub fn now_millis() -> u64` to `state.rs` (`as_millis`). Merge into `next_deadline` like `whichkey_deadline`.

Statusbar: add the param, key field, and `if let Some((label, color)) = dojo_chip { right_items.push((label.to_string(), color)); }` before the AI chip. Call site: `let dojo_chip = self.dojo_statusbar_chip(); … dojo_chip.as_ref().map(|(l, c)| (l.as_str(), *c)),`.

`dojo_end_session` minimal body for this task (Task 12 extends it): clear `active_session`, push `Attempt` (elapsed clamped to budget), `apply` via `record_attempt(today_local())`, `armed = false`, `last_phase = None`, save, remove `current.md` (`TextFileOp::Remove`).

- [ ] **Step 3: Run** tests; `cargo run`: chip counts down, toast at phase change, at 25:00 the session ends (temporarily set `dsa_phases = [["THINK",1],["CODE",1]]` in `~/.config/netherize/dojo/plan.toml` to test fast, then delete that file). **Leave uncommitted.**

---

### Task 12: Session end — pass detection, give up, notebook prompts, SRS, resume

**Files:**
- Modify: `src/app/event_loop/async_results/runner.rs` (all-passed hook), `src/app/event_loop/commands_dojo.rs`, `src/app/event_loop/commands_palette.rs` (DojoNote confirm, from Task 10)
- Test: `commands_tests.rs`

**Interfaces:**
- `AppShell::dojo_on_run_completed(&mut self, all_passed: bool)`, `dojo_give_up(&mut self) -> bool`, `dojo_end_session(outcome)` (full), `confirm_dojo_note(step: u8) -> bool`, `dojo_flush_pending_note(&mut self)`, `dojo_flush_abandoned_note(&mut self)`, `dojo_resume_if_needed(&mut self)` (called from `dojo_open`).

- [ ] **Step 1: Failing tests**
```rust
#[test]
fn dojo_all_passed_ends_the_session_as_pass_and_opens_the_note_prompt() {
    let mut shell = AppShell::new_for_tests().expect("shell");
    let file = std::env::temp_dir().join("netherize_dojo_pass.js");
    std::fs::write(&file, "// x\n").expect("write");
    let _ = shell.handle_command(Command::OpenFile(file.clone()));
    let now = crate::dojo::state::now_unix();
    shell.dojo.state.active_session = Some(crate::dojo::state::ActiveSession { kind: SessionKind::Dsa, slug: "two-sum".into(), title: "Two Sum".into(), started_unix: now - 700, budget_s: 1500, approach: Some("hash".into()), file: file.clone() });
    shell.dojo.armed = true;
    shell.dojo_on_run_completed(false);
    assert!(shell.dojo.state.active_session.is_some(), "failures keep the session");
    shell.dojo_on_run_completed(true);
    assert!(shell.dojo.state.active_session.is_none());
    assert_eq!(shell.dojo.state.status_of("two-sum"), crate::dojo::state::Status::Done);
    assert_eq!(shell.app_state.command_palette_mode(), Some(CommandPaletteMode::DojoNote(0)));
    let note = shell.dojo.pending_note.as_ref().expect("note");
    assert_eq!((note.id, note.outcome, note.redo), (1, Outcome::Pass, false));
    let _ = shell.app_state.set_command_palette_query("ok");
    assert!(shell.confirm_dojo_note(0));
    assert!(shell.dojo.pending_note.is_none(), "single step for pass → flushed");
    assert!(shell.app_state.command_palette_mode().is_none());
    let _ = std::fs::remove_file(&file);
}

#[test]
fn dojo_give_up_walks_three_note_steps_and_flushes_on_abandon() {
    let mut shell = AppShell::new_for_tests().expect("shell");
    let now = crate::dojo::state::now_unix();
    shell.dojo.state.active_session = Some(crate::dojo::state::ActiveSession { kind: SessionKind::Dsa, slug: "min-stack".into(), title: "Min Stack".into(), started_unix: now - 100, budget_s: 1500, approach: None, file: "/tmp/none.js".into() });
    shell.dojo.armed = true;
    assert!(shell.dojo_give_up());
    assert_eq!(shell.dojo.state.status_of("min-stack"), crate::dojo::state::Status::Redo);
    assert_eq!(shell.app_state.command_palette_mode(), Some(CommandPaletteMode::DojoNote(1)));
    let _ = shell.app_state.set_command_palette_query("quên stack phụ");
    assert!(shell.confirm_dojo_note(1));
    assert_eq!(shell.app_state.command_palette_mode(), Some(CommandPaletteMode::DojoNote(2)));
    let _ = shell.app_state.close_command_palette(); // user pressed Esc
    shell.dojo_flush_abandoned_note();
    assert!(shell.dojo.pending_note.is_none(), "partial note is written, not lost");
}
```
(The flush submits a worker request; `new_for_tests` uses a scheduler that drops requests, so only the in-memory effects are asserted.)

- [ ] **Step 2: Implement**
- `runner.rs`: after the toast, `app.dojo_on_run_completed(runner_all_passed);` where `runner_all_passed = passed == total && total > 0` computed before the borrow ends.
- `dojo_on_run_completed(all_passed)`: `if !all_passed { return; }` then if active session (armed, Dsa) and `self.app_state.active_file() == Some(session.file.as_path())` → `dojo_end_session(Outcome::Pass)`.
- `dojo_give_up()`: needs session; outcome = if `self.app_state.test_runner.cases.iter().any(|c| c.status == TestStatus::Failed)` → `Fail` else `Giveup` (SD → `Pass`, see Task 13). Wire `Command::DojoGiveUp`.
- `dojo_end_session(outcome)`: (extends Task 11) after recording, build `PendingNote { date: date_str(today_local()), id, title, outcome, elapsed_s, redo: state.status_of(slug) == Redo, approach, kind, answers: vec![] }`; toast summary `"{title} · {outcome} {mm:ss} · streak {n}"` or `"… · redo {dd/mm}"`; `dojo_open()`; open `DojoNote(if outcome == Pass { 0 } else { 1 })` via `open_prompt_overlay`.
- `confirm_dojo_note(step)`: read query; label by step (`0 "Ghi chú"`, `1 "Bí"`, `2 "Pattern"`, `3 "Dấu hiệu"`); push `(label, text)` to `pending_note.answers`; if `step == 0 || step == 3` → `dojo_flush_pending_note()`, close palette + `ExitFocus` + focus RightSidebar; else `open_prompt_overlay(DojoNote(step + 1))` (this replaces the mode while staying in PaletteFocus; if the query text isn't cleared by `open_command_palette_mode`, call `set_command_palette_query("")`).
- `dojo_flush_pending_note()`: take `pending_note`; contents = `format_block(...)` (or `format_sd_block` for Sd using the first answer as the note); `TextFileOp::Append { path: expand_tilde(&plan.notebook), header: NOTEBOOK_HEADER, contents }`.
- `dojo_flush_abandoned_note()`: `if pending_note.is_some() && !matches!(self.app_state.command_palette_mode(), Some(CommandPaletteMode::DojoNote(_))) { self.dojo_flush_pending_note(); }`.
- `dojo_resume_if_needed()` in `dojo_open()`: if `active_session.is_some() && !armed` → `armed = true; last_phase = None; statement_cache = None;` (an expired one ends on the next tick with `Timeout`; a live one shows the chip; if `approach.is_some()` and the file exists, `OpenFile` it).

- [ ] **Step 3: Run** tests + `cargo test --lib`. `cargo run`: full loop on a real problem (pass via F5, give up via `x`, check `~/Work/docs/interview-notes.md` and `~/.config/netherize/dojo.toml`). **Leave uncommitted.**

---

### Task 13: System-design sessions

**Files:**
- Modify: `src/app/event_loop/commands_dojo.rs`
- Test: `commands_tests.rs`

**Interfaces:**
- `AppShell::dojo_begin_sd_session(&mut self, key: &str, label: &str) -> bool`.

- [ ] **Step 1: Failing test**
```rust
#[test]
fn dojo_sd_session_creates_the_outline_and_runs_a_45_minute_clock() {
    let mut shell = AppShell::new_for_tests().expect("shell");
    let dir = std::env::temp_dir().join(format!("dojo_sd_{}", std::process::id()));
    shell.dojo.plan.sd_dir = dir.to_string_lossy().to_string();
    assert!(shell.dojo_begin_sd_session("url_shortener", "Rút gọn URL"));
    let path = dir.join("url_shortener.md");
    assert!(std::fs::read_to_string(&path).expect("outline").starts_with("# Rút gọn URL — "));
    let s = shell.dojo.state.active_session.clone().expect("session");
    assert_eq!((s.kind, s.budget_s, s.file.as_path()), (SessionKind::Sd, 2700, path.as_path()));
    assert_eq!(shell.app_state.active_file(), Some(path.as_path()));
    assert!(shell.dojo_give_up(), "x finishes an SD session");
    assert_eq!(shell.dojo.state.attempts.last().map(|a| (a.kind, a.outcome)), Some((SessionKind::Sd, Outcome::Pass)));
    assert_eq!(shell.app_state.command_palette_mode(), Some(CommandPaletteMode::DojoNote(0)));
    let _ = std::fs::remove_dir_all(&dir);
}
```
- [ ] **Step 2: Implement**: path = `expand_tilde(&plan.sd_dir).join(format!("{key}.md"))`; `create_dir_all`; if missing write `sd_template(label, &date_str(today_local()))` synchronously (`// ponytail: tiny file written sync so OpenFile below sees it; state.toml precedent`); `OpenFile` + the post-open submits (same block as `confirm_dojo_approach`); `self.handle_command(Command::FocusMarkdownPreview)` then `Command::FocusEditor`; session `{ kind: Sd, slug: key, title: label, budget_s: plan.sd_budget_s(), approach: None, file }`, `armed = true`, `last_phase = Some(0)`, save, write current.md, toast `"SD · 45 phút\n1 Yêu cầu 5' → 2 Quy mô 5' → 3 API 5' → 4 Kiến trúc 10' → 5 Đào sâu 15' → 6 Đánh đổi 5'"`. `dojo_give_up` for Sd → `Outcome::Pass`. `dojo_end_session` for Sd builds the note with `format_sd_block` (single `DojoNote(0)` step). `list_rows` already marks SD done from attempts.
- [ ] **Step 3: Run** tests; `cargo run`: `]` to the SD page, Enter → outline + preview + chip. **Leave uncommitted.**

---

### Task 14: AI Interviewer agent + `i`

**Files:**
- Modify: `src/app/ai_agents.rs`, `src/app/event_loop/commands_dojo.rs`
- Test: `src/app/ai_agents.rs` tests, `commands_tests.rs`

- [ ] **Step 1: Failing tests**
```rust
// ai_agents.rs
#[test]
fn interviewer_agent_reads_the_dojo_prompt_file() {
    let a = ai_agent("interviewer").expect("interviewer");
    assert!(a.command.starts_with("claude --append-system-prompt"));
    assert!(a.command.contains("dojo/interviewer.md"));
}
// commands_tests.rs
#[test]
fn dojo_interviewer_launches_the_agent_terminal_for_the_selected_row() {
    let mut shell = AppShell::new_for_tests().expect("shell");
    let _ = shell.handle_command(Command::DojoOpen);
    assert!(shell.handle_command(Command::DojoInterviewer));
    assert_eq!(shell.right_agent_label.as_deref(), Some("Interviewer (claude)"));
    assert!(shell.pending_right_pty_spawn);
    assert_eq!(shell.panel_state.right.active_tab_id(), Some(PanelTabId::AiChat));
}
```
- [ ] **Step 2: Implement**: append to `default_ai_agents()`:
```rust
        AiAgent {
            id: "interviewer",
            label: "Interviewer (claude)",
            command: "claude --append-system-prompt \"$(cat ~/.config/netherize/dojo/interviewer.md)\"",
        },
```
`dojo_launch_interviewer()`: `dojo_ensure_interviewer_prompt()`; write current.md for the selected row when no session (`TextFileOp::Write` with `current_md(row.kind, row.id, &row.title, &statement, &language, &phases, None)`) or for the session; `let Some(agent) = ai_agent("interviewer")`; `self.spawn_right_agent_terminal(agent.command, agent.label)`; `switch_to_tab(AiChat)`; `focus RightSidebar`; `apply_mode_event(ModeEvent::FocusTerminal)`; `right_terminal_needs_layout = true`; toast `"Interviewer\nĐang mở claude… nói hướng làm trước khi code."`. Wire `Command::DojoInterviewer`.
- [ ] **Step 3: Run** tests; `cargo run`: `i` opens Claude Code in the right dock with the interviewer prompt (`claude` must be on PATH). **Leave uncommitted.**

---

### Task 15: Welcome card

**Files:**
- Modify: `src/render/renderer/ui/welcome.rs` (`dojo_card: Option<(&str, &str)>` param; `action_cards` → `Vec`; push a 4th card with section `"DOJO"`, icon `"built_in:flask"`, color `self.theme.ui.magenta.as_f32()`, keys `&["g", "o"][..]`), `src/app/event_loop/application.rs` (call site ~3161), `src/app/event_loop/commands_dojo.rs` (`dojo_welcome_card(&self) -> (String, String)`)
- Test: `src/dojo/view.rs` (pure formatting)

- [ ] **Step 1: Failing test** (`view.rs`):
```rust
#[test]
fn welcome_card_text() {
    let (plan, problems, mut state, today) = fixture();
    let (title, sub) = welcome_card(&plan, &problems, &state, Page::Group(0), today);
    assert_eq!(title, "○ #217 Contains Duplicate");
    assert_eq!(sub, "Array/Hash Map, Two Pointers 0/14 · 0/150 · streak 0");
    state.record_attempt(attempt("min-stack", Outcome::Timeout, 1500), parse_date("2026-09-01").expect("d"));
    let (title, _) = welcome_card(&plan, &problems, &state, Page::Group(0), today);
    assert_eq!(title, "↻ 1 redo tới hạn · #155 Min Stack");
}
```
- [ ] **Step 2: Implement** `pub fn welcome_card(plan, problems, state, page, today) -> (String, String)` in `view.rs` using `suggested_next` + `header`: title `"↻ {n} redo tới hạn · #{id} {title}"` when `redo_due > 0`, else `"○ #{id} {title}"`, else `"Hết bài — tự thêm vào neetcode150.toml"`; sub `"{page_label} {page_done}/{page_total} · {overall_done}/{overall_total} · streak {streak}"`. Renderer + call site: `let (dojo_title, dojo_sub) = self.dojo_welcome_card(); … Some((dojo_title.as_str(), dojo_sub.as_str()))`.
- [ ] **Step 3: Run** tests; `cargo run` welcome screen shows the card; `g o` from welcome works (`Welcome` focus allows leader/`g` chords? verify — if `g o` is swallowed on the welcome screen, add `dojo.open` handling in the welcome input path the same way `⌘O` is handled, or accept palette-only there and note it). **Leave uncommitted.**

---

### Task 16: Docs, lessons, index

**Files:**
- Modify: `README.md` — Quick Status row `| Dojo (interview prep) | ✅ NeetCode 150 menu, 25'/45' timed sessions, error notebook, spaced redo, AI interviewer |`; layout tree `src/dojo/` (7 files) + `config/dojo/`; Where To Fix What row `| Dojo panel, timed sessions, notebook | src/dojo/, src/app/event_loop/commands_dojo.rs, config/dojo/ | Pure logic in src/dojo, editor wiring in commands_dojo.rs |`.
- Modify: `docs/project-knowledge/lessons.md` — append one entry: Dojo architecture (pure `src/dojo` + `commands_dojo.rs`; fetch reuse via `pending_start` slug match; approach gate via `DojoApproach` palette mode; tick in `about_to_wait`; notebook via `WriteTextFiles`; `rtk grep` false negatives).
- Modify: `DEPENDENCIES.md` — mention `claude` CLI is needed for the Interviewer agent (optional).
- Run: `npx gitnexus analyze`.
- Final: `cargo fmt`, `cargo clippy --all-targets` (no new warnings vs. the baseline of ~230), `cargo test --lib` all green. Report to the user with the GUI checklist:
  1. `g o` → list; `j/k`, `[`/`]`, `r`, Esc.
  2. Enter on a problem → statement + approach prompt; empty Enter refused; Esc keeps THINK; Enter again reopens.
  3. Approach → file opens, Test Runner, chip counts down, phase toasts.
  4. F5 all pass → note prompt → notebook file has the block; row shows ●.
  5. `x` → 3 prompts → row shows redo date; welcome card shows the redo.
  6. SD page → outline + preview + 45' chip; `x` finishes.
  7. `i` → Claude interviewer in the right dock.
- **Leave uncommitted.**

---

## Self-review (done while writing)

- **Spec coverage:** §4 data (T1–T3), §5 panel (T6–T8), §6 DSA flow (T10–T12), §7 SD (T13), §8 SRS (T3), §9 interviewer (T14), §10 chip + welcome (T11, T15), §11 architecture (T7, T9), §12 errors (toasts in T9/T10/T12), §13 tests (each task). **Deviations from spec:** mouse row click/double-click deferred (keyboard-first; noted for follow-up); `PanelTabId::Dojo` reuses the flask icon; SD `x` = finish (records `Pass`).
- **Placeholders:** `dojo_problem_context` body is described, not shown — it is the same cache lookup written out in `dojo_panel_model` (Task 8 Step 4); implement by extracting that lookup into the helper first.
- **Type consistency:** `ActiveSession` fields (`kind, slug, title, started_unix, budget_s, approach, file`) are used identically in T3/T4/T10/T11/T12/T13; `DojoRuntime` gains `statement_cache: Option<(String, String)>` in T8 (add it in T7 to avoid churn).
