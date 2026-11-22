//! ② 远端分辨率调节（竞品借鉴 ②，2026-11-22）：被控端物理显示模式的
//! 枚举 / 会话期切换 / 自动还原。
//!
//! - 枚举进 caps（`display_modes` 字段），发起端据此画分辨率下拉；
//! - 切换由对端 `InputEvent::SetDisplayMode` 触发（要求 Control，见 `inbound.rs`）；
//! - 还原保证三层：**Guard Drop**（`end_session` 单一收口 drop，正常/掉线/
//!   强制结束全覆盖）→ **平台语义**（mac 用 `kCGConfigureForAppOnly`，进程
//!   退出含崩溃由 OS 还原）→ **启动兜底**（Windows 临时模式进程死不还原，
//!   服务层在 config 留 `rc_display_pending_restore`，下次启动静默还原）。
//!
//! 平台真切换 API 全部窄封装在本文件末端的 `#[cfg]` 分支里；去重/排序/
//! 截断是纯函数（单测钉行为）。其余平台枚举返回空 ⇒ caps 不带字段 ⇒
//! 发起端不显示入口（宁缺毋滥）。
//!
//! 🔴 索引口径：`monitor` 与 `platform::screen::list_monitors` / caps 的
//!    `monitors` 数组同序（主屏恒 0）——枚举顺序在这里第二次实现（需要
//!    设备名/displayID），**换序就是错屏**，两侧都在这里钉死。

use serde::{Deserialize, Serialize};

/// 一块显示器的一个显示模式（像素宽高 + 刷新率；`hz == 0` = 平台未报/任意）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DisplayMode {
    pub w: i32,
    pub h: i32,
    pub hz: u32,
}

/// 一块显示器的模式集合（caps 过线，发起端画分辨率下拉）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorModes {
    /// 与 caps `monitors` 数组同序的显示器号（主屏恒 0）。
    pub index: i32,
    /// 当前生效模式（「原始（还原）」的依据）。
    pub current: DisplayMode,
    /// 可选模式（像素降序、每屏截断）。可能不含 current（低分辨率当前态）。
    pub modes: Vec<DisplayMode>,
}

/// 单屏最多上报几档：caps 帧别被模式列表撑爆；像素降序前几档覆盖真实需求
/// （远程调分辨率的场景就是往**低**调提流畅度，但列表按高到低给全谱系，
/// 截断只砍超长尾）。
pub(crate) const MAX_MODES_PER_MONITOR: usize = 8;

/// 原始枚举 → 过滤 / 去重 / 排序 / 截断（纯函数）。
///
/// 过滤：640×480 以下与超荒谬尺寸不进列表（防误切进不可读模式）；
/// 排序：像素面积降序，同尺寸刷新率高在前；三元组去重。
pub(crate) fn normalize_modes(raw: Vec<DisplayMode>) -> Vec<DisplayMode> {
    let mut v: Vec<DisplayMode> = raw
        .into_iter()
        .filter(|m| m.w >= 640 && m.h >= 480 && m.w <= 16384 && m.h <= 16384)
        .collect();
    v.sort();
    v.dedup();
    v.sort_by(|a, b| {
        (b.w as i64 * b.h as i64)
            .cmp(&(a.w as i64 * a.h as i64))
            .then_with(|| b.hz.cmp(&a.hz))
    });
    v.truncate(MAX_MODES_PER_MONITOR);
    v
}

/// 会话期显示模式护栏：Drop 时把该屏还原到 `orig`。
///
/// 调用方（入站会话结构）把它当字段持有——**每一条**退出路径都会析构它，
/// 不需要「记得调还原」的纪律（与 `keep_awake` 同一模式，见其字段注释）。
pub struct DisplayModeGuard {
    restore: Restore,
}

enum Restore {
    #[cfg(target_os = "windows")]
    Win { device: String, orig: DisplayMode },
    #[cfg(target_os = "macos")]
    Mac { display: u32, orig: DisplayMode },
    /// 不支持平台构造不出 guard（apply_mode 已 Err），这个变体只为让
    /// 枚举在不支持的平台上仍可编译。
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    None,
}

