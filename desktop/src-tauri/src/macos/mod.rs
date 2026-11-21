//! macOS 桥接子集（S6 迁入）。
//!
//! 来源：原仓 pastePanda `src-tauri/src/macos/`（41 个文件）中 **rc 真正依赖的
//! 子集**——迁入范围清单与对账判据见 `docs/DeskPanda-拆分方案-2026-11-19.md`
//! S6.2/S6.3。paste 主产品的桥接（paste.m / 录屏 / GIF / 裁剪 / 贴边 / 文件关联 /
//! 启动项 / 剪贴板 / 滚动等）**有意不迁**：本产品没有那些能力，拖着它们等于把
//! pastePanda 的产品面搬进来。
//!
//! `recording_frame.m` 是唯一例外：它虽属原仓录屏族，但 `remote_video.m` 的编码
//! 路径调用其 `pp_recording_sample`（RGBA → CMSampleBuffer 的纯工具函数），
//! 链接期必需。
//!
//! 🔴 这里没有（也不许有）mod 级业务代码：原仓 `macos/mod.rs` 是 paste 引擎本体
//! （目标 PID 管理 / 剪贴板写入 / 应用识别），S3 判据明确 rc 不得依赖那些。
pub(crate) mod av1;
pub(crate) mod av1_receive;
pub mod open;
pub mod screen;
pub mod screen_layout;
/// Keychain 加解密（`dpapi` 的 macOS 后端——身份文件/令牌的落盘加密走它；
/// S6.2 清单外的传递依赖：`net::identity` → `dpapi` → 这里）。
pub(crate) mod secret_store;
