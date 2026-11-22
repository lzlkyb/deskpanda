//! ScreenCaptureKit adapter. Physical coordinates use the primary display scale.
//!
//! S6 迁入（2026-11-21）时相对原仓的两处调整：
//! - `MonitorInfo` / `ScreenCapture` 原住在 pastePanda 的 `crate::screenshot`，
//!   本仓没有那棵树——类型收编为局部定义（字段与 `platform::screen::MonitorInfo`
//!   同形，但**刻意不复用它**：`platform` 是跨平台门面，这里是 mac 桥接私有层，
//!   各自演化不互相牵制）；
//! - `window_rects` / `window_at`（贴图/截图取景的窗口枚举）是 paste 主产品
//!   能力，rc 无消费方，未迁。
use serde::{Deserialize, Serialize};

/// 一台显示器的几何信息（与 `platform::screen::MonitorInfo` 同形；见模块注释）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorInfo {
    /// 0 起；主屏被调到 0（与 Windows 侧 `EnumDisplayMonitors` + 主屏前置同口径）。
    pub index: i32,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub primary: bool,
}

/// 一次区域截图（PNG data URL——`capture` 是截图口径，不是远程流口径；
/// 远程流走 `remote_capture.rs` 的裸像素，不经这里）。
pub struct ScreenCapture {
    pub data_url: String,
    pub origin_x: i32,
    pub origin_y: i32,
    pub width: i32,
    pub height: i32,
}

