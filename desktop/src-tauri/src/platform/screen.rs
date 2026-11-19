//! 屏幕枚举、抓帧（GDI 兜底）、光标位置、显示器工作区。
//!
//! # 谁在用
//!
//! - `rc::video::encode`：DXGI 拿不到帧时的**兜底**（独占全屏、受保护内容、
//!   显卡驱动拒绝复制），走这里的 [`capture_monitor_rgba`] 等 GDI 路径；
//! - `rc::dxgi` / `rc::input`：把「显示器序号」换算成桌面矩形，键鼠注入要用；
//! - `rc::ask_pop`：光标在哪块屏决定「别人敲门」提示出现在哪块屏。
//!
//! # 坐标系
//!
//! 全程**物理像素**，多显示器下是「虚拟屏幕桌面坐标」——左上角不一定是 (0,0)，
//! 主屏左侧的副屏会让原点变负。所以任何 `(x, y, w, h)` 都必须带原点一起传，
//! 不能假设从 0 开始。
//!
//! # 🔴 混合 DPI 下不要用整块 BitBlt 跨屏拷贝
//!
//! 从屏幕 DC 跨显示器拷像素时，混合 DPI 的虚拟屏会**原点错位**。
//! 所以整屏兜底是**逐显示器 BitBlt 再拼合**（[`capture_virtual_screen_rgba`]）：
//! 每块只触碰本显示器的 DC 区域，天然不存在跨屏拷贝。

use windows::Win32::Foundation::{BOOL, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, EnumDisplayMonitors,
    GetDC, GetMonitorInfoW, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS,
    HBITMAP, HDC, HGDIOBJ, HMONITOR, MONITORINFO, SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetSystemMetrics, SM_CXSCREEN, SM_CXVIRTUALSCREEN, SM_CYSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

/// 一台显示器的几何信息（远程多屏用）。
///
/// `Serialize + Deserialize`：caps 控制帧把它 JSON 过线到发起端，
/// 对端要能反序列化（发起端据此画出「显示器 1 / 2 / 3」的切换器）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MonitorInfo {
    /// 0 起，按 `EnumDisplayMonitors` 顺序；**主屏被调到 0**（前端「显示器1=主屏」）。
    pub index: i32,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub primary: bool,
}

/// 枚举所有显示器（远程多屏切换）。
pub fn list_monitors() -> Result<Vec<MonitorInfo>, String> {
    struct Ctx {
        list: Vec<MonitorInfo>,
    }

    unsafe extern "system" fn cb(
        hmon: HMONITOR,
        _hdc: HDC,
        _lprc: *mut RECT,
        lparam: LPARAM,
    ) -> BOOL {
        let ctx = &mut *(lparam.0 as *mut Ctx);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(hmon, &mut mi).as_bool() {
            let r = mi.rcMonitor;
            let primary = mi.dwFlags & 0x1 != 0; // MONITORINFOF_PRIMARY
            let idx = ctx.list.len() as i32;
            ctx.list.push(MonitorInfo {
                index: idx,
                x: r.left,
                y: r.top,
                w: (r.right - r.left).max(1),
                h: (r.bottom - r.top).max(1),
                primary,
            });
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
    // 主屏排到 index 0，方便前端「显示器1=主屏」
    if ctx.list.len() > 1 {
        if let Some(p) = ctx.list.iter().position(|m| m.primary) {
            ctx.list.swap(0, p);
            for (i, m) in ctx.list.iter_mut().enumerate() {
                m.index = i as i32;
            }
        }
    }
    if ctx.list.is_empty() {
        return Err("枚举显示器失败".into());
    }
    Ok(ctx.list)
}

/// 指定显示器的桌面矩形 `(x, y, w, h)`（与截帧同一坐标系，供键鼠映射）。
pub fn monitor_region(index: i32) -> Result<(i32, i32, i32, i32), String> {
    let list = list_monitors()?;
    let m = list
        .iter()
        .find(|m| m.index == index)
        .ok_or_else(|| format!("显示器 {index} 不存在"))?;
    Ok((m.x, m.y, m.w, m.h))
}

/// 光标当前位置（物理像素，虚拟屏幕坐标——与 [`monitor_region`] 同口径）。
pub fn get_cursor_pos() -> (i32, i32) {
    let mut pt = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut pt);
    }
    (pt.x, pt.y)
}

