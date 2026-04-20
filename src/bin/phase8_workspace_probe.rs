use std::path::PathBuf;

use netherize_editor::workspace::model::WorkspaceModel;

fn main() {
    let root_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().expect("get current dir"));

    println!("Phase8 workspace probe root: {}", root_path.display());

    let model = match WorkspaceModel::load(root_path.clone()) {
        Ok(model) => model,
        Err(err) => {
            eprintln!("workspace scan failed: {err}");
            std::process::exit(1);
        }
    };

    println!("=== Workspace Summary ===");
    println!("root_path={}", model.root_path.display());
    println!("nodes={}", model.nodes.len());
    println!(
        "ignore_dirs={}",
        model
            .ignore_rules
            .ignored_directory_names()
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!("=== First Nodes ===");

    for node in model.nodes.iter().take(25) {
        let node_type = match node.file_type {
            netherize_editor::workspace::model::WorkspaceNodeType::File => "file",
            netherize_editor::workspace::model::WorkspaceNodeType::Folder => "folder",
        };
        println!(
            "[{node_type}] path={} modified={:?}",
            node.path.display(),
            node.modified_time
        );
    }
}
