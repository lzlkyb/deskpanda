//! 前端 → 后端的 Tauri IPC 命令层。**本文件只做聚合，不含任何实现。**
//!
//! 组织范式（承自生产项目，可直接照用）：
//! - 每个关注点一个子模块 `xxx.rs` 或 `xxx_cmds.rs`（后者用于与 crate 根下同名
//!   实现模块冲突时，如 `backup` vs `backup_cmds`）。
//! - 本文件 `mod` + `pub use` 重导出，`app.rs` 的 `invoke_handler![...]` 列
//!   `commands::xxx` 即可——拆分命令层是内部结构调整，调用方零改动。
//! - 新增命令：在子模块写 `#[tauri::command]` → 此处 `mod` + `pub use` →
//!   `app.rs` 的 `invoke_handler!` 加一行。三处，缺一不可。

mod net_diag;
mod system;
// Android 下本模块有 13 条 dead_code：桌面更新链路的私有助手（build_updater /
// candidate_endpoint_groups / build_candidate_groups / resolve_update_endpoints /
// resolve_gitee_repository / gitee_repository / DEFAULT_GITEE_REPOSITORY /
// classify_update_error / is_retryable_check_error / MANIFEST_CHECK_TIMEOUT_SECS 等）。
// Android 的更新走 `update_android` 自己那条路（check_update/start_update 在
// Android 上转交 apk 链路），这些助手在 Android 上没有消费者——平台分工，不是腐坏。
// 与 `rc` 同款处理：模块级一次性收口，不在 13 个条目上堆 allow。
// 取证（消费方逐个 grep + 与上游 pastePanda 对照）见迁移文档的执行记录章节；
// 死代码的主门禁仍是 Windows 构建（那里一条都没有）。
#[cfg_attr(target_os = "android", allow(dead_code))]
mod update;

pub use net_diag::*;
pub use system::*;
pub use update::*;

// 手机端（Android）命令层。**不按平台门控**（`mobile_received_file` 例外，见下）：
// 各模块内部自行按平台分派——桌面是 no-op 或诚实报错（与 `rc::keepalive` 同款
// 「桌面 no-op」模式），`app.rs` 的插件链因此不必写平台 cfg。
//
// 用 `pub mod` 而不是裸 `mod`：插件链要按模块路径调 `init()`
// （`commands::update_android::init::<tauri::Wry>()`）。
//
// 🔴 命令名必须走 **glob 重导出**（`pub use xxx::*;`），不能显式列名：
// `#[tauri::command]` 宏生成的 `__cmd__<名字>` / `__tauri_command_name_<名字>`
// 包装器在**定义模块**里，`generate_handler!` 按 `commands::<名字>` 展开时要找
// `commands::__cmd__<名字>`——显式 `pub use mobile_interaction::{mobile_interaction_set, ...}`
// 只带出命令本体，注册表当场 E0433（2026-11-20 实测）。glob 会把包装器一起带出；
// 三个 `init` 因此也在 `commands::` 里撞名——glob 撞名**仅在使用时**报歧义，
// 而插件链一律走模块路径（`commands::update_android::init()`），故无害。
// `mobile_export` 例外：该模块**没有任何 `#[tauri::command]`**（只有插件 struct +
// `init()`，app.rs 全程走模块路径 `commands::mobile_export::init()`），glob 重导出
// 是死代码 ⇒ 不写（写了 clippy `unused_imports` 直接红）。
pub mod mobile_export;
pub mod mobile_interaction;
pub mod update_android;

// `init` 在 mobile_interaction / update_android 两个模块里撞名，glob 到一起即
// `ambiguous_glob_reexports`。**故意留着这个歧义**：显式重导出某一个会让
// `commands::init` 悄悄落到单个插件上（以后有人写 `commands::init()` 时装错插件
// 也不报错），而保持歧义则任何使用点当场 E0659。插件链一律走模块路径，不经过
// 这个名字，故此处只是「名义歧义、实际零使用」。
#[allow(ambiguous_glob_reexports)]
pub use mobile_interaction::*;
#[allow(ambiguous_glob_reexports)]
pub use update_android::*;

