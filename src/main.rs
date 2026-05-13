fn main() {
    let startup_config =
        netherize_editor::app::event_loop::StartupConfig::from_args(std::env::args().skip(1));

    if let Err(e) = netherize_editor::app::event_loop::run(startup_config) {
        eprintln!("fatal: {e}");
        std::process::exit(1);
    }
}
