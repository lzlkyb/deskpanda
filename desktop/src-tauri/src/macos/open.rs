//! Launch Services through the system executable; never invoke a shell.
//!
//! S6 迁入（2026-11-21）时相对原仓的调整：只保留 `file`（rc 文件面板的
//! 「打开所在文件夹」reveal 桥接，`commands/system.rs` 消费）与 `url`；
//! Markdown 文档打开（`markdown_document` / `open_documents`，依赖
//! `crate::PendingEditor` 与 `commands::open_fullscreen_editor`）是 paste 主产品
//! 能力，未迁。
use std::{path::Path, process::Command};
fn launch(target: &str, reveal: bool) -> Result<(), String> {
    let mut command = Command::new("/usr/bin/open");
    if reveal {
        command.arg("-R");
    }
    let output = command
        .arg(target)
        .output()
        .map_err(|e| format!("打开失败：{e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "打开失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}
fn file_target(path: &Path, reveal: bool) -> Result<String, String> {
    let selected = if reveal && !path.exists() {
        path.parent().ok_or("目标目录不存在")?
    } else {
        path
    };
    let canonical = selected
        .canonicalize()
        .map_err(|e| format!("目标路径不可用：{e}"))?;
    canonical
        .into_os_string()
        .into_string()
        .map_err(|_| "目标路径不是有效 UTF-8".into())
}
pub fn file(path: &Path, reveal: bool) -> Result<(), String> {
    launch(&file_target(path, reveal)?, reveal)
}
pub fn url(url: &str) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://") || url.starts_with("mailto:")) {
        return Err("只允许打开 http/https/mailto 链接".into());
    }
    launch(url, false)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_paths_cannot_be_open_options() {
        let folder = std::env::temp_dir().join(format!("pp-open-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("--args $() ; 文本.txt");
        std::fs::write(&file, b"example").unwrap();
        let target = file_target(&file, false).unwrap();
        assert!(target.starts_with('/'));
        assert!(target.ends_with("--args $() ; 文本.txt"));
        assert!(file_target(&folder.join("missing"), false).is_err());
        assert!(file_target(&folder.join("missing"), true).is_ok());
        assert!(url("file:///tmp/test").is_err());
        assert!(url("-a Terminal").is_err());
        std::fs::remove_dir_all(folder).unwrap();
    }
}
