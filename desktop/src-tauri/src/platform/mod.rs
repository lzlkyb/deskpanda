//! 平台绑定：屏幕枚举 / 抓帧 / 光标 / 工作区 / 设备名。
//!
//! 这一层是 `rc` 与操作系统之间唯一的窗口——`rc` 自己不做 Win32 屏幕调用，
//! 只问这里要「有几块屏」「这块屏的矩形」「当前光标在哪」「弹窗该落在哪块屏」
//! 「本机叫什么名字」。
//!
//! ❗ 采集 / 光标 / 工作区目前只有 Windows 实现（见 [`screen`]），其余平台走
//! 诚实 stub（模块文档详述）；设备名（[`device`]）Windows 走主机名、Android 走
//! 系统属性，两端都有真实实现。macOS 的屏幕实现（`CGDisplay` /
//! `ScreenCaptureKit`）尚未迁入，按平台门控在 `lib.rs`。
//!
//! 本模块**不在** `lib.rs` 按 `cfg(windows)` 整体门控：Android 构建要把
//! `rc` 整棵树编进去（手机端是被控/发起两端之一），而 `rc/service` 的
//! `peer_monitors: Vec<MonitorInfo>` 这类字段在所有目标上都要求类型存在——
//! 少一个符号整树红。平台差异收在文件内部的 cfg 里，不收在模块声明上。

pub mod device;
pub mod screen;
