//! 壳子 crate 根。业务模块在此声明（与 commands/ 的子模块一一对应）。
//!
//! 现有模块即壳子全部家当：
//! - `branding`：显示名单一来源
//! - `commands`：IPC 命令层（聚合，实现在 commands/ 子模块）
//! - `config` / `state`：配置与托管状态
//! - `security`：安全原语（路径白名单）
//!
//! 加业务模块时在此 `pub mod xxx;`，实现放 `src/xxx.rs`（关注点够大就开目录），
//! 命令层在 `src/commands/xxx_cmds.rs`，在 `commands/mod.rs` 声明 + `pub use`，
//! 最后在 `app.rs` 的 `invoke_handler!` 注册。四处，缺一不可。

// `app` 在 lib 单测构建里整体门控（2026-11-20）。**门控不是为了修崩溃**——
// 崩溃早已由 build.rs 修掉，见下。
//
// 曾经的事故与曾经的误判（留档，免得有人再按错的因果链改回来）：
// 默认 features 下 `cargo test` 的 lib 单测二进制启动即崩，
// `0xc0000139 STATUS_ENTRYPOINT_NOT_FOUND`。dumpbin 实证：导入表里有
// `comctl32.dll!TaskDialogIndirect`（仅 comctl32 v6 有该入口），而该二进制没有
// Common-Controls v6 manifest。当时的假设是「app.rs 的 Builder → wry + 托盘 →
// tray-icon → muda 把它带进来的」，于是给本模块加了 `cfg(not(test))`。
// **该假设随后被证伪**：门控生效（grep 二进制里 `deskpanda::app` 的
// module_path 字面量为 0）后，导入依旧在 ⇒ 引用方另有其人。现在的解释是
// `tauri::test::mock_builder` + `generate_context!` 会把 wry 运行时的原生对话框
// 代码拉进任何构建 mock 应用的测试二进制（muda 的 `show_about_dialog` 符号本身
// 不在二进制里，故精确引用方未最终定位——不影响修复）。
//
// 真正的修复在 **build.rs**：`compile_for_everything` 发的**裸**
// `cargo:rustc-link-arg` 是唯一能同时覆盖 app bin / lib 单测 / tests/ 三个产物的
// 口径，同时用 `WindowsAttributes::new_without_app_manifest()` 关掉 tauri-build
// 自己那份 manifest，避免两份 RT_MANIFEST 撞成 CVT1100 / LNK1123。实证：各产物
// manifest 均恰好 1 份，839 条 lib 单测全绿。
//
// 那这个门控还留着干什么——**只为卫生**，与崩溃无关：
// 1. app.rs 是组合根（Builder → 插件链 → 托盘 → 窗口），单测要造应用时走
//    `tauri::test::mock_builder`（见 tests/update_flow.rs 的范式），不该把真组合根
//    编进 lib 单测；
// 2. 代价：app.rs 里的 `#[test]` 既不报错也永不运行。由 `commands/mod.rs` 的
//    `registry_guard`「app.rs 不许出现 #[test]」兜底。
#[cfg(not(test))]
pub mod app;
pub mod branding;
pub mod commands;
pub mod config;
#[cfg(any(windows, target_os = "android", target_os = "macos"))]
pub mod dpapi;
// S6（macOS 交付，2026-11-21 迁入）：rc 真正依赖的原仓 `macos/` 桥接子集
// （采集/编码/音频/注入 + av1 + screen + open），paste 主产品的桥接不迁——
// 模块清单与取舍理由见 `src/macos/mod.rs` 头注释与拆分方案 S6.2。
#[cfg(target_os = "macos")]
pub mod macos;
// `net`（共享 iroh 端点）与 `platform` 从 `cfg(windows)` 放宽到
// **含 Android**：手机端是被控/发起两端之一，`rc` 整棵树要在 Android 上编译，
// 而 `commands/net_diag.rs` 与 `rc/service` 的 `MonitorInfo` 字段都指回这两棵树。
// S6 起同样含 macOS（桌面三平台共用同一棵协议/传输树）。
// 平台差异不收在模块声明上（那会让 Android 少符号整树红），收在文件内部：
// `platform::screen` 的 Win32 函数带 `cfg(windows)` + 非 Windows stub，
// `net` 内部仅 identity.rs 有一处 macOS 分支。见 `platform/mod.rs` 模块文档。
#[cfg(any(windows, target_os = "android", target_os = "macos"))]
pub mod net;
pub mod paths;
#[cfg(any(windows, target_os = "android", target_os = "macos"))]
pub mod platform;
// 远程桌面交付平台：**Windows 与 macOS**（S6，2026-11-21 起桌面双平台）+
// Android（手机端被控/查看侧）。平台专属子模块（Windows 的 DXGI/MF、macOS 的
// 三件套 `#[path]` 声明）在 `rc/mod.rs` 内部按平台门控；这里只放行整棵协议/
// 状态/传输树。历史口径（2026-11-20 前只交付 Windows）见拆分方案 S6.1。
//
// 🔴 Android 构建下 `rc` 有 20+ 条 dead_code，**模块级收口**（不逐条 allow）：
// 共享协议/状态层里只被 **Windows 采集/编码/注入侧**消费的条目（流控常量
// WRITE_BUDGET/MAX_PENDING/MELT_*、H.264 缩放 scaled_dimensions、输入注入判定
// is_extended_vk/classify_open、单飞锁 SingleFlight/Admission 等）在 Android 上
// 没有消费者——Android 是被控端的查看/输入侧。**这是平台分工，不是代码腐坏**：
// 逐个 grep 过消费方，全部落在 Windows 专属子模块或 `#[cfg(target_os="macos")]`
// 的 inbound/mac_video（deskpanda 不出 macOS 包，那段不编译）。
// 逐条加 allow 是 20+ 处噪音、逐条加 cfg 门控又有改坏 Windows 构建的风险，
// 故在模块声明处一次性收口。死代码的主门禁仍是 Windows 构建（那里一条都没有）。
// 取证见 `docs/DeskPanda-手机端远程迁移方案-2026-11-20.md` 的执行记录章节。
#[cfg(any(windows, target_os = "android", target_os = "macos"))]
// 死代码豁免同口径扩到 macOS（S6）：mac 消费的是协议/传输/编码侧,
// Windows 专属消费的条目（scaled_dimensions / classify_open 等）在 mac
// 上无引用——平台分工，主门禁仍是 Windows 构建（哪里一条都没有）。
#[cfg_attr(any(target_os = "android", target_os = "macos"), allow(dead_code))]
pub mod rc;
pub mod security;
pub mod state;
pub mod store;