// 例外：`mobile_received_file` 读 `rc::file_state` / `rc::service`（文件任务快照），
// 随 `rc` 同一道门控；macOS / Linux 构建里整块不存在。
// 同样走 glob（理由同上：`__cmd__mobile_received_file_action` 包装器要在
// `commands::` 里可见，否则第 1 段注册表 E0433）。
#[cfg(any(windows, target_os = "android", target_os = "macos"))]
mod mobile_received_file;
#[cfg(any(windows, target_os = "android", target_os = "macos"))]
pub use mobile_received_file::*;

// 远程电脑命令层。**Windows / Android**：`rc::` 本身按
// `cfg(any(windows, target_os = "android", target_os = "macos"))` 门控（采集/编码/音频/输入注入是
// Win32 绑定，但协议/状态/传输层要随手机端远程面一起进 Android 包），命令层
// 跟着同一道门控。macOS 构建只出壳子（见 `lib.rs` 的 rc 门控注释）。
// 三个文件按职责分开（不是按规模任意切）：
// - `rc`：配对（邀请码 + 局域网）、设备表、会话、输入、设置
// - `rc_file`：文件传输
// - `rc_pair`：局域网附近设备
#[cfg(any(windows, target_os = "android", target_os = "macos"))]
mod rc;
#[cfg(any(windows, target_os = "android", target_os = "macos"))]
mod rc_file;
#[cfg(any(windows, target_os = "android", target_os = "macos"))]
mod rc_pair;

#[cfg(any(windows, target_os = "android", target_os = "macos"))]
pub use rc::*;
#[cfg(any(windows, target_os = "android", target_os = "macos"))]
pub use rc_file::*;
#[cfg(any(windows, target_os = "android", target_os = "macos"))]
pub use rc_pair::*;

#[cfg(all(test, windows))]
mod registry_guard {
    //! 「实现了但没注册」是远程电脑最容易犯的错：命令写好了、前端 invoke 了，
    //! 界面上就是**点了没反应**（`Command xxx not found` 只在 devtools 里）。
    //! 单条命令的接线守卫在 `rc/tests/guards.rs`，这里补一条**穷举式**的：
    //! rc 三个命令文件 + 手机端命令文件里的每个 `#[tauri::command]` 都必须出现在
    //! `app.rs` 的注册列表里（壳子级命令两段都要有，见下）。

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

