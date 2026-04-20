fn main() {
    if let Err(err) = netherize_editor::app::event_loop::run() {
        eprintln!("phase2_gpu_probe terminated with error: {err}");
    }
}
