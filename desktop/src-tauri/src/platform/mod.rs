//! 平台绑定：屏幕枚举 / 抓帧 / 光标 / 工作区。
//!
//! 这一层是 `rc` 与操作系统之间唯一的窗口——`rc` 自己不做 Win32 屏幕调用，
//! 只问这里要「有几块屏」「这块屏的矩形」「当前光标在哪」「弹窗该落在哪块屏」。
//!
//! ❗ 全部接口目前只有 Windows 实现（见 [`screen`]）。macOS 的对应实现
//! （`CGDisplay` / `ScreenCaptureKit`）尚未迁入，按平台门控在 `lib.rs`。

pub mod device;
pub mod screen;
