# Dojo — Interview-Prep Roadmap Inside Netherize + AI Interviewer

Date: 2026-09-02
Status: Draft, awaiting user review
Source roadmap: `interview-prep-roadmap.md` (repo root, untracked)

## 1. Goal

Turn the interview roadmap's problem menu into an in-editor practice loop. The
roadmap's week order is only a **browsable grouping**, never a calendar: the
user opens Dojo whenever they feel like coding and picks any problem. What the
app enforces is confined to a single problem session, so the three habits the
roadmap demands but the user tends to skip become automatic:

1. **Timebox** every DSA problem at 25 minutes (3' think / 15' code / 5' test / 2' review).
2. **Error notebook** entry after every session (3 lines: where stuck, right pattern, tell-tale sign).
3. **Spaced redo** of failed problems (+3 days, then +14 days).

Plus a motivation layer: per-group and overall progress, streak, "next problem"
one keypress away, and a Welcome-screen card that *suggests* (never assigns)
what to do next.

Plus an **AI Interviewer**: a Claude Code agent preset that forces the user to
state approach + complexity before coding and grades at the end.

## 2. Non-goals

- Teaching or solving. The app never shows a solution.
- Go practice, behavioral STAR stories, CV work, weekly pacing — not modeled;
  they stay in `interview-prep-roadmap.md`.
- System-design drawing. NetherCanvas is code-only; SD sessions get a markdown
  outline + timer, not boxes.
