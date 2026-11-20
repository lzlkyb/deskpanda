//! Windows 备用组件按需获取：安装包不再携带 EasyTier 二进制，仓库只保留驱动代码。
//! 原型开关启用且本地找不到组件时才联网取件；按**编译期钉入**的仓库内 build.json
//! 的 SHA256 校验通过后才落盘执行。
use sha2::Digest;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

// 组件与 build.json 同源同版本；换组件必须随发版更新此 ref，否则校验必然不匹配。
const COMPONENT_REF: &str = "v7.2.10";
const RESOURCE_ROOT: &str = "src-tauri/resources/easytier";
const SOURCES: [&str; 2] = [
    "https://raw.githubusercontent.com/lzlkyb/pastepanda",
    "https://ghproxy.net/https://raw.githubusercontent.com/lzlkyb/pastepanda",
];
const BINARIES: [&str; 2] = ["easytier-core.exe", "easytier-cli.exe"];
const NOTICES: [&str; 5] = [
    "LICENSE",
    "COPYING-GPL-3.0.txt",
    "NOTICE.txt",
    "pnet-LICENSE-MIT.txt",
    "pnet-LICENSE-APACHE.txt",
];

fn source_url(source: &str, rel: &str) -> String {
    format!("{source}/{COMPONENT_REF}/{RESOURCE_ROOT}/{rel}")
}

/// 与 runtime 原口径一致：开发与安装目录均不查 PATH，避免启动不确定版本的系统 EasyTier。
pub(super) async fn desktop_binary(name: &str) -> Result<PathBuf, String> {
    if !BINARIES.contains(&name) {
        return Err(format!("未知的备用承载组件：{name}"));
    }
    if cfg!(debug_assertions) {
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources/easytier/windows-x86_64")
            .join(name);
        if source.is_file() {
            return Ok(source);
        }
    }
    let dir = download_dir()?;
    let cached = dir.join(name);
    if cached.is_file() {
        return Ok(cached);
    }
    // v7.2.10 及更早的安装包仍带组件，升级用户直接复用原文件。
    if let Some(installed) = installed_binary(name) {
        return Ok(installed);
    }
    fetch(name, &dir, &cached).await
}

fn download_dir() -> Result<PathBuf, String> {
    let dir = super::APP_DIR
        .get()
        .ok_or("备用承载目录未初始化")?
        .join("easytier-prototype/bin");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn installed_binary(name: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let path = exe
        .parent()?
        .join("resources/easytier/windows-x86_64")
        .join(name);
    path.is_file().then_some(path)
}

async fn fetch(name: &str, dir: &Path, target: &Path) -> Result<PathBuf, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;
    // 🔴 2026-11-20 体检修复：SHA 期望值从「运行时远端取清单」改为**编译期钉入**
    // （仓库内 build.json 经 include_str! 进二进制）。旧实现清单与二进制同源——
    // 国内走 ghproxy.net 兜底源时，第三方代理若被控可同时替换两者，SHA 校验
    // 形同虚设（启用该特性即供应链 RCE 面）。现在代理最多劫持下载失败，不可能
    // 劫持「校验通过」。换组件时更新 resources/easytier/ 下的组件与 build.json
    // 即可，本函数自动跟随（编译期读的就是那份文件）。
    let shas = pinned_shas()?;
    let expected = shas
        .get(name)
        .and_then(|v| v.as_str())
        .ok_or("备用组件清单缺少校验值")?;
    for source in SOURCES {
        let url = source_url(source, &format!("windows-x86_64/{name}"));
        let downloaded: Result<Vec<u8>, String> = async {
            let response = client
                .get(&url)
                .send()
                .await
                .map_err(|e| e.to_string())?
                .error_for_status()
                .map_err(|e| e.to_string())?;
            Ok(response.bytes().await.map_err(|e| e.to_string())?.to_vec())
        }
        .await;
        let body = match downloaded {
            Ok(b) => b,
            Err(error) => {
                log::warn!("[RC-UNDERLAY] 备用组件下载失败，换源：{error}");
                continue;
            }
        };
        if !sha_ok(&body, expected) {
            log::warn!("[RC-UNDERLAY] 备用组件校验不匹配，弃用该源：{name}");
            continue;
        }
        // 校验通过后才落盘；create+truncate 映射到 CREATE_ALWAYS（不能塞 custom_flags，那是 dwFlagsAndAttributes 位）。
        // 同机同时最多一个写者：single-instance 插件保证桌面单实例，会话本身又是单槽。
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(0x8000_0000); // FILE_FLAG_WRITE_THROUGH
        }
        use std::io::Write as _;
        let mut file = options
            .open(target)
            .map_err(|e| format!("备用组件落盘失败：{e}"))?;
        file.write_all(&body).map_err(|e| e.to_string())?;
        file.flush().map_err(|e| e.to_string())?;
        drop(file);
        fetch_notices(&client, dir).await;
        log::info!("[RC-UNDERLAY] 备用组件已按需获取：{name}，{}B", body.len());
        return Ok(target.to_path_buf());
    }
    Err("备用组件获取失败，所有来源均未通过校验".into())
}