impl DisplayModeGuard {
    /// Windows 崩溃兜底记录用：(设备名, 本会话原始模式)。
    #[cfg(target_os = "windows")]
    pub(crate) fn restore_info(&self) -> Option<(&str, DisplayMode)> {
        match &self.restore {
            Restore::Win { device, orig } => Some((device, *orig)),
        }
    }
}

impl Drop for DisplayModeGuard {
    fn drop(&mut self) {
        match &self.restore {
            #[cfg(target_os = "windows")]
            Restore::Win { device, orig } => {
                if let Err(e) = win_set_mode(device, orig) {
                    log::warn!(
                        "[DisplayMode] 会话结束还原 {device} 到 {}×{}@{} 失败：{e}",
                        orig.w,
                        orig.h,
                        orig.hz
                    );
                }
            }
            #[cfg(target_os = "macos")]
            Restore::Mac { display, orig } => {
                if let Err(e) = mac_set_mode(*display, orig) {
                    log::warn!(
                        "[DisplayMode] 会话结束还原显示器 {display} 到 {}×{} 失败：{e}",
                        orig.w,
                        orig.h
                    );
                }
            }
            #[cfg(not(any(target_os = "windows", target_os = "macos")))]
            Restore::None => {}
        }
    }
}

/// 把 `monitor` 号（caps 同序）切到 `mode`；成功返回护栏。
/// `mode.w == 0 && mode.h == 0` 是「还原」哨兵，不该走到这里（上层拦截）。
pub fn apply_mode(monitor: i32, mode: &DisplayMode) -> Result<DisplayModeGuard, String> {
    if mode.w <= 0 || mode.h <= 0 {
        return Err("显示模式尺寸无效".into());
    }
    #[cfg(target_os = "windows")]
    {
        let device = win_device_of_monitor(monitor)?;
        let orig = win_current_mode(&device)?;
        win_set_mode(&device, mode)?;
        Ok(DisplayModeGuard {
            restore: Restore::Win { device, orig },
        })
    }
    #[cfg(target_os = "macos")]
    {
        let display = mac_display_of_monitor(monitor)?;
        let orig = mac_current_mode(display)?;
        mac_set_mode(display, mode)?;
        Ok(DisplayModeGuard {
            restore: Restore::Mac { display, orig },
        })
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = (monitor, mode);
        Err("当前平台不支持调节分辨率".into())
    }
}

/// Windows 启动兜底：按设备名强制还原（config 里 `rc_display_pending_restore`
/// 的消费口，服务层负责读/清记录）。
#[cfg(target_os = "windows")]
pub fn restore_by_device(device: &str, mode: &DisplayMode) -> Result<(), String> {
    win_set_mode(device, mode)
}

// ===== 平台分支：真切换 API 的窄封装，全部不进单测（真机验收覆盖） =====

/// 枚举所有显示器的模式集合（不支持平台返回空 ⇒ caps 不带该字段）。
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub fn list_display_modes() -> Vec<MonitorModes> {
    #[cfg(target_os = "windows")]
    return win_list();
    #[cfg(target_os = "macos")]
    return mac_list();
}
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn list_display_modes() -> Vec<MonitorModes> {
    Vec::new()
}

// ---------- Windows：EnumDisplaySettings / ChangeDisplaySettingsEx ----------

#[cfg(target_os = "windows")]
mod win_imp {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::{
        ChangeDisplaySettingsExW, EnumDisplayMonitors, EnumDisplaySettingsW, GetMonitorInfoW,
        CDS_TYPE, DEVMODEW, DISP_CHANGE_SUCCESSFUL, DM_DISPLAYFREQUENCY, DM_PELSHEIGHT,
        DM_PELSWIDTH, ENUM_CURRENT_SETTINGS, ENUM_DISPLAY_SETTINGS_MODE, HDC, HMONITOR,
        MONITORINFO, MONITORINFOEXW,
    };

