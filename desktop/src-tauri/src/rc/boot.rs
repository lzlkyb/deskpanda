//! 远程电脑的后端启动接线：单例、事件契约、数据库落盘位置。
//!
//! # 为什么单独一个文件
//!
//! `main.rs` 只管「一个有托盘的桌面窗口」，业务接线按壳子的分层纪律不进它。
//! 而这段接线又必须**贴着 `rc` 看**：事件名、回调语义、单例的唯一性，
//! 改任何一处都要和 `service::` 的 notify 契约对齐，放在 `rc/` 里改错成本最低。
//!
//! # 事件契约（前端必须用同一批字面量）
//!
//! | 事件 | 载荷 | 谁需要它 |
//! |---|---|---|
//! | [`EV_SESSION`] | `RcStatus` 全量快照 | 会话壳、设备列表、设置页 |
//! | [`EV_FRAME_READY`] | `()` | 发起端：来帧了，去 `rc_drain_frames` 取 |
//! | [`EV_FILE_STATE`] | 文件状态快照 JSON | 文件确认条与进度条 |
//! | [`EV_CURSOR`] | `{shape,x,y}` | 发起端画远端光标 |
//! | [`EV_SCOPE`] | `{scope,by_peer:true}` | 被控端横幅「对方改了画面范围」 |
//! | [`EV_STREAM_NOTE`] | `{kind,name}` | 被控端横幅「对方改了画质」 |
//! | [`EV_PATH`] | `{from,to}` | 「从哪条路换到了哪条路」 |
//! | [`EV_INJECT_ERR`] | 文本 | 被控端：注入失败 toast |
//! | [`EV_CLIP_PUSH_ERR`] | 文本 | 发起端：剪贴板推不过去 toast |
//!
//! 🔴 这些名字是**跨语言契约**：`emit` 出去没人接不会报错，只会「功能静默不生效」。
//! 改名前先 grep 前端（`src/lib/api/rcEvents.ts` 与所有 `listen("rc-…")`）。

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};

use super::service::{self, RcService};

/// 会话状态变化（全量快照，不是增量）。
pub const EV_SESSION: &str = "rc-session-changed";
/// outbox 有新帧：前端据此拉一次 `rc_drain_frames`。
pub const EV_FRAME_READY: &str = "rc-frame-ready";
/// 文件传输状态快照。
pub const EV_FILE_STATE: &str = "rc-file-state";
/// 远端光标遥测。
pub const EV_CURSOR: &str = "rc-cursor-changed";
/// 被控端画面范围被**对端**改动。
pub const EV_SCOPE: &str = "rc-scope-changed";
/// 被控端画质 / 编码被**对端**改动。
pub const EV_STREAM_NOTE: &str = "rc-stream-note";
/// 会话换路了（中继 ↔ 直连）。
pub const EV_PATH: &str = "rc-path-changed";
/// 被控端：本机输入注入失败。
pub const EV_INJECT_ERR: &str = "rc-inject-error";
/// 发起端：剪贴板推不过去（对端拒收 / 写不进）。
pub const EV_CLIP_PUSH_ERR: &str = "rc-clip-push-error";

/// 数据库文件名。**不要跟着 productName 改**：改名等于换库 = 用户已配对设备全丢。
const DB_FILE: &str = "deskpanda.db";

/// 建立 Store 与 `RcService` 单例，并把事件回调接上。
///
/// 调用点：`main.rs` 的 `setup`，**一次**。重复调用会 panic（`AppHandle::manage`
/// 对同一个类型只能存一份），这是刻意的：两份 `RcService` 会让会话状态与
/// ALPN 路由各说各话（原仓在这一点上踩过）。
pub fn boot(app: &AppHandle) -> Result<Arc<RcService>, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("无法解析应用数据目录：{e}"))?;
    std::fs::create_dir_all(&data_dir)
        .map_err(|e| format!("无法创建应用数据目录 {}：{e}", data_dir.display()))?;
    let store = crate::store::DataStore::new(data_dir.join(DB_FILE))?;
    app.manage(store.clone());

    let svc = Arc::new(RcService::new(store));
    // 全局单例：入站 ALPN 路由（`net::endpoint`）与命令层必须拿到**同一个**实例。
    service::install_global(svc.clone());
    // State 也放同一份：79 条命令都是 `State<'_, Arc<RcService>>`。
    app.manage(svc.clone());

    install_notify(app, &svc);
    Ok(svc)
}

/// 把 `RcService` 的回调接到 Tauri 事件上。
///
/// 每个块单独 clone 一份 `AppHandle`：闭包要 `'static + Send + Sync`，
/// 共用同一个 handle 得加 `Arc`，而 `AppHandle` 本来就是引用计数的句柄，
/// 一个个 clone 更省事也更好读。
fn install_notify(app: &AppHandle, svc: &Arc<RcService>) {
    {
        let handle = app.clone();
        svc.set_notify(Arc::new(move || {
            // ❗ 取 `global()` 而不是捕获 `svc`：回调可能在 `RcService` 内部
            // 持锁时触发，闭包里再持一份 `Arc` 会把「谁先释放」变成隐式契约。
            let Some(svc) = super::service::global() else {
                return;
            };
            let _ = handle.emit(EV_SESSION, svc.status());
            // 「取走即清」：同一句错误只报一次，否则每次状态变都会重播。
            if let Some(err) = svc.take_inject_err() {
                let _ = handle.emit(EV_INJECT_ERR, err);
            }
            if let Some(err) = svc.take_clip_push_err() {
                let _ = handle.emit(EV_CLIP_PUSH_ERR, err);
            }
        }));
    }
    {
        let handle = app.clone();
        svc.set_frame_notify(Arc::new(move || {
            let _ = handle.emit(EV_FRAME_READY, ());
        }));
    }
    {
        let handle = app.clone();
        svc.set_scope_notify(Arc::new(move |scope: &str| {
            let _ = handle.emit(
                EV_SCOPE,
                serde_json::json!({ "scope": scope, "by_peer": true }),
            );
        }));
    }
    {
        let handle = app.clone();
        svc.set_stream_notify(Arc::new(move |kind: &str, name: &str| {
            let _ = handle.emit(
                EV_STREAM_NOTE,
                serde_json::json!({ "kind": kind, "name": name }),
            );
        }));
    }
    {
        let handle = app.clone();
        svc.set_path_notify(Arc::new(move |from: &str, to: &str| {
            let _ = handle.emit(EV_PATH, serde_json::json!({ "from": from, "to": to }));
        }));
    }
    {
        let handle = app.clone();
        svc.set_cursor_notify(Arc::new(move |cur: &super::input::RemoteCursor| {
            let _ = handle.emit(
                EV_CURSOR,
                serde_json::json!({ "shape": cur.shape, "x": cur.x, "y": cur.y }),
            );
        }));
    }
    {
        let handle = app.clone();
        svc.set_file_notify(Arc::new(move |json: &str| {
            // 载荷是后端序列化好的快照：转发前解一次只为「不合法就别发出去」，
            // 免得前端拿到一段坏 JSON 后 `JSON.parse` 抛错把监听器整条打掉。
            match serde_json::from_str::<serde_json::Value>(json) {
                Ok(v) => {
                    let _ = handle.emit(EV_FILE_STATE, v);
                }
                Err(e) => log::warn!("[RC] 文件状态事件载荷不合法：{e}"),
            }
        }));
    }
}
