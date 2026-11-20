//! 本机设备名：**所有配对与发现路径必须报同一个名字**。
//!
//! 原仓在这里有一条守卫测试：全 `src/` 里除了本文件不许出现 `hostname::get()`。
//! 理由成立——配对界面显示的名字、附近设备列表里的名字、连接建立后对端记下的名字
//! 如果不是同一个来源，用户会看到「同一台机器两个名字」，而那种 bug 没有报错、
//! 只在对端列表里表现为多出一台设备。本仓保留这条守卫。
//!
//! # Android 分支
//!
//! 手机端没有「主机名」这个概念（`gethostname()` 在 Android 上恒返回
//! `localhost`），报出去的就是「localhost」这种假名字。所以 Android 走系统属性：
//! 用户自己改过的设备名 > 厂商市场名 > 厂商 + 型号，优先级见 [`choose_android_name`]。

/// 本机设备名（取系统主机名，去首尾空白）。
///
/// 拿不到主机名时返回空串而不是编一个「Desktop-XXXX」：空串在界面上会显示成
/// 「未命名设备」，用户能改；编出来的名字会让用户以为那就是真的。
pub fn local_device_name() -> String {
    #[cfg(target_os = "android")]
    {
        return android_device_name();
    }
    #[cfg(not(target_os = "android"))]
    hostname::get()
        .map(|h| h.to_string_lossy().trim().to_string())
        .unwrap_or_default()
}

/// Android 设备名的择优（纯函数，双端编译供桌面单测——与拆分前
/// `pastePanda/device_identity.rs` 同一份逻辑，迁入时连测试一起搬）。
///
/// 优先级：用户自定义名（`persist.sys.device_name`，设置里可改）>
/// 市场名（`ro.product.marketname`，如「Redmi K80」）> 厂商 + 型号。
/// 第三条里型号已含厂商名时不再重复拼（`Samsung SM-S9310` 不变成
/// `Samsung Samsung SM-S9310`）；三者全空时回「Android 手机」而不是空串——
/// 配对列表里一台没有名字的设备等于不存在。
#[cfg(any(target_os = "android", test))]
fn choose_android_name(custom: &str, market: &str, manufacturer: &str, model: &str) -> String {
    for name in [custom, market] {
        if !name.trim().is_empty() {
            return name.trim().to_string();
        }
    }
    let model = model.trim();
    let maker = manufacturer.trim();
    if model.is_empty() {
        return "Android 手机".into();
    }
    if maker.is_empty() || model.to_lowercase().starts_with(&maker.to_lowercase()) {
        model.into()
    } else {
        format!("{maker} {model}")
    }
}

/// Android 设备名：经 `__system_property_get` 读系统属性（bionic 的 C API，
/// 没有 Rust 绑定，直接 extern 声明；Rust 侧零依赖）。
#[cfg(target_os = "android")]
fn android_device_name() -> String {
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_int};
    extern "C" {
        fn __system_property_get(name: *const c_char, value: *mut c_char) -> c_int;
    }
    let property = |key: &str| {
        let key = CString::new(key).expect("static property key");
        // Android's native API requires PROP_VALUE_MAX (92), including the NUL.
        let mut value = [0 as c_char; 92];
        unsafe {
            if __system_property_get(key.as_ptr(), value.as_mut_ptr()) <= 0 {
                return String::new();
            }
            CStr::from_ptr(value.as_ptr())
                .to_string_lossy()
                .trim()
                .to_string()
        }
    };
    choose_android_name(
        &property("persist.sys.device_name"),
        &property("ro.product.marketname"),
        &property("ro.product.manufacturer"),
        &property("ro.product.model"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 配对路径不得绕过本模块直接取主机名() {
        fn visit(path: &std::path::Path) {
            for entry in std::fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    visit(&path);
                } else if path.extension().and_then(|s| s.to_str()) == Some("rs")
                    && path.file_name().unwrap() != "device.rs"
                {
                    let source = std::fs::read_to_string(&path).unwrap();
                    let call = ["hostname::", "get()"].concat();
                    assert!(
                        !source.lines().any(
                            |line| !line.trim_start().starts_with("//") && line.contains(&call)
                        ),
                        "{} 绕过了本模块直接取主机名——同一台机器会出现两个名字",
                        path.display()
                    );
                }
            }
        }
        visit(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
    }

    #[test]
    fn 设备名不带首尾空白() {
        let n = local_device_name();
        assert_eq!(n, n.trim());
    }

    /// 优先级逐条钉死：自定义名赢过市场名、市场名赢过厂商+型号、
    /// 型号已含厂商名时不重复拼、全空时回兜底名而不是空串。
    ///
    /// 这条是纯函数测试（不读系统属性），在桌面 CI 上也能跑——Android 命名
    /// 逻辑的正确性不能只靠真机验收，那太晚了。
    #[test]
    fn android_name_prefers_custom_then_market_then_manufacturer_model() {
        assert_eq!(
            choose_android_name(" 我的手机 ", "Pixel 9", "Google", "Pixel 9"),
            "我的手机"
        );
        assert_eq!(
            choose_android_name("", " Redmi K80 ", "Xiaomi", "24122RKC7C"),
            "Redmi K80"
        );
        assert_eq!(
            choose_android_name("", "", "Google", "Pixel 9"),
            "Google Pixel 9"
        );
        // 型号已含厂商名（大小写不敏感）⇒ 不重复拼
        assert_eq!(
            choose_android_name("", "", "Samsung", "Samsung SM-S9310"),
            "Samsung SM-S9310"
        );
        assert_eq!(choose_android_name("", "", "", ""), "Android 手机");
    }
}