    /// 一块屏的枚举上下文：设备名 + 是否主屏（索引口径见模块头）。
    pub(super) struct Dev {
        pub device: String,
        pub primary: bool,
    }

    /// 按 `list_monitors` 同一顺序枚举（EnumDisplayMonitors 序 + 主屏换到 0）。
    pub(super) fn devices() -> Vec<Dev> {
        struct Ctx {
            list: Vec<Dev>,
        }
        unsafe extern "system" fn cb(
            hmon: HMONITOR,
            _hdc: HDC,
            _rc: *mut RECT,
            lp: LPARAM,
        ) -> BOOL {
            let ctx = &mut *(lp.0 as *mut Ctx);
            let mut mi = MONITORINFOEXW {
                monitorInfo: MONITORINFO {
                    cbSize: std::mem::size_of::<MONITORINFOEXW>() as u32,
                    ..Default::default()
                },
                ..Default::default()
            };
            if GetMonitorInfoW(hmon, &mut mi as *mut MONITORINFOEXW as *mut MONITORINFO).as_bool() {
                let len = mi.szDevice.iter().position(|c| *c == 0).unwrap_or(32);
                if len > 0 {
                    ctx.list.push(Dev {
                        device: String::from_utf16_lossy(&mi.szDevice[..len]),
                        primary: mi.monitorInfo.dwFlags & 0x1 != 0,
                    });
                }
            }
            BOOL(1)
        }
        let mut ctx = Ctx { list: Vec::new() };
        unsafe {
            let _ = EnumDisplayMonitors(
                HDC::default(),
                None,
                Some(cb),
                LPARAM(&mut ctx as *mut Ctx as isize),
            );
        }
        // 与 platform::screen::list_monitors 同一口径：主屏换到 0 并重排号
        if ctx.list.len() > 1 {
            if let Some(p) = ctx.list.iter().position(|d| d.primary) {
                ctx.list.swap(0, p);
            }
        }
        ctx.list
    }

    pub(super) fn modes_of(
        device: Option<&str>,
    ) -> (Option<super::DisplayMode>, Vec<super::DisplayMode>) {
        let dev_wide: Option<Vec<u16>> = device.map(|s| s.encode_utf16().chain([0]).collect());
        let name = dev_wide
            .as_deref()
            .map(|w| PCWSTR(w.as_ptr()))
            .unwrap_or_else(PCWSTR::null);
        // 当前模式
        let mut dm = fresh_devmode();
        let mut current = None;
        if unsafe { EnumDisplaySettingsW(name, ENUM_CURRENT_SETTINGS, &mut dm) }.as_bool() {
            current = Some(mode_from(&dm));
        }
        // 全量模式（i 从 0 枚举到失败）
        let mut raw = Vec::new();
        let mut i: u32 = 0;
        loop {
            let mut m = fresh_devmode();
            if !unsafe { EnumDisplaySettingsW(name, ENUM_DISPLAY_SETTINGS_MODE(i), &mut m) }
                .as_bool()
            {
                break;
            }
            raw.push(mode_from(&m));
            i += 1;
            if i > 512 {
                break; // 防病态驱动把列表灌爆
            }
        }
        (current, raw)
    }

    fn fresh_devmode() -> DEVMODEW {
        DEVMODEW {
            dmSize: std::mem::size_of::<DEVMODEW>() as u16,
            ..Default::default()
        }
    }

    fn mode_from(dm: &DEVMODEW) -> super::DisplayMode {
        let hz = if dm.dmFields.contains(DM_DISPLAYFREQUENCY) && dm.dmDisplayFrequency >= 10 {
            dm.dmDisplayFrequency.min(1000)
        } else {
            0
        };
        super::DisplayMode {
            w: dm.dmPelsWidth as i32,
            h: dm.dmPelsHeight as i32,
            hz,
        }
    }

