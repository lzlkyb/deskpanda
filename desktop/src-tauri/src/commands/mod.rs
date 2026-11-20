//! 前端 → 后端的 Tauri IPC 命令层。**本文件只做聚合，不含任何实现。**
//!
//! 组织范式（承自生产项目，可直接照用）：
//! - 每个关注点一个子模块 `xxx.rs` 或 `xxx_cmds.rs`（后者用于与 crate 根下同名
//!   实现模块冲突时，如 `backup` vs `backup_cmds`）。
//! - 本文件 `mod` + `pub use` 重导出，`main.rs` 的 `invoke_handler![...]` 列
//!   `commands::xxx` 即可——拆分命令层是内部结构调整，调用方零改动。
//! - 新增命令：在子模块写 `#[tauri::command]` → 此处 `mod` + `pub use` →
//!   `main.rs` 的 `invoke_handler!` 加一行。三处，缺一不可。

mod net_diag;
mod system;
mod update;

pub use net_diag::*;
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

    /// 去掉 `//` 行注释（保留换行，其余原样）。
    ///
    /// 🔴 必须做这一步，否则守卫有个**假绿**后门：把某条注册注释成
    /// `// commands::rc_xxx,`，解析器照样从注释文本里抠出 `rc_xxx`，于是「注册行被
    /// 注释掉」在守卫眼里等于仍然注册着（2026-11-20 实测：注释掉 `rc_ask_state`
    /// 后 787 条测试全绿）。实现不做词法分析，会连带砍掉字符串里的 `//`——
    /// 这个块里只有命令名和注释，没有字符串字面量。
    fn strip_line_comments(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();
        let mut in_comment = false;
        while let Some(c) = chars.next() {
            if in_comment {
                if c == '\n' {
                    in_comment = false;
                    out.push(c);
                }
            } else if c == '/' && chars.peek() == Some(&'/') {
                in_comment = true;
                chars.next();
            } else {
                out.push(c);
            }
        }
        out
    }

    /// 抠出 `main.rs` 里所有 `tauri::generate_handler![…]` 的命令名。
    ///
    /// ❗ 认两种前缀：`commands::` 与 `rc::ask_pop::`。后者是入站申请浮层的三条命令
    /// （`rc_ask_state` / `hide` / `reopen`），定义在 `rc/ask_pop.rs` 而不是命令层里
    /// —— 2026-11-20 它们就是这么漏注册的：本解析器只认 `commands::`，于是「注册表
    /// 少三条」这件事在穷举守卫眼里等于什么都没发生，而浮层卡片永远空白。
    fn handler_lists() -> Vec<Vec<String>> {
        const MARKS: [&str; 2] = ["commands::", "rc::ask_pop::"];
        let src = include_str!("../main.rs");
        let mut out = Vec::new();
        let mut rest = src;
        const MARK: &str = "generate_handler![";
        while let Some(at) = rest.find(MARK) {
            rest = &rest[at + MARK.len()..];
            let end = rest.find(']').expect("generate_handler! 没有闭合");
            let block = &rest[..end];
            rest = &rest[end..];
            let clean = strip_line_comments(block);
            out.push(
                clean
                    .split(',')
                    .filter_map(|item| {
                        // 每个分组前面有一行 `// —— 分组名 ——` 注释（已在上面剥掉），
                        // 这里再取**最靠右**的前缀：万一将来块里出现别的写法，
                        // 靠右的那个才是紧贴命令名的。
                        let mut best: Option<(usize, &str)> = None;
                        for m in MARKS {
                            let mut from = 0;
                            while let Some(rel) = item[from..].find(m) {
                                let at = from + rel;
                                if best.is_none_or(|(b, _)| at > b) {
                                    best = Some((at, m));
                                }
                                from = at + m.len();
                            }
                        }
                        let (at, mark) = best?;
                        item[at + mark.len()..]
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

    /// **只存在于 Windows 构建**的命令名集合。
    ///
    /// 判据是「定义所在的模块按 `cfg(windows)` 门控」（`commands/mod.rs` 顶部
    /// 把 `rc` / `rc_file` / `rc_pair` 三个模块连同 `rc/ask_pop.rs` 一起门控），
    /// **不是**命令名的前缀。按前缀判会把 `rc_net_diag` 这类误杀——它定义在
    /// 未门控的 `commands/net_diag.rs` 里，macOS 上符号存在、注册合法
    /// （2026-11-20 补该命令时实际撞到过：按前缀一刀切会把它判成泄漏）。
    fn windows_only_commands() -> std::collections::HashSet<String> {
        let files = [
            ("commands/rc.rs", include_str!("rc.rs")),
            ("commands/rc_file.rs", include_str!("rc_file.rs")),
            ("commands/rc_pair.rs", include_str!("rc_pair.rs")),
            // 入站申请浮层的三条命令（定义在 rc 里，注册前缀是 `rc::ask_pop::`）。
            ("rc/ask_pop.rs", include_str!("../rc/ask_pop.rs")),
        ];
        let mut set = std::collections::HashSet::new();
        for (name, src) in files {
            for cmd in declared(name, src) {
                set.insert(cmd);
            }
        }
        set
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
            // 入站申请浮层的三条命令（定义在 rc 里，注册前缀是 `rc::ask_pop::`）。
            ("rc/ask_pop.rs", include_str!("../rc/ask_pop.rs")),
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

        // ① 非 Windows 段不能有 Windows-only 命令：那些命令函数本身按
        //    `cfg(windows)` 门控，在 macOS / Linux 上符号不存在 ⇒ 整条注册列表
        //    编不过（不是运行时才炸）。
        //    判据取「定义所在模块是否门控」而不是名字前缀（见 windows_only_commands）。
        let win_only = windows_only_commands();
        assert!(
            win_only.len() > 70,
            "只解析到 {} 条 Windows-only 命令，解析器或门控文件已经不对了",
            win_only.len()
        );
        let leaked: Vec<&&str> = other.iter().filter(|c| win_only.contains(**c)).collect();
        assert!(
            leaked.is_empty(),
            "非 Windows 段里出现了只在 Windows 存在的命令（{leaked:?}）——那边 `commands::rc*` 根本不存在，会编不过"
        );

        // ② 反向：壳子命令必须两边都有，否则 macOS 构建少一批功能而没人发现。
        let shell: Vec<&&str> = win.iter().filter(|c| !win_only.contains(**c)).collect();
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
        // ②b 反向的镜像：非 Windows 段里出现的命令，Windows 段也必须注册。
        //     少了它，「只在非 Windows 段注册」这类漏洞没人抓——`rc_net_diag`
        //     与 `open_file_location` 2026-11-20 迁移时是**两段全漏**
        //     （前端一调用就 command not found：网络诊断卡一进 App 就报、
        //     「打开所在文件夹」点了没反应），而 ② 的方向查不到这个。
        //     非 Windows 段按构造全是壳子命令（① 已禁掉 win_only），直接全量查。
        for cmd in &other {
            assert!(
                win.contains(*cmd),
                "壳子命令 {cmd} 只在非 Windows 段注册了 —— Windows 构建会缺它"
            );
        }
    }
}
