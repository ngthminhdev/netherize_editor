fn main() {
    // First statement: the perf probe measures startup from here.
    netherize_editor::app::event_loop::perf_probe::mark_process_start();
    if let Err(e) = netherize_editor::app::event_loop::run() {
        eprintln!("fatal: {e}");
        std::process::exit(1);
    }
}
