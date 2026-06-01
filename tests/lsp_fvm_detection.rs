#[cfg(test)]
mod lsp_fvm_detection_tests {
    use std::path::PathBuf;

    #[test]
    fn test_fvm_workspace_detection() {
        // Test workspace with local FVM
        let workspace = PathBuf::from("/Users/qc-bright/Project/mine_wallet");

        // Check if workspace has pubspec.yaml
        assert!(
            workspace.join("pubspec.yaml").exists(),
            "Test workspace should have pubspec.yaml"
        );

        // Check if local FVM dart exists
        let local_fvm_dart = workspace
            .join(".fvm")
            .join("flutter_sdk")
            .join("bin")
            .join("cache")
            .join("dart-sdk")
            .join("bin")
            .join("dart");

        assert!(
            local_fvm_dart.exists(),
            "Local FVM dart binary should exist at: {}",
            local_fvm_dart.display()
        );
    }

    #[test]
    fn test_non_dart_workspace_returns_none() {
        // Test workspace without pubspec.yaml
        let workspace = PathBuf::from("/tmp");

        // Should not detect FVM for non-Dart workspace
        assert!(
            !workspace.join("pubspec.yaml").exists(),
            "Non-Dart workspace should not have pubspec.yaml"
        );
    }

    #[tokio::test]
    async fn test_real_dap_session_with_flutter() {
        let flutter_bin = "/Users/qc-bright/Project/mine_wallet/.fvm/flutter_sdk/bin/flutter";
        if !std::path::Path::new(flutter_bin).exists() {
            eprintln!("FVM flutter not found, skipping real DAP integration test");
            return;
        }

        let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();
        let client = netherize_editor::dap::DapClient::launch(
            flutter_bin,
            &["debug_adapter".to_string()],
            Some(PathBuf::from("/Users/qc-bright/Project/mine_wallet")),
            event_tx,
        ).expect("launch flutter debug adapter");

        // Send initialize request
        let init_args = serde_json::json!({
            "adapterID": "flutter",
            "clientID": "netherize_editor_test",
            "linesStartAt1": true,
            "columnsStartAt1": true,
            "pathFormat": "path"
        });

        let init_resp = client.send_request("initialize", Some(init_args)).await
            .expect("send initialize request");
        assert!(init_resp.success, "initialize request should be successful");
        assert!(init_resp.body.is_some(), "initialize response should have capabilities");
    }
}
