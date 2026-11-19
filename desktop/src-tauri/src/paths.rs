//! 用户主目录解析。
//!
//! 收在这里而不是各处再抄一遍：远程文件传输的默认落地目录、文件选择器的起点
//! 都要它。

use std::path::PathBuf;

/// 用户主目录。
///
/// Windows 上是 `USERPROFILE`，回落到 `HOME`（Git Bash / WSL 下才有）。
/// 两个都没有就报错而不是猜一个默认值——猜错的后果是往一个莫名其妙的地方写文件。
pub fn home_dir() -> Result<PathBuf, String> {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .map_err(|_| "无法定位用户主目录（USERPROFILE / HOME 均未设置）".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 主目录能被解析出来() {
        // 两个环境变量都清空的环境不存在（Windows 一定有 USERPROFILE），
        // 所以这里只钉「能拿到一个绝对路径」，不做存在性断言。
        let home = home_dir().expect("主目录必须可解析");
        assert!(home.is_absolute(), "主目录必须是绝对路径：{:?}", home);
    }
}