extern "C" {
    fn pp_mac_monitors(output: *mut *mut u8, length: *mut usize) -> i32;
    fn pp_mac_capture_display(
        display: u32,
        width: u32,
        height: u32,
        output: *mut *mut u8,
        length: *mut usize,
    ) -> i32;
    fn pp_mac_free(memory: *mut std::ffi::c_void);
    fn pp_mac_screen_scale() -> f64;
    fn pp_mac_display_hz(display: u32) -> u32;
    fn pp_mac_cursor(x: *mut i32, y: *mut i32);
}
struct NativeBytes(*mut u8);
impl Drop for NativeBytes {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { pp_mac_free(self.0.cast()) }
        }
    }
}
fn read_native(call: impl FnOnce(*mut *mut u8, *mut usize) -> i32) -> Result<Vec<u8>, String> {
    let mut output = std::ptr::null_mut();
    let mut length = 0;
    let status = call(&mut output, &mut length);
    let owned = NativeBytes(output);
    if status != 0 {
        return Err(match status {
            2 => {
                "无法读取屏幕。请在系统设置 → 隐私与安全性 → 屏幕录制中允许 DeskPanda，然后重启应用"
            }
            4 => "Mac 屏幕采集超时，请重试",
            6 => "屏幕图像超出安全尺寸限制",
            7 => "屏幕采集不能在主线程运行",
            8 => "当前截图适配需要 macOS 12.3 或更高版本",
            _ => "Mac 屏幕采集失败，请检查权限并重启应用",
        }
        .into());
    }
    if owned.0.is_null() || length == 0 || length > 256 * 1024 * 1024 {
        return Err("屏幕数据无效".into());
    }
    Ok(unsafe { std::slice::from_raw_parts(owned.0, length) }.to_vec())
}
#[derive(serde::Deserialize)]
pub(crate) struct NativeMonitor {
    #[serde(flatten)]
    pub monitor: MonitorInfo,
    #[serde(rename = "displayId")]
    pub display_id: u32,
    pub scale: f64,
}
pub(crate) fn native_monitors() -> Result<Vec<NativeMonitor>, String> {
    let list: Vec<NativeMonitor> =
        serde_json::from_slice(&read_native(|p, n| unsafe { pp_mac_monitors(p, n) })?)
            .map_err(|e| format!("显示器信息无效: {e}"))?;
    if list.is_empty()
        || list.len() > 32
        || list
            .iter()
            .any(|m| m.display_id == 0 || !m.scale.is_finite() || m.scale <= 0.)
    {
        return Err("显示器信息无效".into());
    }
    Ok(list)
}
/// 布局变化的分类（②远端分辨率调节,2026-11-22）。
pub(crate) enum LayoutChange {
    /// 指纹一致。
    Same,
    /// 同一组显示器、同一位置，只有 w/h/scale 变了——**分辨率被切**，
    /// 采集可以按新几何重建,不必断会话。
    ModeOnly,
    /// 显示器增删/位移——维持原熔断口径(布局变了,拼合/区域语义失效)。
    Structural(&'static str),
}

/// 把「旧布局快照 vs 当前枚举」分类。原点(x,y)与 display_id 必须逐一相同
/// 才算 ModeOnly——副屏尺寸变化常引发邻居屏原点重排,那属于 Structural
/// (多屏重排场景仍按旧行为终止,单屏切换是绝对主流)。
pub(crate) fn classify_layout_change(old: &[NativeMonitor], new: &[NativeMonitor]) -> LayoutChange {
    let a = fingerprint(old);
    let b = fingerprint(new);
    if a == b {
        return LayoutChange::Same;
    }
    if a.len() == b.len()
        && a.iter()
            .zip(b.iter())
            .all(|(x, y)| x.0 == y.0 && x.1 == y.1 && x.2 == y.2)
    {
        return LayoutChange::ModeOnly;
    }
    LayoutChange::Structural("远控显示器布局或缩放已变化，请重新连接")
}

pub(crate) fn fingerprint(list: &[NativeMonitor]) -> Vec<(u32, i32, i32, i32, i32, u64)> {
    list.iter()
        .map(|m| {
            (
                m.display_id,
                m.monitor.x,
                m.monitor.y,
                m.monitor.w,
                m.monitor.h,
                m.scale.to_bits(),
            )
        })
        .collect()
}
pub fn monitors() -> Result<Vec<MonitorInfo>, String> {
    Ok(native_monitors()?.into_iter().map(|m| m.monitor).collect())
}
pub fn desktop() -> Result<super::screen_layout::Rect, String> {
    super::screen_layout::bounds(&monitors()?)
}
pub fn primary() -> Result<MonitorInfo, String> {
    monitors()?
        .into_iter()
        .find(|m| m.primary)
        .ok_or("未找到主显示器".into())
}
pub fn primary_scale() -> f64 {
    let scale = unsafe { pp_mac_screen_scale() };
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}
pub fn cursor() -> (i32, i32) {
    let (mut x, mut y) = (0, 0);
    unsafe {
        pp_mac_cursor(&mut x, &mut y);
    }
    (x, y)
}
pub fn capture(x: i32, y: i32, w: i32, h: i32) -> Result<ScreenCapture, String> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use image::{
        codecs::png::{CompressionType, FilterType, PngEncoder},
        ImageEncoder,
    };
    let native = native_monitors()?;
    let list: Vec<MonitorInfo> = native.iter().map(|m| m.monitor.clone()).collect();
    let region = super::screen_layout::Rect { x, y, w, h };
    let frame = super::screen_layout::compose(&list, region, |m| {
        let source = native
            .iter()
            .find(|n| n.monitor.index == m.index)
            .ok_or("显示器已移除")?;
        let png = read_native(|p, n| unsafe {
            pp_mac_capture_display(source.display_id, m.w as u32, m.h as u32, p, n)
        })?;
        let img = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
            .map_err(|e| format!("截图解码失败: {e}"))?;
        Ok(img.to_rgba8())
    })?;
    if fingerprint(&native) != fingerprint(&native_monitors()?) {
        return Err("显示器布局或缩放已变化，请重新截图".into());
    }
    let mut output = Vec::new();
    PngEncoder::new_with_quality(&mut output, CompressionType::Fast, FilterType::Up)
        .write_image(&frame, w as u32, h as u32, image::ExtendedColorType::Rgba8)
        .map_err(|e| format!("截图编码失败: {e}"))?;
    Ok(ScreenCapture {
        data_url: format!("data:image/png;base64,{}", STANDARD.encode(output)),
        origin_x: x,
        origin_y: y,
        width: w,
        height: h,
    })
}
pub fn capture_screen() -> Result<ScreenCapture, String> {
    let region = desktop()?;
    capture(region.x, region.y, region.w, region.h)
}

/// Canonical pixels are converted to AppKit points explicitly. A spanning
/// window's own backing scale can change when most of it lies on another screen.
pub fn place_window(
    window: &tauri::WebviewWindow,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
) -> Result<(), String> {
    let scale = primary_scale();
    window
        .set_size(tauri::LogicalSize::new(
            f64::from(w) / scale,
            f64::from(h) / scale,
        ))
        .map_err(|e| e.to_string())?;
    position_window(window, x, y)
}
pub fn position_window(window: &tauri::WebviewWindow, x: i32, y: i32) -> Result<(), String> {
    let scale = primary_scale();
    window
        .set_position(tauri::LogicalPosition::new(
            f64::from(x) / scale,
            f64::from(y) / scale,
        ))
        .map_err(|e| e.to_string())
}

pub(crate) fn display_hz(display: u32) -> u32 {
    unsafe { pp_mac_display_hz(display) }
}