// `app` 用 `use crate::{...}` 直接引根上的模块；AppState 是最常被引用的托管状态，
// 在根上再导出，避免写 `crate::state::AppState` 的长路径。
pub use state::AppState;

/// 应用入口。桌面由 `main.rs` 的 `fn main()` 转调，Android 由 Kotlin 的
/// `RustActivity` 经 JNI 符号 `Java_<prefix>_<name>_start_app` 直接调。
///
/// 🔴 `mobile_entry_point` 属性**必须挂在 lib 目标的函数上**（tauri-macros 2.6.3
/// `mobile.rs` 源码级确认）：宏在 Android 侧生成 JNI 绑定、非 Android 侧生成
/// `#[no_mangle] pub extern "C" fn start_app()`，两者都只落在 lib 的 cdylib 里，
/// 而 APK 加载的正是 cdylib。挂在 `main.rs`（bin target）上 Android 包会因
/// 缺符号在启动时静默失败。应用体因此放在 `app.rs`，本函数只做转调。
///
/// 函数签名保持 `()`：宏的 `stop_unwind` 包装器丢弃返回值，构建期错误在
/// `app::run()` 内已 `expect` 收口（与 pastePanda 的 run() 同款形态）。
///
/// 🔴 lib 单测构建下函数体留空：`app` 模块被 `cfg(not(test))` 整体门控
/// （卫生理由见上方 `pub mod app` 处长注释；与 comctl32 manifest 无关，
/// 那个由 build.rs 负责），门控后这里不能再出现 `app::run()`，否则测试构建编不过。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(not(test))]
    app::run()
}

/// 把窗口摆到用户眼前（工作台窗口、后续的会话窗口都用它）。
///
/// ❗ `unminimize()` 不能省：对已最小化的窗口，`show()` 在 Windows 上
/// **不会把它从任务栏恢复回来**，窗口依然缩着。只写 `show() + set_focus()`
/// 拉不回最小化的窗口——用户看到的是「点了没反应，得再点一次」。
pub fn present_window(window: &tauri::WebviewWindow) {
    // unminimize 是桌面 API（手机端没有最小化概念），mobile 下不编译。
    #[cfg(desktop)]
    window.unminimize().ok();
    if let Err(e) = window.show() {
        log::warn!("[Window] 显示窗口失败: {}", e);
    }
    window.set_focus().ok();
}
