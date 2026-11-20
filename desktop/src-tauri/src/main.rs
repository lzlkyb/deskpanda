//! 桌面壳入口（薄壳）：Windows release 的「不弹控制台」属性 + 转调 lib 的 `run()`。
//!
//! **为什么这里只剩两行**：应用体（托盘 / 单实例 / 窗口事件 / 命令注册）在
//! `app.rs`。原因是 `tauri::mobile_entry_point` 生成的 JNI / `start_app` 符号
//! 必须落在 **lib 目标**（APK 加载的是 cdylib），而 bin target 不进 APK——
//! 详见 `lib.rs` 的 `run()` 与 `app.rs` 的模块文档。**别把应用体搬回本文件。**
//!
//! `#![windows_subsystem]` 是 crate 级属性、只对 bin 目标生效，所以留在这里；
//! lib 目标上没有它也不影响（属性不被继承，release 桌面包的 GUI 形态由本文件决定）。

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    deskpanda::run()
}
