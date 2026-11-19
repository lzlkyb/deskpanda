//! 本机设备名：**所有配对与发现路径必须报同一个名字**。
//!
//! 原仓在这里有一条守卫测试：全 `src/` 里除了本文件不许出现 `hostname::get()`。
//! 理由成立——配对界面显示的名字、附近设备列表里的名字、连接建立后对端记下的名字
//! 如果不是同一个来源，用户会看到「同一台机器两个名字」，而那种 bug 没有报错、
//! 只在对端列表里表现为多出一台设备。本仓保留这条守卫。

/// 本机设备名（取系统主机名，去首尾空白）。
///
/// 拿不到主机名时返回空串而不是编一个「Desktop-XXXX」：空串在界面上会显示成
/// 「未命名设备」，用户能改；编出来的名字会让用户以为那就是真的。
pub fn local_device_name() -> String {
    hostname::get()
        .map(|h| h.to_string_lossy().trim().to_string())
        .unwrap_or_default()
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
}
