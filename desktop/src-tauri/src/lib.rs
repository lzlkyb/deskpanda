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
//! 最后在 `main.rs` 的 `invoke_handler!` 注册。四处，缺一不可。

pub mod branding;
pub mod commands;
pub mod config;
#[cfg(windows)]
pub mod dpapi;
#[cfg(windows)]
pub mod net;
pub mod paths;
#[cfg(windows)]
pub mod platform;
// 🔴 远程桌面当前**只交付 Windows**：采集（DXGI Desktop Duplication）、编码
// （Media Foundation）、音频（WASAPI）、输入注入全是 Win32 绑定。
// macOS 分支（`mac_capture` / AV1 / 录屏）在原仓里依赖一整套 `.m` + `.c` 桥接，
// 尚未迁入 ⇒ 本模块按平台门控，macOS 构建只出壳子，不会假装有远程桌面。
// 见 `docs/DeskPanda-拆分方案-2026-11-19.md` 的 S6。
#[cfg(windows)]
pub mod rc;
pub mod security;
pub mod state;
pub mod store;

// main.rs 用 `use deskpanda::*;` 一把引入；AppState 是最常被引用的托管状态，
// 在根上再导出，避免 main.rs 里写 `use deskpanda::state::AppState` 的长路径。
pub use state::AppState;

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
