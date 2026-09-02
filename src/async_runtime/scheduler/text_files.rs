//! Sequential small text-file writes for `WorkerRequestPayload::WriteTextFiles`.
use std::path::{Path, PathBuf};

use crate::async_runtime::message::TextFileOp;

async fn ensure_parent(path: &Path) -> std::io::Result<()> {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => tokio::fs::create_dir_all(parent).await,
        _ => Ok(()),
    }
}

async fn apply_one(op: TextFileOp) -> Result<(), (PathBuf, String)> {
    let result: std::io::Result<()> = match &op {
        TextFileOp::Write { path, contents } => match ensure_parent(path).await {
            Ok(()) => tokio::fs::write(path, contents).await,
            Err(err) => Err(err),
        },
        TextFileOp::WriteIfMissing { path, contents } => {
            if tokio::fs::try_exists(path).await.unwrap_or(false) {
                Ok(())
            } else {
                match ensure_parent(path).await {
                    Ok(()) => tokio::fs::write(path, contents).await,
                    Err(err) => Err(err),
                }
            }
        }
        TextFileOp::Append {
            path,
            header,
            contents,
        } => {
            let exists = tokio::fs::try_exists(path).await.unwrap_or(false);
            match ensure_parent(path).await {
                Ok(()) => {
                    let mut text = String::new();
                    if !exists {
                        text.push_str(header);
                    }
                    text.push_str(contents);
                    match tokio::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(path)
                        .await
                    {
                        Ok(mut file) => {
                            use tokio::io::AsyncWriteExt;
                            file.write_all(text.as_bytes()).await
                        }
                        Err(err) => Err(err),
                    }
                }
                Err(err) => Err(err),
            }
        }
        TextFileOp::Remove { path } => match tokio::fs::remove_file(path).await {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err),
        },
    };
    result.map_err(|err| (op_path(&op).to_path_buf(), err.to_string()))
}

fn op_path(op: &TextFileOp) -> &Path {
    match op {
        TextFileOp::Write { path, .. }
        | TextFileOp::Append { path, .. }
        | TextFileOp::WriteIfMissing { path, .. }
        | TextFileOp::Remove { path } => path,
    }
}

/// Apply `ops` in order; returns one (path, error) per failed op.
pub async fn apply_text_ops(ops: Vec<TextFileOp>) -> Vec<(PathBuf, String)> {
    let mut failures = Vec::new();
    for op in ops {
        if let Err(failure) = apply_one(op).await {
            failures.push(failure);
        }
    }
    failures
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ops_apply_in_order_and_report_failures() {
        let rt = tokio::runtime::Runtime::new().expect("rt");
        let dir = std::env::temp_dir().join(format!("dojo_ops_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let nb = dir.join("notes/interview-notes.md");
        let failures = rt.block_on(apply_text_ops(vec![
            TextFileOp::Append {
                path: nb.clone(),
                header: "# H\n\n".into(),
                contents: "a\n".into(),
            },
            TextFileOp::Append {
                path: nb.clone(),
                header: "# H\n\n".into(),
                contents: "b\n".into(),
            },
            TextFileOp::WriteIfMissing {
                path: dir.join("x.md"),
                contents: "1".into(),
            },
            TextFileOp::WriteIfMissing {
                path: dir.join("x.md"),
                contents: "2".into(),
            },
            TextFileOp::Write {
                path: dir.join("cur.md"),
                contents: "c".into(),
            },
            TextFileOp::Remove {
                path: dir.join("cur.md"),
            },
            TextFileOp::Remove {
                path: dir.join("missing.md"),
            },
            TextFileOp::Write {
                path: dir.join("x.md").join("child"),
                contents: "boom".into(),
            },
        ]));
        assert_eq!(std::fs::read_to_string(&nb).expect("nb"), "# H\n\na\nb\n");
        assert_eq!(std::fs::read_to_string(dir.join("x.md")).expect("x"), "1");
        assert!(!dir.join("cur.md").exists());
        assert_eq!(
            failures.len(),
            1,
            "only the impossible path fails; missing remove is fine"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