/// 虚拟屏幕尺寸与原点 `(width, height, origin_x, origin_y)`。
fn virtual_screen_metrics() -> (i32, i32, i32, i32) {
    unsafe {
        (
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
        )
    }
}

/// 抓指定矩形区域的 RGBA（物理像素，虚拟屏幕坐标）。**不做任何编码。**
///
/// 远程每 50~100ms 一帧，不能走 PNG 编码那条重路径，所以这里是裸像素。
///
/// # 🔴 GDI 清理必须与失败路径共用一段
///
/// 函数里有三处可能提前返回（`CreateDIBSection` / `BitBlt` / 后续编码）。
/// 把清理写在函数尾部的话，任一处 `?` 触发就会跳过它——而远程是**持续**调用，
/// 编码偶发失败会持续泄漏一个 DC 加一张 4K DIB（约 33MB），
/// GDI 对象有每进程配额，泄到上限后所有画面都会失败。
/// 所以可失败部分收进闭包，出来后无论成败都走同一段清理。
pub fn grab_rect_rgba(
    origin_x: i32,
    origin_y: i32,
    width: i32,
    height: i32,
) -> Result<Vec<u8>, String> {
    if width <= 0 || height <= 0 {
        return Err(format!("无效的抓帧区域尺寸: {width}x{height}"));
    }

    unsafe {
        let screen_dc = GetDC(hwnd_null());
        if screen_dc.0.is_null() {
            return Err("获取屏幕 DC 失败".to_string());
        }
        let mem_dc = CreateCompatibleDC(screen_dc);
        if mem_dc.0.is_null() {
            let _ = ReleaseDC(hwnd_null(), screen_dc);
            return Err("创建兼容 DC 失败".to_string());
        }

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height, // 负值 = top-down DIB，行序与内存一致
                biPlanes: 1,
                biBitCount: 32,
                biCompression: 0,
                biSizeImage: (width * height * 4) as u32,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut hbmp = HBITMAP::default();
        let mut old = HGDIOBJ::default();
        let result = (|| -> Result<Vec<u8>, String> {
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            hbmp = CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
                .map_err(|e| format!("创建 DIB 位图失败: {e}"))?;
            old = SelectObject(mem_dc, hbmp);
            // 检查返回值：BitBlt 失败（DC 失效 / 参数越界）若静默忽略，
            // 会拿未初始化的 DIB 当图，表现为黑屏 / 错位且零报错。
            if BitBlt(
                mem_dc, 0, 0, width, height, screen_dc, origin_x, origin_y, SRCCOPY,
            )
            .is_err()
            {
                return Err("BitBlt 拷贝屏幕像素失败".to_string());
            }

            // BGRA 像素 → RGBA（编码与降采样侧统一按 RGBA 处理）
            let len = (width as usize) * (height as usize) * 4;
            let bgra = std::slice::from_raw_parts(bits as *mut u8, len);
            let mut rgba = Vec::with_capacity(len);
            for px in bgra.chunks_exact(4) {
                rgba.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
            }
            Ok(rgba)
        })();

        // 清理 GDI 资源（先还原对象再删位图，避免悬挂）——成功与失败路径共用
        if !hbmp.0.is_null() {
            let _ = SelectObject(mem_dc, old);
            let _ = DeleteObject(hbmp);
        }
        let _ = DeleteDC(mem_dc);
        let _ = ReleaseDC(hwnd_null(), screen_dc);
        result
    }
}

/// `GetDC(NULL)` 用的空窗口句柄。
fn hwnd_null() -> windows::Win32::Foundation::HWND {
    windows::Win32::Foundation::HWND(std::ptr::null_mut())
}

/// 指定显示器的 RGBA（不编码）。
pub fn capture_monitor_rgba(index: i32) -> Result<(i32, i32, Vec<u8>), String> {
    let list = list_monitors()?;
    let m = list
        .iter()
        .find(|m| m.index == index)
        .ok_or_else(|| format!("显示器 {index} 不存在"))?;
    let rgba = grab_rect_rgba(m.x, m.y, m.w, m.h)?;
    Ok((m.w, m.h, rgba))
}

