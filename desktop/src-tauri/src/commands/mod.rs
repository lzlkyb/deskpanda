//! 前端 → 后端的 Tauri IPC 命令层。**本文件只做聚合，不含任何实现。**
//!
//! 组织范式（承自生产项目，可直接照用）：
//! - 每个关注点一个子模块 `xxx.rs` 或 `xxx_cmds.rs`（后者用于与 crate 根下同名
//!   实现模块冲突时，如 `backup` vs `backup_cmds`）。
//! - 本文件 `mod` + `pub use` 重导出，`main.rs` 的 `invoke_handler![...]` 列
//!   `commands::xxx` 即可——拆分命令层是内部结构调整，调用方零改动。
//! - 新增命令：在子模块写 `#[tauri::command]` → 此处 `mod` + `pub use` →
//!   `main.rs` 的 `invoke_handler!` 加一行。三处，缺一不可。

mod system;
mod update;

pub use system::*;
pub use update::*;

// 远程电脑命令层。**Windows 专属**：`rc::` 本身按 `cfg(windows)` 门控
// （采集/编码/音频/输入注入全是 Win32 绑定），命令层跟着一起门控。
// 三个文件按职责分开（不是按规模任意切）：
// - `rc`：配对（邀请码 + 局域网）、设备表、会话、输入、设置
// - `rc_file`：文件传输
// - `rc_pair`：局域网附近设备
#[cfg(windows)]
mod rc;
#[cfg(windows)]
mod rc_file;
#[cfg(windows)]
mod rc_pair;

#[cfg(windows)]
pub use rc::*;
#[cfg(windows)]
pub use rc_file::*;
#[cfg(windows)]
pub use rc_pair::*;

#[cfg(all(test, windows))]
mod registry_guard {
    //! 「实现了但没注册」是远程电脑最容易犯的错：命令写好了、前端 invoke 了，
    //! 界面上就是**点了没反应**（`Command xxx not found` 只在 devtools 里）。
    //! 单条命令的接线守卫在 `rc/tests/guards.rs`，这里补一条**穷举式**的：
    //! 三个命令文件里的每个 `#[tauri::command]` 都必须出现在 `main.rs` 的注册列表里。

    /// 抠出 `main.rs` 里所有 `tauri::generate_handler![…]` 的命令名。
    fn handler_lists() -> Vec<Vec<String>> {
        let src = include_str!("../main.rs");
        let mut out = Vec::new();
        let mut rest = src;
        const MARK: &str = "generate_handler![";
        while let Some(at) = rest.find(MARK) {
            rest = &rest[at + MARK.len()..];
            let end = rest.find(']').expect("generate_handler! 没有闭合");
            let block = &rest[..end];
            rest = &rest[end..];
            out.push(
                block
                    .split(',')
                    .filter_map(|item| {
                        // 每个分组前面有一行 `// —— 分组名 ——` 注释，它和第一条命令
                        // 落在同一个逗号段里 ⇒ 不能直接 trim 后 strip_prefix，
                        // 必须找出段内的 `commands::` 再取标识符（否则每组第一条命令
                        // 都会被当成「没注册」，这条守卫就会假红）。
                        let at = item.find("commands::")?;
                        item[at + "commands::".len()..]
                            .chars()
                            .take_while(|c| c.is_alphanumeric() || *c == '_')
                            .collect::<String>()
                            .into()
                    })
                    .collect(),
            );
        }
        out
    }

    /// 抠出命令文件里声明了 `#[tauri::command]` 的函数名。
    ///
    /// ❗ 只认**行首**的标记：这三个文件的模块注释里会引用 `#[tauri::command]`
    /// 讲接线纪律（如 `commands/rc_file.rs` 的头部），那种出现的下一处 `fn `
    /// 往往是注释里的示例，抓进来就是假阳性。
    fn declared(file: &str, src: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = src;
        const MARK: &str = "#[tauri::command]";
        while let Some(at) = rest.find(MARK) {
            let line_start = rest[..at].rfind('\n').map_or(0, |i| i + 1);
            let at_line_start = rest[line_start..at].trim().is_empty();
            rest = &rest[at + MARK.len()..];
            if !at_line_start {
                continue;
            }
            // `#[tauri::command]` 与 `pub (async) fn name` 之间可能隔着 `#[cfg]` 等属性
            let Some(fn_at) = rest.find("fn ") else { break };
            let after = &rest[fn_at + 3..];
            let name: String = after
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            assert!(!name.is_empty(), "{file}: #[tauri::command] 后面没跟函数名");
            out.push(name);
            rest = after;
        }
        out
    }

    #[test]
    fn 每个命令都必须进_invoke_handler() {
        let lists = handler_lists();
        assert_eq!(
            lists.len(),
            2,
            "main.rs 应当有 Windows / 非 Windows 两段注册（见段首注释）"
        );
        // 第 0 段是 `#[cfg(windows)]` 那段（源码里先出现）
        let registered: std::collections::HashSet<&str> =
            lists[0].iter().map(String::as_str).collect();

        let files = [
            ("commands/rc.rs", include_str!("rc.rs")),
            ("commands/rc_file.rs", include_str!("rc_file.rs")),
            ("commands/rc_pair.rs", include_str!("rc_pair.rs")),
        ];
        let mut missing = Vec::new();
        let mut count = 0;
        for (name, src) in files {
            for cmd in declared(name, src) {
                count += 1;
                if !registered.contains(cmd.as_str()) {
                    missing.push(format!("{name}::{cmd}"));
                }
            }
        }
        assert!(
            count > 70,
            "只解析到 {count} 条命令，解析器或文件已经不对了"
        );
        assert!(
            missing.is_empty(),
            "这些命令没进 invoke_handler —— 前端点了会报 command not found：{missing:?}"
        );
    }

    #[test]
    fn 非_windows_那段不许出现只在_windows_存在的命令() {
        let lists = handler_lists();
        let win: std::collections::HashSet<&str> = lists[0].iter().map(String::as_str).collect();
        let other: Vec<&str> = lists[1].iter().map(String::as_str).collect();

        // ① 非 Windows 段不能有 `rc*`：那些命令函数本身按 `cfg(windows)` 门控，
        //    在 macOS / Linux 上符号不存在 ⇒ 整条注册列表编不过（不是运行时才炸）。
        let leaked: Vec<&&str> = other.iter().filter(|c| c.starts_with("rc")).collect();
        assert!(
            leaked.is_empty(),
            "非 Windows 段里出现了远程电脑命令（{leaked:?}）——那边 `commands::rc*` 根本不存在，会编不过"
        );

        // ② 反向：壳子命令必须两边都有，否则 macOS 构建少一批功能而没人发现。
        let shell: Vec<&&str> = win.iter().filter(|c| !c.starts_with("rc")).collect();
        assert!(
            !shell.is_empty(),
            "Windows 段里连壳子命令都没有，解析器已经不对了"
        );
        for cmd in shell {
            assert!(
                other.contains(cmd),
                "壳子命令 {cmd} 只在 Windows 段注册了 —— 非 Windows 构建会缺它"
            );
        }
    }
}