    /// 抠出 `app.rs` 里所有 `tauri::generate_handler![…]` 的命令名。
    ///
    /// ❗ 认两种前缀：`commands::` 与 `rc::ask_pop::`。后者是入站申请浮层的三条命令
    /// （`rc_ask_state` / `hide` / `reopen`），定义在 `rc/ask_pop.rs` 而不是命令层里
    /// —— 2026-11-20 它们就是这么漏注册的：本解析器只认 `commands::`，于是「注册表
    /// 少三条」这件事在穷举守卫眼里等于什么都没发生，而浮层卡片永远空白。
    fn handler_lists() -> Vec<Vec<String>> {
        const MARKS: [&str; 2] = ["commands::", "rc::ask_pop::"];
        let src = include_str!("../app.rs");
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

    /// **只存在于 Windows / Android 构建**的命令名集合。
    ///
    /// 判据是「定义所在的模块按 `cfg(any(windows, target_os = "android", target_os = "macos"))` 门控」
    /// （`commands/mod.rs` 顶部把 `rc` / `rc_file` / `rc_pair` 三个模块连同
    /// `rc/ask_pop.rs`、`mobile_received_file` 一起门控；macOS / Linux 构建里
    /// 这些模块整体不存在），**不是**命令名的前缀。按前缀判会把 `rc_net_diag`
    /// 这类误杀——它定义在未门控的 `commands/net_diag.rs` 里，macOS 上符号存在、
    /// 注册合法（2026-11-20 补该命令时实际撞到过：按前缀一刀切会把它判成泄漏）。
    /// 同理，手机端三个桌面 no-op 模块（mobile_interaction / update_android /
    /// mobile_export）的命令在 macOS 上符号存在，不算 windows_only。
    fn windows_only_commands() -> std::collections::HashSet<String> {
        let files = [
            ("commands/rc.rs", include_str!("rc.rs")),
            ("commands/rc_file.rs", include_str!("rc_file.rs")),
            ("commands/rc_pair.rs", include_str!("rc_pair.rs")),
            // 入站申请浮层的三条命令（定义在 rc 里，注册前缀是 `rc::ask_pop::`）。
            ("rc/ask_pop.rs", include_str!("../rc/ask_pop.rs")),
            // 手机端接收文件动作：读 rc 的文件任务快照，随 rc 门控。
            (
                "commands/mobile_received_file.rs",
                include_str!("mobile_received_file.rs"),
            ),
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
            "app.rs 应当有 Windows/Android / 其它两段注册（见段首注释）"
        );
        // 第 0 段是 `#[cfg(any(windows, target_os = \"android\"))]` 那段（源码里先出现）
        let registered: std::collections::HashSet<&str> =
            lists[0].iter().map(String::as_str).collect();

        let files = [
            ("commands/rc.rs", include_str!("rc.rs")),
            ("commands/rc_file.rs", include_str!("rc_file.rs")),
            ("commands/rc_pair.rs", include_str!("rc_pair.rs")),
            // 入站申请浮层的三条命令（定义在 rc 里，注册前缀是 `rc::ask_pop::`）。
            ("rc/ask_pop.rs", include_str!("../rc/ask_pop.rs")),
            // 手机端：交互两条 + apk 安装/授权两条 + 接收文件动作一条。
            // 前四个模块桌面 no-op（macOS 符号存在、两段都要注册），
            // mobile_received_file 随 rc 门控（只在第 0 段注册）。
            (
                "commands/mobile_interaction.rs",
                include_str!("mobile_interaction.rs"),
            ),
            (
                "commands/update_android.rs",
                include_str!("update_android.rs"),
            ),
            (
                "commands/mobile_received_file.rs",
                include_str!("mobile_received_file.rs"),
            ),
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

        // ① 非 Windows/Android 段不能有只在 Windows/Android 存在的命令：那些命令
        //    函数本身按 `cfg(any(windows, target_os = "android", target_os = "macos"))` 门控，在
        //    macOS / Linux 上符号不存在 ⇒ 整条注册列表编不过（不是运行时才炸）。
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

    /// `app.rs` 不许出现 `#[test]`。
    ///
    /// 为什么值得单独立守卫：`app` 模块在 lib 单测构建里被 `cfg(not(test))`
    /// 整体门控（comctl32 v6 manifest 到不了 lib 单测二进制，默认 features 下
    /// `cargo test` 会 0xc0000139 启动即崩，完整取证见 `lib.rs` 的 `pub mod app`
    /// 处长注释）。门控的后果是：往 app.rs 里写 `#[test]` **既不报错也永不运行**——
    /// 一条「看着在跑、其实没跑」的假绿，比没有测试更坏。
    ///
    /// 判据取**行首**（trim 后以 `#[test]` 开头），注释里提到 `#[test]` 不算；
    /// `#[cfg(test)]` 前缀不匹配，不会误伤。
    #[test]
    fn app_rs_里不许出现单测() {
        let src = include_str!("../app.rs");
        let offenders: Vec<&str> = src
            .lines()
            .filter(|l| l.trim_start().starts_with("#[test]"))
            .collect();
        assert!(
            offenders.is_empty(),
            "app.rs 出现了 #[test]（{offenders:?}）：该模块在 lib 单测构建里被整体门控，写在这里的测试不会运行——请挪到 commands/mod.rs 或 rc/tests/ 这些始终编译的位置"
        );
    }
}
