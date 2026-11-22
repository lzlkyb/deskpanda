//! ②远端分辨率调节的 service 层：guard 持有 / 会话收口还原 / Windows 崩溃兜底。
//!
//! 还原的三层保证（见 `display_mode.rs` 模块头）里，这里负责：
//! - guard 存 `RcService`（会话期状态），`end_session` 单一收口还原；
//! - Windows：apply 成功即写 config `rc_display_pending_restore`（**本会话原始**
//!   模式），正常还原/结束都清记录；进程崩溃后记录残留 → 下次启动静默还原。
//!   mac 不需要（`kCGConfigureForAppOnly` 进程退出由 OS 还原）。
//!
//! 🔴 连续切换的语义：每次 apply 前先 drop 旧 guard（还原到本会话原始）再切
//!    新档——guard.orig 恒为「本会话原始」，pending 记录与崩溃还原才不会
//!    指到中间档位（4K→1080p→720p 的链式还原会闪烁一帧 4K，正确性优先）。

use super::*;
use crate::rc::display_mode;

/// config 里的兜底记录键（Windows 专用）。
#[cfg(target_os = "windows")]
pub(crate) const CFG_PENDING_RESTORE: &str = "rc_display_pending_restore";

impl RcService {
    /// 被控端：切 `monitor` 号显示器到 `mode`。成功后旧 guard（若有）已还原、
    /// 新 guard 已持有、pending-restore 已按本会话原始档刷新。
    pub(in crate::rc) fn apply_display_mode(
        &self,
        monitor: i32,
        mode: &crate::rc::display_mode::DisplayMode,
    ) -> Result<(), String> {
        // 先还原上一档（见模块头：guard.orig 恒为会话原始的保证）
        self.restore_display_mode();
        let guard = display_mode::apply_mode(monitor, mode)?;
        #[cfg(target_os = "windows")]
        self.write_pending_restore(&guard);
        *self.display_guard.lock().unwrap_or_else(|p| p.into_inner()) = Some(guard);
        Ok(())
    }

    /// 被控端：立刻还原本会话改过的分辨率（对端发还原指令 / 会话收口共用）。
    /// guard 的 Drop 做平台还原；这里只负责取走 + 清兜底记录。
    pub(in crate::rc) fn restore_display_mode(&self) {
        let taken = self
            .display_guard
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take();
        if taken.is_some() {
            #[cfg(target_os = "windows")]
            self.clear_pending_restore();
            log::info!("[DisplayMode] 已还原本会话修改的分辨率");
        }
    }

    /// Windows：guard 的原始档落 config（进程崩溃后的兜底依据）。
    #[cfg(target_os = "windows")]
    fn write_pending_restore(&self, guard: &crate::rc::display_mode::DisplayModeGuard) {
        let Some((device, orig)) = guard.restore_info() else {
            return;
        };
        let Ok(mut config) = self.store.get_config() else {
            log::warn!("[DisplayMode] 写崩溃兜底记录失败（读配置失败）");
            return;
        };
        let Ok(obj) = config
            .as_object_mut()
            .ok_or_else(|| "配置文件不是一个对象".to_string())
        else {
            return;
        };
        obj.insert(
            CFG_PENDING_RESTORE.into(),
            serde_json::json!({
                "device": device,
                "w": orig.w,
                "h": orig.h,
                "hz": orig.hz,
            }),
        );
        if let Err(e) = self.store.save_config(&config) {
            log::warn!("[DisplayMode] 写崩溃兜底记录失败：{e}");
        }
    }

    #[cfg(target_os = "windows")]
    fn clear_pending_restore(&self) {
        let Ok(mut config) = self.store.get_config() else {
            return;
        };
        if let Some(obj) = config.as_object_mut() {
            if obj.remove(CFG_PENDING_RESTORE).is_some() {
                let _ = self.store.save_config(&config);
            }
        }
    }

    /// 启动兜底：上次进程死在「分辨率改了没还原」的状态 → 静默还原 + 清记录。
    /// 只在 Windows 有意义（mac 的 AppOnly 会话 OS 已还原）。
    #[cfg(target_os = "windows")]
    pub(crate) fn restore_pending_display_mode_at_boot(store: &crate::store::DataStore) {
        let Ok(config) = store.get_config() else {
            return;
        };
        let Some(rec) = config.get(CFG_PENDING_RESTORE) else {
            return;
        };
        let (Some(device), Some(w), Some(h), hz) = (
            rec.get("device").and_then(|v| v.as_str()),
            rec.get("w").and_then(|v| v.as_i64()),
            rec.get("h").and_then(|v| v.as_i64()),
            rec.get("hz").and_then(|v| v.as_u64()).unwrap_or(0),
        ) else {
            // 记录坏了：清掉别每次启动都来一遍
            Self::from_store_clear_pending(store);
            return;
        };
        let orig = crate::rc::display_mode::DisplayMode {
            w: w as i32,
            h: h as i32,
            hz: hz as u32,
        };
        match crate::rc::display_mode::restore_by_device(device, &orig) {
            Ok(()) => {
                log::info!(
                    "[DisplayMode] 启动兜底：已把 {device} 还原到 {}×{}@{}（上次会话异常退出残留）",
                    orig.w,
                    orig.h,
                    orig.hz
                );
            }
            Err(e) => {
                log::warn!("[DisplayMode] 启动兜底还原失败（显示器可能已拔/换）：{e}");
            }
        }
        Self::from_store_clear_pending(store);
    }

    #[cfg(target_os = "windows")]
    fn from_store_clear_pending(store: &crate::store::DataStore) {
        if let Ok(mut config) = store.get_config() {
            if let Some(obj) = config.as_object_mut() {
                if obj.remove(CFG_PENDING_RESTORE).is_some() {
                    let _ = store.save_config(&config);
                }
            }
        }
    }
}
