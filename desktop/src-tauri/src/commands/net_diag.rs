//! 网络诊断命令层（R0，迁自原仓
//! `pastePanda/src-tauri/src/commands/net_diag.rs`）。
//!
//! 把共享 iroh 端点的 net_report 结构化出口给前端：判定逻辑（对称 NAT →
//! iroh 放弃直连等）在 [`crate::net::diagnostics::punch_verdict`]，本层只取数。
//! 数据只下发到本机 UI，不落盘、不上传（红线②口径：观测含公网映射，仅本地可见）。
//!
//! 迁移核查（2026-11-20）发现这条命令在原仓有、本仓漏了：前端
//! `lib/api/rcNetDiag.ts` 一直在调，宿主 `RcNetDiagCard` 挂在主界面
//! `RcA2SelfCard` 上 ⇒ 一进 App 就报「命令不存在」。本文件补上缺口。

use serde::Serialize;

#[cfg(any(windows, target_os = "macos"))]
use crate::net;

/// `rc_net_diag` 的返回：快照 + 取不到快照时的原因。
#[derive(Serialize)]
pub struct NetDiag {
    /// `None` = 端点未绑定（远程电脑从未开启过联网业务）。
    #[cfg(any(windows, target_os = "macos"))]
    pub snapshot: Option<net::diagnostics::NetDiagSnapshot>,
    /// 其余平台：`NetDiagSnapshot` 不编译，恒 `None`（序列化成 `null`，
    /// 与前端 `NetDiagSnapshot | null` 契约一致）。
    #[cfg(not(any(windows, target_os = "macos")))]
    pub snapshot: Option<()>,
    /// 快照为空时给前端的一句话原因（有快照时为空串）。
    pub unavailable_reason: String,
}

/// 本机网络画像 + 打洞可行性判定（本地显示用）。
///
/// 🔴 命令本体**不**随平台门控（`app.rs` 两段都要注册——前端一进 App 就调它，
/// 漏注册就是 2026-11-20 修过的「命令不存在」）；门控的只是实现：探测住在
/// `net` 模块。S6 起 `net` 在 Windows/macOS 都编译 ⇒ 桌面双平台走真探测；
/// 其余平台（Android 等）暂无诊断面板消费方，保持诚实降级。
#[tauri::command]
pub async fn rc_net_diag() -> Result<NetDiag, String> {
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        Ok(NetDiag {
            snapshot: None,
            unavailable_reason: "当前平台没有网络诊断".into(),
        })
    }
    #[cfg(any(windows, target_os = "macos"))]
    {
        let Some(ep) = net::endpoint::existing() else {
            return Ok(NetDiag {
                snapshot: None,
                unavailable_reason: "共享端点未绑定：请先开启远程电脑".into(),
            });
        };
        match net::diagnostics::snapshot(&ep).await {
            Some(snapshot) => Ok(NetDiag {
                snapshot: Some(snapshot),
                unavailable_reason: String::new(),
            }),
            None => Ok(NetDiag {
                snapshot: None,
                unavailable_reason: "网络探测尚未完成（首份报告约需数秒）".into(),
            }),
        }
    }
}