    /// 临时模式切换（flags=0：不写注册表；进程死了**不会**自动还原，
    /// 兜底见服务层 pending-restore）。
    pub(super) fn set_mode(device: &str, m: &super::DisplayMode) -> Result<(), String> {
        let wide: Vec<u16> = device.encode_utf16().chain([0]).collect();
        let mut dm = fresh_devmode();
        dm.dmFields |= DM_PELSWIDTH | DM_PELSHEIGHT;
        dm.dmPelsWidth = m.w as u32;
        dm.dmPelsHeight = m.h as u32;
        if m.hz >= 10 {
            dm.dmFields |= DM_DISPLAYFREQUENCY;
            dm.dmDisplayFrequency = m.hz;
        }
        // flags=0（临时模式,不写注册表）;hwnd 空;lparam 空。
        let r = unsafe {
            ChangeDisplaySettingsExW(
                PCWSTR(wide.as_ptr()),
                Some(&dm as *const DEVMODEW),
                HWND::default(),
                CDS_TYPE(0),
                None,
            )
        };
        if r == DISP_CHANGE_SUCCESSFUL {
            Ok(())
        } else {
            Err(format!(
                "切换显示模式失败（{r:?}）——显示器可能不支持 {}×{}@{}",
                m.w, m.h, m.hz
            ))
        }
    }
}

#[cfg(target_os = "windows")]
use win_imp::{devices as win_devices, modes_of as win_modes_of, set_mode as win_set_mode};

#[cfg(target_os = "windows")]
fn win_list() -> Vec<MonitorModes> {
    let mut out = Vec::new();
    for (i, dev) in win_devices().into_iter().enumerate() {
        let (current, raw) = win_modes_of(Some(&dev.device));
        let Some(current) = current else { continue };
        out.push(MonitorModes {
            index: i as i32,
            current,
            modes: normalize_modes(raw),
        });
    }
    out
}

#[cfg(target_os = "windows")]
fn win_device_of_monitor(monitor: i32) -> Result<String, String> {
    let devs = win_devices();
    devs.get(monitor as usize)
        .map(|d| d.device.clone())
        .ok_or_else(|| format!("显示器号 {monitor} 不存在（共 {} 块）", devs.len()))
}

#[cfg(target_os = "windows")]
fn win_current_mode(device: &str) -> Result<DisplayMode, String> {
    win_modes_of(Some(device))
        .0
        .ok_or_else(|| "读不到当前显示模式".to_string())
}

// ---------- macOS：CGDisplay（AppOnly 会话，进程退出自动还原） ----------

#[cfg(target_os = "macos")]
mod mac_imp {
    use serde::Deserialize;

    #[derive(Deserialize)]
    pub(super) struct RawMode {
        pub w: i32,
        pub h: i32,
        pub hz: u32,
        pub current: bool,
    }
}

extern "C" {
    #[cfg(target_os = "macos")]
    fn pp_mac_display_modes(display: u32, output: *mut *mut u8, length: *mut usize) -> i32;
    #[cfg(target_os = "macos")]
    fn pp_mac_set_display_mode(
        display: u32,
        w: i32,
        h: i32,
        hz: u32,
        output: *mut *mut u8,
        length: *mut usize,
    ) -> i32;
    #[cfg(target_os = "macos")]
    fn pp_mac_free(memory: *mut std::ffi::c_void);
}

#[cfg(target_os = "macos")]
fn mac_call(call: impl FnOnce(*mut *mut u8, *mut usize) -> i32) -> Result<Vec<u8>, String> {
    let mut output = std::ptr::null_mut();
    let mut length = 0usize;
    let status = call(&mut output, &mut length);
    struct Native(*mut u8);
    impl Drop for Native {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { pp_mac_free(self.0.cast()) }
            }
        }
    }
    let owned = Native(output);
    if status != 0 {
        return Err(format!("显示器模式调用失败（status {status}）"));
    }
    if owned.0.is_null() || length == 0 {
        return Err("显示器模式数据为空".into());
    }
    Ok(unsafe { std::slice::from_raw_parts(owned.0, length) }.to_vec())
}