/// 整个虚拟屏幕的 RGBA（多显示器拼合）。
///
/// 🔴 逐显示器 BitBlt 再拼合，理由见模块文档（混合 DPI 下整块 BitBlt 会错位）。
pub fn capture_virtual_screen_rgba() -> Result<(i32, i32, Vec<u8>), String> {
    let (width, height, origin_x, origin_y) = virtual_screen_metrics();
    if width <= 0 || height <= 0 {
        return Err(format!("获取虚拟屏幕尺寸失败: {width}x{height}"));
    }
    let monitors = list_monitors()?;
    let cw = width as usize;
    let ch = height as usize;
    let mut canvas = vec![0u8; cw * ch * 4];
    for m in &monitors {
        let rgba = grab_rect_rgba(m.x, m.y, m.w, m.h)?;
        // 桌面坐标 → 画布局部坐标；越界部分（理论不会有）裁掉
        let ox = (m.x - origin_x).max(0) as usize;
        let oy = (m.y - origin_y).max(0) as usize;
        if ox >= cw || oy >= ch {
            continue;
        }
        let copy_w = (m.w as usize).min(cw - ox);
        let copy_h = (m.h as usize).min(ch - oy);
        if copy_w == 0 || copy_h == 0 {
            continue;
        }
        let m_w = m.w as usize;
        for y in 0..copy_h {
            let src = &rgba[y * m_w * 4..y * m_w * 4 + copy_w * 4];
            let dst = (oy + y) * cw + ox;
            canvas[dst * 4..dst * 4 + copy_w * 4].copy_from_slice(src);
        }
    }
    Ok((width, height, canvas))
}

/// 仅主屏的 RGBA（单显示器场景更省编码）。
pub fn capture_primary_screen_rgba() -> Result<(i32, i32, Vec<u8>), String> {
    let (width, height) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    if width <= 0 || height <= 0 {
        return Err(format!("获取主屏尺寸失败: {width}x{height}"));
    }
    let rgba = grab_rect_rgba(0, 0, width, height)?;
    Ok((width, height, rgba))
}

/// 显示器工作区（物理像素，含原点）+ 该屏缩放因子。
pub struct MonitorWorkArea {
    pub work_x: f64,
    pub work_y: f64,
    pub work_w: f64,
    pub work_h: f64,
    pub scale: f64,
}

/// 获取包含指定物理坐标点的显示器的工作区（排除任务栏，带原点）与缩放因子。
///
/// 用 `MonitorFromPoint(MONITOR_DEFAULTTONEAREST)`（点不在任何屏上时返回最近屏）
/// + `GetMonitorInfoW(rcWork)` + `GetDpiForMonitor`（每监视器 DPI）。
///
/// # 为什么不简化
///
/// - 只用 `SPI_GETWORKAREA` 取的是**主显示器**工作区且丢弃原点，多屏 /
///   任务栏在上或左侧时坐标系直接错位；
/// - 把「逻辑」尺寸当「物理」尺寸参与物理坐标钳制，高 DPI（125%/150%）屏上
///   弹窗实际物理高度 = 逻辑×缩放，按逻辑高度钳制后底部仍会超出屏幕。
pub fn get_monitor_work_area(px: f64, py: f64) -> MonitorWorkArea {
    use windows::Win32::Graphics::Gdi::{MonitorFromPoint, MONITOR_DEFAULTTONEAREST};
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};

    let pt = POINT {
        x: px as i32,
        y: py as i32,
    };
    let hmonitor = unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST) };

    let mut mi = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let mut scale = 1.0_f64;
    unsafe {
        let _ = GetMonitorInfoW(hmonitor, &mut mi);
        let mut dpi_x: u32 = 96;
        let mut dpi_y: u32 = 96;
        if GetDpiForMonitor(hmonitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y).is_ok()
            && dpi_x > 0
        {
            scale = dpi_x as f64 / 96.0;
        }
    }

    let work = mi.rcWork;
    // 退化矩形兜底，避免弹窗被定位到屏幕外
    if work.right <= work.left || work.bottom <= work.top {
        log::warn!(
            "[Screen] 获取显示器工作区失败，使用默认 1920x1080 @ scale {}",
            scale
        );
        return MonitorWorkArea {
            work_x: 0.0,
            work_y: 0.0,
            work_w: 1920.0,
            work_h: 1080.0,
            scale,
        };
    }
    MonitorWorkArea {
        work_x: work.left as f64,
        work_y: work.top as f64,
        work_w: (work.right - work.left) as f64,
        work_h: (work.bottom - work.top) as f64,
        scale,
    }
}
