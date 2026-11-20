//! 线路层：帧编解码、路径判定、设备身份、局域网发现、全进程唯一端点。
//!
//! # 与 `rc::` 的界线
//!
//! ```text
//! rc::           会话 / 采集 / 编码 / 文件传输 / 配对   ← 业务，不知道端点从哪来
//! net::          字节怎么出去、走哪条路、对面是谁         ← 本模块
//! ```
//!
//! # 从 PastePanda 迁入时的改名
//!
//! 原仓库这几个文件挂在 `sync::` 下（知识库同步也共用它们）。本产品只做远程桌面，
//! 没有知识库同步，所以整棵树改名为 `net::`，模块本身一行未改：
//!
//! | 原（PastePanda） | 现（DeskPanda） |
//! |---|---|
//! | `crate::sync::transport` | [`transport`] |
//! | `crate::sync::path_kind` | [`path_kind`] |
//! | `crate::sync::identity` | [`identity`] |
//! | `crate::sync::presence` | [`presence`] |
//! | `crate::sync::invite` | [`invite`] |
//! | `crate::shared_ep` | [`endpoint`] |
//!
//! 保留原模块名（而非重构成 `net::frame` 之类）是为了让 `rc/` 里 46k 行代码及其
//! 注释里的 `net::transport::write_frame` 能一一对上原来的取证记录。

pub mod diagnostics;
pub mod endpoint;
pub mod identity;
pub mod invite;
pub mod lan_pair;
pub mod mcast;
/// Android 切网通知 → 刷新共享端点（Kotlin `IrohNetworkPlugin` 的 Rust 侧装载点）。
/// 桌面目标下 `init()` 是空 setup，不注册任何插件。
pub mod network;
pub mod path_kind;
pub mod presence;
pub mod transport;
