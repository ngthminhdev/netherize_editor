//! Phase 9b — Terminal Grid Probe.
//!
//! Mục tiêu:
//! 1. Spawn PTY shell.
//! 2. Gửi lệnh có ANSI màu (`printf` với escape codes).
//! 3. Đưa output raw qua `TerminalGrid::feed_chunk()`.
//! 4. Dump grid (text + styles) ra stdout để verify parser + grid đúng.
//!
//! Chạy: `cargo run --bin phase9b_grid_probe`

use std::{
    thread,
    time::{Duration, Instant},
};

use netherize_editor::{
    app::async_bridge::{AppAsyncBridge, AsyncResultRouter},
    async_runtime::{
        message::{
            RequestSpec, RequestTopic, WorkerEvent, WorkerEventKind, WorkerRequestPayload,
            WorkerResult, WorkerResultPayload,
        },
        scheduler::AsyncScheduler,
    },
    terminal::{ansi_parser::AnsiColor, grid::TerminalGrid},
};

const TERMINAL_REVISION: u64 = 1;
const GRID_COLS: usize = 120;
const GRID_ROWS: usize = 30;

// ─── Demo commands ────────────────────────────────────────────────────────────

/// Lệnh test: in text màu xanh, đỏ, reset, và một gradient 256-color.
const DEMO_COMMANDS: &str = concat!(
    // Test 1: standard 8-color
    "printf '\\033[32mGREEN TEXT\\033[0m | \\033[31mRED TEXT\\033[0m | \\033[33mYELLOW\\033[0m\\n'\n",
    // Test 2: bright colors
    "printf '\\033[1;92mBRIGHT GREEN (bold)\\033[0m | \\033[95mMAGENTA\\033[0m\\n'\n",
    // Test 3: 256-color
    "printf '\\033[38;5;202mORANGE-256\\033[0m | \\033[38;5;51mCYAN-256\\033[0m\\n'\n",
    // Test 4: truecolor
    "printf '\\033[38;2;255;128;0mTRUECOLOR ORANGE\\033[0m\\n'\n",
    // Test 5: bg color
    "printf '\\033[42m BG GREEN \\033[0m | \\033[41m BG RED \\033[0m\\n'\n",
    // Exit
    "exit\n",
);

// ─── State ────────────────────────────────────────────────────────────────────

struct GridProbeState {
    session_id: Option<u64>,
    sent_demo: bool,
    saw_close: bool,
    grid: TerminalGrid,
    raw_chunks: Vec<String>,
}

impl GridProbeState {
    fn new() -> Self {
        Self {
            session_id: None,
            sent_demo: false,
            saw_close: false,
            grid: TerminalGrid::new(GRID_COLS, GRID_ROWS),
            raw_chunks: Vec::new(),
        }
    }

    fn submit_demo(&mut self, scheduler: &AsyncScheduler) -> Result<(), String> {
        if self.sent_demo {
            return Ok(());
        }
        let Some(session_id) = self.session_id else {
            return Ok(());
        };

        scheduler.submit(RequestSpec {
            revision_id: TERMINAL_REVISION,
            topic: RequestTopic::TerminalPty,
            payload: WorkerRequestPayload::WritePtyInput {
                session_id,
                input: DEMO_COMMANDS.to_string(),
            },
        })?;
        self.sent_demo = true;
        println!(
            "[Probe] submitted {} demo commands to session={}",
            6, session_id
        );
        Ok(())
    }
}

impl AsyncResultRouter for GridProbeState {
    fn current_revision_for(&self, topic: RequestTopic) -> u64 {
        match topic {
            RequestTopic::TerminalPty => TERMINAL_REVISION,
            _ => 0,
        }
    }

    fn on_worker_event(&mut self, event: WorkerEvent) {
        match &event.kind {
            WorkerEventKind::Failed { error } => {
                eprintln!(
                    "[GridProbe] worker failed request_id={} error={}",
                    event.request_id, error.message
                );
            }
            _ => {} // started/completed — bỏ qua
        }
    }

    fn on_worker_result(&mut self, result: WorkerResult) {
        match result.payload {
            WorkerResultPayload::PtySpawned {
                session_id,
                shell,
                working_dir,
            } => {
                self.session_id = Some(session_id);
                println!(
                    "[GridProbe] PTY spawned session={} shell={} cwd={}",
                    session_id,
                    shell,
                    working_dir.display()
                );
            }
            WorkerResultPayload::PtyOutput {
                session_id: _,
                chunk,
            } => {
                // Feed chunk vào grid
                self.grid.feed_chunk(&chunk);
                self.raw_chunks.push(chunk);
            }
            WorkerResultPayload::PtySessionClosed {
                session_id,
                exit_status,
                reason,
            } => {
                self.saw_close = true;
                println!(
                    "[GridProbe] PTY closed session={} exit={:?} reason={}",
                    session_id, exit_status, reason
                );
            }
            _ => {}
        }
    }

    fn on_stale_result(&mut self, stale: WorkerResult) {
        eprintln!(
            "[GridProbe] stale result discarded request_id={} topic={:?}",
            stale.request_id, stale.topic
        );
    }
}

