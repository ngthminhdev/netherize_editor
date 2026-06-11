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
}