#[cfg(target_os = "macos")]
fn mac_display_of_monitor(monitor: i32) -> Result<u32, String> {
    let list = crate::macos::screen::native_monitors()?;
    list.get(monitor as usize)
        .map(|m| m.display_id)
        .ok_or_else(|| format!("显示器号 {monitor} 不存在（共 {} 块）", list.len()))
}

#[cfg(target_os = "macos")]
fn mac_list() -> Vec<MonitorModes> {
    let Ok(list) = crate::macos::screen::native_monitors() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for m in &list {
        let Ok(bytes) = mac_call(|p, n| unsafe { pp_mac_display_modes(m.display_id, p, n) }) else {
            continue;
        };
        let Ok(raw) = serde_json::from_slice::<Vec<mac_imp::RawMode>>(&bytes) else {
            continue;
        };
        let Some(cur) = raw.iter().find(|r| r.current) else {
            continue;
        };
        out.push(MonitorModes {
            index: m.monitor.index,
            current: DisplayMode {
                w: cur.w,
                h: cur.h,
                hz: cur.hz,
            },
            modes: normalize_modes(
                raw.iter()
                    .map(|r| DisplayMode {
                        w: r.w,
                        h: r.h,
                        hz: r.hz,
                    })
                    .collect(),
            ),
        });
    }
    out
}

#[cfg(target_os = "macos")]
fn mac_current_mode(display: u32) -> Result<DisplayMode, String> {
    let bytes = mac_call(|p, n| unsafe { pp_mac_display_modes(display, p, n) })?;
    let raw: Vec<mac_imp::RawMode> =
        serde_json::from_slice(&bytes).map_err(|e| format!("显示器模式数据无效：{e}"))?;
    raw.iter()
        .find(|r| r.current)
        .map(|r| DisplayMode {
            w: r.w,
            h: r.h,
            hz: r.hz,
        })
        .ok_or_else(|| "读不到当前显示模式".into())
}

#[cfg(target_os = "macos")]
fn mac_set_mode(display: u32, m: &DisplayMode) -> Result<(), String> {
    // 桥接返回一条 JSON 错误串（空 = 成功）；w/h 哨兵校验在 apply_mode 已过。
    let bytes = mac_call(|p, n| unsafe { pp_mac_set_display_mode(display, m.w, m.h, m.hz, p, n) })?;
    let err = String::from_utf8_lossy(&bytes).trim().to_string();
    if err.is_empty() {
        Ok(())
    } else {
        Err(err)
    }
}

/// settings 回执通道的 value 编码：`"m{monitor}/{w}x{h}@{hz}"`（还原哨兵 = `m{n}/0x0@0`）。
/// 发起端 `rcApplySetting(sessionId, "display_mode", v)` 与被控端回执共用同一编码,
/// 回执 value 逐字匹配靠它。
pub(crate) fn encode_setting(monitor: i32, w: i32, h: i32, hz: u32) -> String {
    format!("m{monitor}/{w}x{h}@{hz}")
}