- A "type Heap from memory" drill. The user fetches Kth Largest (#215) instead.
- LeetCode account sync, submissions, or any network beyond the existing fetch.

## 3. Decisions locked with the user

| # | Question | Decision |
|---|---|---|
| a | Key to open Dojo | `g o` (Normal mode chord; `g d/r/p/e/v/i/g/c` already taken) + palette `Dojo: Open` |
| b | THINK phase gate | **Hard**: the solution file does not open until the user has typed an approach line. Closing the prompt (Esc) keeps the session in THINK with the file locked; `x` in the Dojo panel gives up. |
| c | At 25:00 | **Auto-stop**: outcome `timeout`, notebook prompts open immediately, problem marked redo (+3d). No overtime. |
| — | Source of truth | Machine state in `~/.config/netherize/dojo.toml`. Human notebook is markdown the user reads on Sundays. Plan + problem list are git-tracked TOML in `config/dojo/`. |
| — | Calendar | **None.** No start date, no "current week", nothing expires or unlocks. Groups are pages the user browses; the only clock is the 25'/45' session timer. |

## 4. Data

### 4.1 `config/dojo/neetcode150.toml` (shipped, git-tracked)

```toml
[[problem]]
id = 1
slug = "two-sum"
title = "Two Sum"
category = "arrays_hashing"
difficulty = "easy"
```

Category keys (NeetCode 150 groups): `arrays_hashing`, `two_pointers`,
`sliding_window`, `stack`, `binary_search`, `linked_list`, `trees`, `tries`,
`heap`, `backtracking`, `graphs`, `advanced_graphs`, `dp_1d`, `dp_2d`,
`greedy`, `intervals`, `math_geometry`, `bit_manipulation`.

The list is populated at implementation time from a public NeetCode 150 index
and every slug is checked against the existing fetch resolver
(`normalize_problem_input` + GraphQL) so `Enter` on any row fetches. Loaded via
`include_str!` fallback so a missing file never breaks the app; a user-edited
copy in `~/.config/netherize/dojo/neetcode150.toml` wins if present.

### 4.2 `config/dojo/plan.toml` (shipped, git-tracked, user-editable)

```toml
notebook = "~/Work/docs/interview-notes.md"
sd_dir    = "~/Work/docs/sd"
dsa_minutes = 25
dsa_phases  = [["THINK", 3], ["CODE", 15], ["TEST", 5], ["REVIEW", 2]]
sd_minutes  = 45

# DSA groups, in the roadmap's suggested order. Pages, not weeks.
[[group]]
key = "hash_two_pointers"
label = "Array/Hash Map, Two Pointers"
categories = ["arrays_hashing", "two_pointers"]
note = "Đổi O(n²)→O(n) bằng hash; template two pointer."

# same shape:
# sliding_stack      sliding_window + stack           "Window co giãn; monotonic stack"
# bsearch_list       binary_search + linked_list      "lo/hi không off-by-one; fast/slow"
# tree_trie          trees + tries                    "Đệ quy trên cây; BFS theo tầng"
# heap_bt_interval   heap + backtracking + intervals  "Top-K heap; cắt nhánh; merge interval"
# graph              graphs + advanced_graphs         "Topo sort; detect cycle; DSU"
# extra              dp_1d + dp_2d + greedy + math_geometry + bit_manipulation

# System-design cases, any order, any time.
[[sd_case]]
key = "wallet_v1"
label = "Ví điện tử / chuyển tiền (bản 1)"
topic = "Scaling: LB, replication, sharding, CAP"
# url_shortener · rate_limiter · leaderboard · feed · chat_notification
# log_metrics · wallet_v2 ("so với bản 1")
```

Group sizes come from the category sizes (14, 13, 18, 18, 22, 19, Extra 46 =
150). The user prunes or reorders by editing this file. Every problem belongs to
exactly one group; a problem whose category matches no group lands in the last
group. The roadmap's per-week schedule, Go, STAR and CV items stay in
`interview-prep-roadmap.md`; the app does not model them.

### 4.3 `~/.config/netherize/dojo.toml` (machine state, atomic write)

```toml
last_group = "sliding_stack"     # page the user was browsing; [ / ] update it

[active_session]                 # absent when idle; survives restart
kind = "dsa"                     # or "sd"
slug = "two-sum"                 # or sd case label
started_unix = 1788400000
approach = "hash map, one pass, O(n)"   # absent until typed (gate)
file = "/path/to/solution.js"

[[attempt]]
slug = "two-sum"
started_unix = 1788400000
ended_unix = 1788401100
outcome = "pass"                 # pass | fail | timeout | giveup
elapsed_s = 1100
approach = "hash map, one pass, O(n)"

[problem.two-sum]
status = "done"                  # todo | redo | done
redo_at = "2026-09-05"           # only while status = redo
passes = 1
```

Derived, never stored:
- page order for `[`/`]` = groups in file order, then the SD page; no wrap.
  On first open with no `last_group`, the page is the first group that still
  has a `todo` problem.
- streak = consecutive local calendar days with ≥1 attempt, counted back from
  today if today has an attempt, otherwise from yesterday (so a streak is not
  "broken" before the day's session).
- progress = `done` count / size, per group and overall (x/150).
- redo-due list = `status = redo && redo_at <= today`, across all groups.
- suggested next = first redo-due problem, else first `todo` in the current
  page, else first `todo` anywhere. Shown on the Welcome card and pre-selected
  in the list; never auto-started.

### 4.4 Notebook (`notebook` path, append-only markdown)

Created with `# Sổ tay lỗi` if missing. One block per finished session:

```markdown
## 2026-09-02 · #3 Longest Substring Without Repeating Characters · timeout 25:00 · #redo
- Hướng: sliding window + set, O(n)
- Bí: quên co cửa sổ khi gặp trùng, lệch index
- Pattern: window co giãn, map char→last index
- Dấu hiệu: "longest/shortest substring thoả điều kiện" → sliding window
```

`pass` on first try writes the header plus one optional `- Ghi chú:` line.
SD sessions write `## <date> · SD · <case> · 45:00` + one note line.

## 5. Dojo panel

New `PanelTabId::Dojo` in the right dock: `[AiChat, TestRunner, Outline, Dojo]`.
Rendered in the existing `test_runner` surface below the tab strip, exactly like
Outline (no new pipeline). Two views.

### 5.1 List view (idle)

```
 DOJO · Sliding Window, Stack (2/7)      23/150 · streak 5
 ████████░░░░░░  5/13                     redo tới hạn: 2
 ─────────────────────────────────────────────────────
 ↻   3  Longest Substring Without Repeating   redo hôm nay
 ●  121  Best Time to Buy and Sell Stock       pass 14:20
 ○  424  Longest Repeating Character Replace
 …
 [Enter] bắt đầu  [r] chỉ redo  [ ] ] nhóm  [i] interviewer  [Esc] editor
```

- Rows: redo-due first (from every group, so they are never missed), then
  this group's todo, then done. Glyph `↻` redo-due, `·` redo not yet due
  (dimmed, shows date), `○` todo, `●` done (shows best time).
- The SD page lists the cases with the topic as a dimmed second column.
- Keys (Normal-style, no modifiers): `j/k/↑/↓` select, `Enter` start,
  `r` toggle redo-only filter (all groups), `[` / `]` previous/next page
  (writes `last_group`), `i` launch AI Interviewer for the selected problem,
  `x` give up (only when a session is active), `Esc` focus editor.
  `Cmd` shortcuts stay global (existing rule).

### 5.2 Session view (active session)

```
 #3 Longest Substring Without Repeating Characters    THINK 02:14
 ─────────────────────────────────────────────────────
 Given a string s, find the length of the longest substring
 without repeating characters. …          (statement, HTML stripped)
 …
 Hướng làm: (chưa có)  [Enter] nhập   [x] bỏ phiên   [i] interviewer
```

- Statement = cached `statement` with tags stripped and entities decoded
  (extend `extract_example_outputs`'s tag regex into a shared helper in
  `leetcode_api.rs`), word-wrapped to the panel width; `j/k` scroll.
- `Enter` reopens the approach prompt while approach is empty; after the
  approach is set, `Enter` switches to the Test Runner tab.
- Timer and phase are always visible in the header and in the statusbar chip.

## 6. DSA session flow

```
Enter on row
  → DojoStartProblem{slug}
  → existing LeetCodeFetchJob (+ new field dojo: bool)
  → LeetCodeProblemFetched{…}
      · file written to disk (existing), test runner filled (existing)
      · if dojo: DO NOT open the file. Create active_session{started_unix=now},
        write current.md (see §9), switch right dock to Dojo session view,
        open approach prompt overlay.
Approach prompt (prompt overlay, kind DojoApproach)
  · Enter with ≥1 non-blank char → save approach, open solution file (the
    test runner was already filled at fetch), switch right dock to TestRunner,
    focus editor.
  · Esc → close prompt only; session stays in THINK, file locked; panel shows
    "[Enter] nhập". Timer keeps running.
Timer
  · phase_at(elapsed) over dsa_phases; phase change → toast "CODE · 15 phút"
    and chip color change. Timer starts at fetch result, not at approach confirm
    (reading the statement is part of THINK).
End of session (first of):
  · TestCasesCompleted with every case Passed while a session is active → pass
  · elapsed ≥ dsa_minutes*60 → timeout (auto-stop, decision c)
  · x in Dojo panel → giveup
  · TestCasesCompleted with failures does NOT end the session (user keeps coding)
After end
  · record attempt, apply SRS (§8), clear active_session, save state
  · notebook prompts: pass → one "Ghi chú (Enter bỏ qua)"; else three chained
    prompts "Bí ở đâu" → "Pattern đúng" → "Dấu hiệu nhận biết lần sau". Esc at
    any step writes what was collected so far (a header line is always written).
  · toast summary: "Two Sum · pass 12:40 · streak 6" / "timeout · redo 05/09"
  · right dock returns to Dojo list view with the row updated
Restart while active_session exists
  · elapsed = now - started_unix. If ≥ budget → treat as timeout immediately on
    first Dojo open (prompts appear then). Else resume: reopen the solution file
    if approach exists, chip shows remaining time.
```

Fetch failure (network, unknown slug) → existing failure toast, no session
created, row unchanged.

## 7. SD session flow (SD page, any time)

Enter on a case row → create `<sd_dir>/<key>.md` from the template
below (skip if exists), open it, open Markdown Preview in the right dock, start a
45-minute single-phase timer. Ends on `x` or timeout (auto-stop). One notebook
prompt after end. No approach gate.

```markdown
# <case> — <date>
## 1. Làm rõ yêu cầu (5')
## 2. Ước lượng quy mô (5')
## 3. API + mô hình dữ liệu (5')
## 4. Kiến trúc mức cao (10')
## 5. Đào sâu 1–2 điểm (15')
## 6. Nút cổ chai + đánh đổi (5')
> Câu hỏi bắt buộc: request này chết giữa chừng thì sao?
```

## 8. Spaced repetition rules (pure, in `dojo/state.rs`)

| Before | Outcome | After |
|---|---|---|
| todo | pass | done, passes=1 |
| todo | fail/timeout/giveup | redo, redo_at = today+3 |
| redo | pass | passes+1; passes ≥ 2 → done, else redo, redo_at = today+14 |
| redo | fail/timeout/giveup | redo, redo_at = today+3 |
| done | any | done (re-doing a done problem records the attempt only) |

`fail` is produced only by give-up after at least one F5 with failures; plain
`x` before any run is `giveup`. Both map identically; the distinction is kept for
the notebook header.

## 9. AI Interviewer

- New entry in `src/app/ai_agents.rs`:
  `AiAgent { id: "interviewer", label: "Interviewer (claude)", command: "claude --append-system-prompt \"$(cat ~/.config/netherize/dojo/interviewer.md)\"" }`.
  Runs in the existing right-dock login-shell PTY, so `$(cat …)` resolves there.
- `~/.config/netherize/dojo/interviewer.md` is written on first Dojo open if
  absent (user-editable, never overwritten). Content: interviewer persona —
  read `~/.config/netherize/dojo/current.md` first; ask for approach and
  time/space complexity before any code; never give the solution; hints only
  when explicitly asked, one at a time; when the candidate says "done", grade
  correctness / complexity / communication out of 5 each and name the pattern.
  For SD (`kind = sd` in current.md) follow the 45' framework and push on
  "what if this request dies halfway".
- `current.md` is rewritten at every session start: kind, id, title, plain
  statement, constraints, language, phase budget, and the approach line once
  typed. Removed at session end.
- `i` in the Dojo panel = write current.md for the selected problem (even
  without a session) then launch the interviewer via the existing
  `spawn_right_agent_terminal` (`commands_ai_agent.rs`). Since AiChat and Dojo share the right dock, the
  dock switches to AiChat; `g o` returns to Dojo.

## 10. Statusbar chip and Welcome card

- Statusbar right zone, left of the LSP chip: `⏱ CODE 11:42`. Color by phase
  (THINK info, CODE accent, TEST warning, REVIEW magenta); the last 60 seconds
  render with `error` color. Absent when idle.
- Tick: while a session is active, `about_to_wait` arms a deadline at the next
  whole second (same pattern as `whichkey_deadline`) and requests a redraw;
  phase transitions and timeout are detected there.
- Welcome screen: fourth action card, section `DOJO`, key chips `g` `o`.
  Title = suggested next (`↻ 2 redo tới hạn · #3 Longest Substring` or
  `○ #424 Longest Repeating Character Replacement`), subtitle =
  `Sliding Window, Stack 5/13 · 23/150 · streak 5`. Fresh install:
  `○ #1 Two Sum` / `0/150`. Reads the same derived values as the panel header.

## 11. Architecture

New module `src/dojo/` (pure, no UI, no I/O except explicit load/save):

| File | Responsibility |
|---|---|
| `plan.rs` | Parse `plan.toml`; `Plan { groups, sd_cases, notebook, sd_dir, dsa_phases, … }`; `Group::problems(&Problems)`; `pages()` in `[`/`]` order |
| `problems.rs` | Parse `neetcode150.toml`; lookup by slug/id; `~/.config` override precedence |
| `state.rs` | `DojoState` load/save (atomic via existing `app::persistence::atomic_write`); SRS transition fn; streak; progress; redo-due |
| `session.rs` | `Session { kind, slug, started_unix, approach, file }`, `phase_at(elapsed_s) -> (Phase, remaining_s)`, `is_expired` — takes `now` as a parameter |
| `notebook.rs` | Format markdown blocks; `html_to_text` shared with the panel |
| `current_md.rs` | Format `current.md` and the default `interviewer.md` |

Editor integration (follows the golden data flow):

- `Command::Dojo*` variants in `core/commands.rs` + ids in `command_ids.rs`
  (`dojo.open`, `dojo.select_next/prev`, `dojo.start`, `dojo.toggle_redo`,
  `dojo.page_next/prev`, `dojo.interviewer`, `dojo.give_up`,
  `dojo.scroll_down/up`). Shell-handled passthrough in `command_dispatch`
  (same as `TestRunner*`), real handlers in a new
  `app/event_loop/commands_dojo.rs`.
- Keymap: `g o` → `dojo.open` in `config/keymaps/default.toml`; palette entry
  `Dojo: Open`.
- `InputFocusContext::Dojo` (right dock visible, active tab Dojo). Hooked in
  BOTH `route_normalized_input` and `route_repeated_normalized_input`
  (`docs/project-knowledge/lessons.md` #113) plus the IME-commit path.
- `AppShell` gains `dojo: DojoRuntime { plan, problems, state, page, selected, redo_only, scroll, view }`.
  Loaded in `setup.rs` on a worker (file I/O), applied on result.
- Prompt overlays: new text-input palette modes `CommandPaletteMode::DojoApproach`
  and `CommandPaletteMode::DojoNote { step: u8 }`, modelled on
  `LeetCodeProblemInput` (prompt/hint/title arms in `command_palette.rs`,
  empty-items arm in `refresh_results`, paste allow-list in
  `command_dispatch/editing.rs`), opened through the existing
  `open_prompt_overlay(mode)` in `commands_prompts.rs`; confirm branches in
  `commands_palette.rs` delegate to `commands_dojo.rs`.
- Fetch: `LeetCodeFetchJob` + `WorkerResultPayload::LeetCodeProblemFetched`
  gain `dojo: bool`; `handle_leetcode_fetch_result` branches on it (skip
  open-file, create session, open approach prompt).
- Run results: `handle_test_cases_completed` (async_results/runner.rs) calls
  `dojo.on_run_completed(all_passed)`.
- I/O off the UI thread: notebook append, `current.md` / `interviewer.md`
  writes, SD template write, and `dojo.toml` saves go through a new
  `WorkerRequestPayload::DojoWriteFiles { ops }` (fire-and-forget with a
  failure toast). State saves are debounced like window geometry.
- Renderer: `build_dojo_content` in `render/renderer/ui/test_runner.rs`
  (list + session views), called from the right-dock render block in
  `application.rs` when the active tab is Dojo. Mouse: row click selects,
  double-click starts (reuse `right_dock_tab_index_at`-style hit test).

## 12. Error handling

- Missing/invalid `plan.toml` or problem list → embedded defaults + one toast.
- Notebook path not writable → toast with the path; the attempt is still
  recorded in `dojo.toml` so nothing is lost.
- Fetch failure → no session (§6).
- `claude` not on PATH → the PTY shows "command not found" like every other
  agent; nothing else to do.
- No `.unwrap()`/`.expect()` outside tests; all parsing returns
  `anyhow::Result`.

## 13. Testing

Unit (no GUI):
- plan parse, page order, first-open page = first group with a todo, every
  problem lands in exactly one group, unmatched category → last group.
- SRS table (§8) exhaustively; streak across day boundaries and gaps; progress
  per group and overall; suggested-next precedence (redo-due → page todo →
  any todo → none).
- `phase_at` boundaries (0, 179, 180, 1499, 1500) and `is_expired`.
- notebook block formatting for pass/timeout/partial (Esc mid-way); html→text.
- `current.md` content; interviewer default is non-empty and mentions
  current.md.
- Keybinding routing: `g o` resolves to `dojo.open`; Dojo focus context keys map
  to the right commands in both routing paths.
- Fetch result handler with `dojo = true` does not open the file and creates a
  session (state-level test on AppState/DojoRuntime where feasible).

Runtime: `cargo test`, `cargo clippy` no new warnings, `cargo run` GUI check by
the user (fetch a real problem, run the 25' loop with a shortened
`dsa_phases` in a scratch plan.toml).

## 14. Follow-ups (explicitly deferred)

- Optional 14-week calendar overlay (start date, "this week's group") if the
  user later wants pacing. The data model above does not preclude it.
- Implementation deviations (2026-09-02): mouse row click/double-click not
  wired (keyboard only); the tab reuses the flask icon; SD sessions do NOT
  auto-open the markdown preview (`FocusMarkdownPreview` replaces the center
  buffer with a preview and hides the outline file) — the user toggles it.
- Statistics page (avg time per pattern, weakest category).
- Cloud/mobile sync of dojo.toml.

## 15. v2 rework (2026-09-02, after the first GUI round)

The user could not use v1 ("chưa dùng được cái gì cả"): the solution file was dropped into the open project, the language was picked silently, the list was in the wrong dock with no difficulty and an opaque order, and the statement only showed after Enter plus a text prompt. v2 supersedes §5–§7 and §10 where they conflict:

- **LeetCode workspace.** A user-chosen folder (system dialog; Cmd+P "Dojo: Choose Folder" or `w`), stored in `dojo.toml` as `workspace`. `g o` / "Dojo: Open (LeetCode workspace)" switches the editor to that folder (dirty-buffer guard applies) and shows the panels. Each problem lives in `<workspace>/0001-two-sum/solution.<ext>`; `notes.md` and `sd/` live there too (plan.toml `notebook`/`sd_dir` remain as overrides). The Explorer of that workspace therefore lists exactly the attempted problems.
- **Left dock: Dojo tree.** 18 NeetCode categories in file order, collapsible (`h`/`l`/←/→, Enter or click on a header), each row `E/M/H` + status glyph (`○` todo, `●` solved + best time, `↻` redo due, `·` redo date) + `id. title`, plus a System Design group. `r` = only due redos. Collapsed set persists.
- **Right dock: Problem tab.** Title + difficulty, status line, category, language, then the statement, example cases and (on `?`) LeetCode's hints — fetched on selection with a 300 ms debounce through a statement-only worker job that refreshes the per-problem cache; no file is written until Enter. While a session runs the tab keeps the session's problem and shows the phase clock.
- **Enter = start.** No approach prompt. Missing language → picker (Cmd+P "Dojo: Language" / `c`, remembered), then fetch into the problem folder, open the file, load the example cases, start the 25' clock. An existing `solution.<ext>` is reopened instead of re-fetched.
- **Session end.** F5 all green / `x` / timeout → attempt recorded, spaced redo applied, one notebook stub appended (`- Note:` for a pass, `- Stuck at / Right pattern / Signal next time` otherwise). No prompts; `n` opens the notebook.
- **UI language:** English everywhere (toasts, footers, notebook, SD template, case labels).
- Removed: pages/groups in plan.toml, `DojoApproach`/`DojoNote` palette modes, `approach` on sessions/attempts, `[`/`]` paging.