// ─── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    println!("=== Phase 9b Grid Probe: ANSI Parser + TerminalGrid ===");
    println!("Grid size: {}×{}", GRID_COLS, GRID_ROWS);

    let (scheduler, receiver) = AsyncScheduler::new().expect("init scheduler failed");
    let mut bridge = AppAsyncBridge::new(receiver);
    let mut state = GridProbeState::new();

    // Spawn PTY
    let spawn_req = scheduler
        .submit(RequestSpec {
            revision_id: TERMINAL_REVISION,
            topic: RequestTopic::TerminalPty,
            payload: WorkerRequestPayload::SpawnPtyShell {
                shell: None,
                working_dir: None,
            },
        })
        .expect("spawn PTY request failed");
    println!(
        "[GridProbe] spawn request submitted request_id={}",
        spawn_req.request_id
    );

    // Pump bridge ~8s
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        let stats = bridge.pump(&mut state);
        if let Err(err) = state.submit_demo(&scheduler) {
            eprintln!("[GridProbe] submit demo failed: {err}");
        }
        if state.saw_close {
            break;
        }
        if stats.routed == 0 {
            thread::sleep(Duration::from_millis(20));
        }
    }

    // Drain final events
    let drain_end = Instant::now() + Duration::from_millis(300);
    while Instant::now() < drain_end {
        let stats = bridge.pump(&mut state);
        if stats.routed == 0 {
            thread::sleep(Duration::from_millis(20));
        }
    }

    // ─── Dump kết quả ─────────────────────────────────────────────────────────

    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║            GRID TEXT DUMP (used rows)                   ║");
    println!("╚══════════════════════════════════════════════════════════╝\n");
    println!("{}", state.grid.debug_dump());

    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║           GRID STYLE DUMP (row 0 sample)                ║");
    println!("╚══════════════════════════════════════════════════════════╝\n");

    let used = state.grid.used_rows();
    for row in 0..used {
        let styles = state.grid.debug_row_styles(row);
        let has_color = styles.iter().any(|(_, s)| s.fg != AnsiColor::Default);
        if !has_color {
            continue; // chỉ dump rows có màu để gọn output
        }
        println!("Row {:02}:", row);
        let mut last_fg = AnsiColor::Default;
        let mut run_start = 0;
        let mut run_content = String::new();
        for (i, (ch, style)) in styles.iter().enumerate() {
            if *ch == ' ' && run_content.is_empty() {
                run_start += 1;
                continue;
            }
            if style.fg != last_fg && !run_content.is_empty() {
                println!(
                    "  col {:3}-{:3}: fg={:?}  '{}'",
                    run_start,
                    i - 1,
                    last_fg,
                    run_content.trim_end()
                );
                run_content.clear();
                run_start = i;
            }
            last_fg = style.fg;
            run_content.push(*ch);
        }
        if !run_content.trim().is_empty() {
            println!(
                "  col {:3}-???:  fg={:?}  '{}'",
                run_start,
                last_fg,
                run_content.trim_end()
            );
        }
    }

    // ─── Verify ANSI parsing ───────────────────────────────────────────────────

    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║                  ANSI VERIFY REPORT                    ║");
    println!("╚══════════════════════════════════════════════════════════╝\n");

    verify_ansi_parser();

    println!("\n=== Phase 9b Grid Probe: done ===");
}

/// Chạy một số ANSI parse test trực tiếp và in kết quả.
fn verify_ansi_parser() {
    use netherize_editor::terminal::ansi_parser::{AnsiEvent, AnsiParser};

    struct Case {
        input: &'static str,
        check: &'static str,
    }
    let cases = [
        Case {
            input: "\x1b[32mHI\x1b[0m",
            check: "green fg + reset",
        },
        Case {
            input: "\x1b[38;5;200mIDX200\x1b[0m",
            check: "256-color index 200",
        },
        Case {
            input: "\x1b[38;2;255;128;0mRGB\x1b[0m",
            check: "truecolor 255,128,0",
        },
        Case {
            input: "\x1b[1;92mBOLD BRIGHT GREEN\x1b[0m",
            check: "bold + bright green",
        },
        Case {
            input: "hello\r\nworld",
            check: "CR LF handling",
        },
    ];

    let mut all_pass = true;
    for case in &cases {
        let mut parser = AnsiParser::new();
        let mut events: Vec<AnsiEvent> = Vec::new();
        parser.feed_str(case.input, &mut |ev| events.push(ev));

        let has_color = events.iter().any(|e| {
            matches!(
                e,
                AnsiEvent::SetFg(_)
                    | AnsiEvent::SetBg(_)
                    | AnsiEvent::SetBold(_)
                    | AnsiEvent::Newline
                    | AnsiEvent::CarriageReturn
                    | AnsiEvent::PrintChar(_)
            )
        });

        let status = if has_color { "✓ PASS" } else { "✗ FAIL" };
        if !has_color {
            all_pass = false;
        }
        println!("  {} {:40} events={}", status, case.check, events.len());
        if has_color {
            println!("      first event: {:?}", events.first());
        }
    }

    println!();
    if all_pass {
        println!("  ✓✓✓ All ANSI parse cases passed!");
    } else {
        println!("  ✗ Some ANSI parse cases FAILED — check parser logic.");
    }
}