/// [`encode_setting`] 的反解。非法串报人话错误（回执通道两头都先验）。
pub(crate) fn decode_setting(v: &str) -> Result<(i32, i32, i32, u32), String> {
    let v = v.trim();
    let Some(rest) = v.strip_prefix('m') else {
        return Err(format!(
            "直连分辨率值「{v}」应以 m 开头（如 m0/1920x1080@60）"
        ));
    };
    let Some((mon, dims)) = rest.split_once('/') else {
        return Err(format!(
            "直连分辨率值「{v}」缺显示器号分隔（如 m0/1920x1080@60）"
        ));
    };
    let mon: i32 = mon
        .parse()
        .map_err(|_| format!("显示器号「{mon}」不是整数"))?;
    let Some((wh, hz)) = dims.rsplit_once('@') else {
        return Err(format!("分辨率「{dims}」缺刷新率分隔（如 1920x1080@60）"));
    };
    let Some((w, h)) = wh.split_once('x') else {
        return Err(format!("分辨率「{wh}」应为 宽x高（如 1920x1080）"));
    };
    let w: i32 = w.parse().map_err(|_| format!("宽度「{w}」不是整数"))?;
    let h: i32 = h.parse().map_err(|_| format!("高度「{h}」不是整数"))?;
    let hz: u32 = hz.parse().map_err(|_| format!("刷新率「{hz}」不是整数"))?;
    Ok((mon, w, h, hz))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(w: i32, h: i32, hz: u32) -> DisplayMode {
        DisplayMode { w, h, hz }
    }

    #[test]
    fn 模式表_去重_过滤_按面积降序() {
        let out = normalize_modes(vec![
            m(1920, 1080, 60),
            m(3840, 2160, 60),
            m(1920, 1080, 60), // 重复
            m(640, 480, 60),   // 低于下限,过滤
            m(100, 100, 60),   // 过滤
            m(1920, 1080, 144),
        ]);
        assert_eq!(
            out,
            vec![
                m(3840, 2160, 60),
                m(1920, 1080, 144),
                m(1920, 1080, 60),
                m(640, 480, 60),
            ],
            "面积降序、同尺寸高刷在前、三元组去重;640×480 恰在门槛内(≥640 且 ≥480)保留"
        );
    }

    #[test]
    fn 模式表_门槛恰好在640x480() {
        assert_eq!(
            normalize_modes(vec![m(640, 480, 60)]),
            vec![m(640, 480, 60)]
        );
        assert!(normalize_modes(vec![m(639, 480, 60)]).is_empty());
        assert!(normalize_modes(vec![m(640, 479, 60)]).is_empty());
    }

    #[test]
    fn 模式表_截断到上限() {
        let raw: Vec<DisplayMode> = (0..20).map(|i| m(1000 + i, 800, 60)).collect();
        assert_eq!(normalize_modes(raw).len(), MAX_MODES_PER_MONITOR);
    }

    #[test]
    fn 模式表_hz0是合法档_平台未报刷新率时仍可切() {
        let out = normalize_modes(vec![m(1920, 1080, 0)]);
        assert_eq!(out, vec![m(1920, 1080, 0)]);
    }

    #[test]
    fn settings编码_往返与非法串() {
        let (m, w, h, hz) = decode_setting(&encode_setting(0, 1920, 1080, 60)).unwrap();
        assert_eq!((m, w, h, hz), (0, 1920, 1080, 60));
        // 还原哨兵
        assert_eq!(decode_setting("m1/0x0@0").unwrap(), (1, 0, 0, 0));
        for bad in [
            "1920x1080",
            "m0",
            "m0/1920",
            "m0/1920x1080",
            "mx/1920x1080@60",
            "m0/ax1080@60",
        ] {
            assert!(decode_setting(bad).is_err(), "「{bad}」应被拒绝");
        }
    }

    #[test]
    fn apply_mode_拒绝零尺寸与还原哨兵() {
        assert!(
            apply_mode(0, &m(0, 0, 0)).is_err(),
            "w=0,h=0 是还原哨兵,不该进 apply"
        );
        assert!(apply_mode(0, &m(0, 1080, 60)).is_err());
    }
}

#[cfg(all(test, target_os = "macos"))]
mod mac_probe_tests {
    use super::*;
    /// 真机探针(仅 mac 宿主跑):桥接枚举真的出数据、当前模式在内。
    /// CI 是 mac/linux 双平台——linux 上这个模块不编译;mac CI 上无头环境可能
    /// 枚举为空,断言写成条件式(空列表只验不 panic,非空才验结构)。
    #[test]
    fn mac_枚举探针_结构合法() {
        for m in list_display_modes() {
            assert!(m.index >= 0);
            assert!(
                m.current.w >= 640 && m.current.h >= 480,
                "当前模式不该是荒谬尺寸"
            );
            assert!(!m.modes.is_empty());
            for x in &m.modes {
                assert!(x.w >= 640 && x.h >= 480);
            }
        }
    }
}
