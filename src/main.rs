fn main() {
    if let Err(e) = netherize_editor::app::event_loop::run() {
        eprintln!("fatal: {e}");
        std::process::exit(1);
    }
}
