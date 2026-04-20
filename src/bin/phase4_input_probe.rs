use std::{path::PathBuf, sync::Arc};

use netherize_editor::{
    app::{
        app_state::AppState,
        input::{InputHandler, InputRouteOutcome},
        input_map::{InputMap, KeybindingContext},
    },
    core::command_dispatch::dispatch_command,
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{Ime, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

const WINDOW_TITLE: &str = "Netherize Phase4 - Input Probe";
const WINDOW_WIDTH: u32 = 900;
const WINDOW_HEIGHT: u32 = 600;

const DEFAULT_SAVE_PATH: &str = "phase4_scratch.txt";
const DEFAULT_OPEN_PATH: &str = "phase4_open_sample.txt";
const DEFAULT_OPEN_CONTENT: &str =
    "Hello from phase4 open sample.\nYou can type, save (Cmd/Ctrl+S), and open (Cmd/Ctrl+O).\n";

struct Phase4InputProbeApp {
    window: Option<Arc<Window>>,
    input_handler: InputHandler,
    input_map: InputMap,
    app_state: AppState,
}

impl Phase4InputProbeApp {
    fn new() -> Self {
        let save_path = PathBuf::from(DEFAULT_SAVE_PATH);
        let open_path = PathBuf::from(DEFAULT_OPEN_PATH);

        if let Err(err) = AppState::ensure_probe_file(open_path.as_path(), DEFAULT_OPEN_CONTENT) {
            eprintln!("{err}");
        }

        Self {
            window: None,
            input_handler: InputHandler::new(),
            input_map: InputMap::new(open_path),
            app_state: {
                let mut state = AppState::new(save_path);
                if let Ok(cwd) = std::env::current_dir() {
                    if let Err(err) = state.attach_workspace(cwd.clone()) {
                        eprintln!("workspace attach failed for {}: {err}", cwd.display());
                    }
                }
                state
            },
        }
    }

    fn refresh_window_title(&self) {
        let Some(window) = &self.window else {
            return;
        };

        let (line, col) = self.app_state.cursor_line_col();
        let dirty_mark = if self.app_state.is_dirty() { "*" } else { "" };
        window.set_title(&format!(
            "{WINDOW_TITLE}{dirty_mark} | Ln {}, Col {}",
            line + 1,
            col + 1
        ));
    }
}

impl ApplicationHandler for Phase4InputProbeApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title(WINDOW_TITLE)
            .with_inner_size(PhysicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT));

        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(err) => {
                eprintln!("create_window failed: {err}");
                event_loop.exit();
                return;
            }
        };

        self.window = Some(window);
        self.refresh_window_title();

        println!("Phase 4 ready: event -> command translation -> dispatcher.");
        println!("Controls:");
        println!("  - Type any character: InsertChar");
        println!("  - Backspace / Arrow keys: navigation + edit commands");
        println!("  - Cmd/Ctrl+S: SaveFile -> {}", DEFAULT_SAVE_PATH);
        println!(
            "  - Cmd/Ctrl+O: OpenFile -> {}",
            self.input_map.open_file_path().display()
        );
        println!("State: {}", self.app_state.debug_state_line());
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self
            .window
            .as_ref()
            .is_some_and(|window| window.id() != window_id)
        {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.input_handler.update_modifiers(modifiers);
            }
            WindowEvent::Focused(focused) => {
                self.input_handler.on_focus_changed(focused);
            }
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                // Event loop chỉ translate + forward command.
                // Không mutate editor state trực tiếp ở đây.
                if let Some(route) = self.input_handler.translate_key_event(
                    &key_event,
                    &self.input_map,
                    KeybindingContext::for_mode_with_picker(
                        self.app_state.current_mode(),
                        self.app_state.is_file_picker_open(),
                    ),
                ) {
                    match route {
                        InputRouteOutcome::Dispatch(translated) => {
                            println!("Input: {}", translated.input_debug);
                            println!("Route: {}", translated.route_debug);
                            println!("Command: {:?}", translated.command);

                            let report = dispatch_command(&mut self.app_state, translated.command);
                            println!("{}", report.message);
                            println!("State: {}", self.app_state.debug_state_line());

                            self.refresh_window_title();
                            if report.request_redraw {
                                if let Some(window) = &self.window {
                                    window.request_redraw();
                                }
                            }
                        }
                        InputRouteOutcome::NoDispatch {
                            input_debug,
                            route_debug,
                        } => {
                            println!("Input: {}", input_debug);
                            println!("Route: {}", route_debug);
                        }
                    }
                }
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                if let Some(translated) = self.input_handler.translate_ime_commit(
                    &text,
                    KeybindingContext::for_mode_with_picker(
                        self.app_state.current_mode(),
                        self.app_state.is_file_picker_open(),
                    ),
                ) {
                    println!("Input: {}", translated.input_debug);
                    println!("Route: {}", translated.route_debug);
                    println!("Command: {:?}", translated.command);

                    let report = dispatch_command(&mut self.app_state, translated.command);
                    println!("{}", report.message);
                    println!("State: {}", self.app_state.debug_state_line());

                    self.refresh_window_title();
                    if report.request_redraw {
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                // Phase 4 chỉ probe input pipeline nên chưa render vào GPU.
            }
            _ => {}
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = Phase4InputProbeApp::new();
    if let Err(err) = event_loop.run_app(&mut app) {
        eprintln!("phase4_input_probe exited with error: {err}");
    }
}