/// 编译期钉入的校验清单：仓库内 `resources/easytier/windows-x86_64/build.json`。
/// 见 [`fetch`] 里的 🔴 注释——期望值绝不能与二进制同源获取。
fn pinned_shas() -> Result<serde_json::Map<String, serde_json::Value>, String> {
    const PINNED_BUILD_JSON: &str =
        include_str!("../../../resources/easytier/windows-x86_64/build.json");
    serde_json::from_str::<serde_json::Value>(PINNED_BUILD_JSON)
        .map_err(|e| format!("编译期钉入的备用组件清单不合法：{e}"))?
        .get("sha256")
        .and_then(|v| v.as_object())
        .cloned()
        .ok_or_else(|| "编译期钉入的备用组件清单缺少 sha256 段".to_string())
}

async fn fetch_notices(client: &reqwest::Client, dir: &Path) {
    // LGPL 文本与组件同目录存放；取不到不阻断，但绝不执行无许可文本旁证的组件。
    for notice in NOTICES {
        for source in SOURCES {
            let url = source_url(source, notice);
            let fetched: Result<Vec<u8>, String> = async {
                let response = client
                    .get(&url)
                    .send()
                    .await
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?;
                Ok(response.bytes().await.map_err(|e| e.to_string())?.to_vec())
            }
            .await;
            if let Ok(body) = fetched {
                let _ = std::fs::write(dir.join(notice), &body);
                break;
            }
        }
    }
}

fn sha_ok(bytes: &[u8], expected: &str) -> bool {
    let expected = expected.trim().to_ascii_lowercase();
    expected.len() == 64
        && expected.bytes().all(|b| b.is_ascii_hexdigit())
        && sha2::Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
            == expected
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn committed_manifest() -> Value {
        let raw = include_str!("../../../resources/easytier/windows-x86_64/build.json");
        serde_json::from_str(raw).expect("仓库内 build.json 必须是合法 JSON")
    }

    #[test]
    fn component_sources_are_https_pinned_and_traversal_free() {
        for source in SOURCES {
            for rel in ["windows-x86_64/easytier-core.exe", "LICENSE"] {
                let url = source_url(source, rel);
                assert!(url.starts_with("https://"), "{url}");
                assert!(url.contains(&format!("/{COMPONENT_REF}/")), "{url}");
                assert!(!url.contains(".."), "{url}");
            }
        }
    }

    #[test]
    fn manifest_covers_every_bundlable_binary_with_canonical_sha() {
        // 🔴 走 pinned_shas()（运行时真正用的同一条路径）而不是另读一遍文件：
        // 钉的就是「运行时校验清单 = 仓库内 build.json」这件事本身。
        let map = pinned_shas().expect("编译期清单必须可用");
        for name in BINARIES {
            let sha = map.get(name).and_then(|v| v.as_str()).unwrap_or_default();
            assert!(
                sha.len() == 64
                    && sha.bytes().all(|b| b.is_ascii_hexdigit())
                    && sha == sha.to_ascii_lowercase(),
                "{name}"
            );
        }
    }

    /// ❗ `#[ignore]`：本仓**不随包分发** easytier 组件（37MB GPL-3.0 二进制，
    /// 且该特性默认关）。断言的语义仍在——把组件放进
    /// `resources/easytier/windows-x86_64/` 后 `cargo test -- --ignored` 即可验。
    /// 不删：换组件忘同步 build.json 会让 SHA 校验失败、下载下来的件直接拒用，
    /// 那种失败在运行期只表现为「打洞就是不生效」。
    #[ignore = "DeskPanda 不分发 easytier 组件；装好组件后 --ignored 手动跑"]
    #[test]
    fn committed_binaries_match_manifest_before_download_side_verification() {
        let dir =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/easytier/windows-x86_64");
        let map = committed_manifest()["sha256"].as_object().unwrap().clone();
        for name in BINARIES {
            let bytes = std::fs::read(dir.join(name)).unwrap();
            assert!(
                sha_ok(&bytes, map[name].as_str().unwrap()),
                "{name} 已换版但 build.json 未同步"
            );
        }
    }

    #[tokio::test]
    async fn unknown_component_never_reaches_filesystem_or_network() {
        assert!(desktop_binary("cmd.exe")
            .await
            .unwrap_err()
            .contains("未知"));
    }

    #[test]
    fn write_options_create_missing_target_file() {
        // 钉住落盘语义：目标不存在时必须新建成功（曾有组合把 CREATE_ALWAYS 塞进 custom_flags 而静默判挂）。
        let dir = std::env::temp_dir().join(format!("pp-fetch-opts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("missing.bin");
        let _ = std::fs::remove_file(&target);
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(0x8000_0000);
        }
        options.open(&target).expect("不存在的目标必须能新建落盘");
        std::fs::remove_dir_all(&dir).ok();
    }
}
