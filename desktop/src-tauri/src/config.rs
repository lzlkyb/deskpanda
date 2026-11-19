//! 应用配置：JSON 文件持久化，落在 `app_config_dir()/config.json`。
//!
//! 为何用 JSON 文件而不是 SQLite：壳子不预设业务需要关系库；JSON 足够表达
//! 「少量键值 + 少量结构化字段」，且省掉 rusqlite(bundled) 的编译时间与体积。
//! 业务真需要数据库时，在此模块旁新增 `db.rs`（参考典型 rusqlite 嵌入式用法），
//! 不要把配置也搬进去——配置是启动时第一件要读的东西，越简单越可靠。
//!
//! 硬性规则：
//! - **字段只增不改不删语义**：老版本读新配置时未知字段忽略（serde 默认行为），
//!   所以新增字段必须带 `#[serde(default)]`，否则升级后旧配置文件反序列化直接失败。
//! - 写盘用「临时文件 + rename」：半截配置写坏比没有配置更致命。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 应用配置。字段即「跨重启需要记住的设置」；一次性状态不要放这里。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// 关窗时释放界面内存（销毁 webview 进程组，实测省约 80MB 专用工作集；
    /// 代价是重开窗口要重新加载前端 1~2 秒）。由托盘/双击图标重建窗口。
    #[serde(default = "default_true")]
    pub release_webview_on_close: bool,
    /// 开机自启（与 tauri-plugin-autostart 同步的镜像值，供前端读取）。
    #[serde(default)]
    pub autostart_enabled: bool,

    // ── 远程会话（rc）偏好 ────────────────────────────────────────────────
    // 这三项**只影响本机界面行为**，不参与任何协议与鉴权判断，后端也不读它们
    // （`rc_enabled` / `rc_keep_awake` 是另一套：真值在 SQLite `config` 表，
    // 由 `rc_set_enabled` / `rc_set_keep_awake` 命令写，别往这里塞）。
    //
    // 放在这里的理由：前端 `save_config` 一次提交整份 config，若结构体不认识这些
    // 键，serde 会在反序列化时静默丢弃 → 写盘后重启偏好全丢（实测过）。所以字段
    // 必须在结构体里「登记」，值由前端算。
    /// 会话内剪贴板自动同步（默认开）。读取口径见前端 `lib/rcClipAuto.ts`。
    #[serde(default = "default_true")]
    pub rc_clip_auto: bool,
    /// 控端浮条顶缘 hover 唤出（默认开）。读取口径见前端 `lib/rcHoverReveal.ts`。
    #[serde(default = "default_true")]
    pub rc_hover_reveal: bool,
    /// 控端键盘模式：`type`（打字，可输中文，默认）/ `direct`（直传扫描码）。
    /// 非法值由前端 `lib/rcKeyMode.ts` 的 `rcKeyModeOf` 收敛，这里存字符串不改语义。
    #[serde(default = "default_rc_key_mode")]
    pub rc_key_mode: String,
}

fn default_rc_key_mode() -> String {
    "type".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            release_webview_on_close: true,
            autostart_enabled: false,
            rc_clip_auto: true,
            rc_hover_reveal: true,
            rc_key_mode: default_rc_key_mode(),
        }
    }
}

/// 配置文件路径：`{config_dir}/config.json`。
pub fn config_path(config_dir: &std::path::Path) -> PathBuf {
    config_dir.join("config.json")
}

/// 读配置；文件不存在或损坏时回落默认值（损坏时留警告，不静默）。
pub fn load_config(config_dir: &std::path::Path) -> AppConfig {
    let path = config_path(config_dir);
    match std::fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<AppConfig>(&text) {
            Ok(cfg) => cfg,
            Err(e) => {
                log::warn!("配置文件损坏，使用默认值（{path:?}: {e}）");
                AppConfig::default()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => AppConfig::default(),
        Err(e) => {
            log::warn!("读取配置失败，使用默认值（{path:?}: {e}）");
            AppConfig::default()
        }
    }
}

/// 写配置：先写临时文件再 rename，避免半截文件。
///
/// 临时文件名带**进程 id + 进程内自增序号**，不用固定的 `config.json.tmp`：
/// 固定名下两个并发的 save_config（前端连点两次开关就会发生）会互相覆盖对方的
/// 半截内容，且 rename 时序错乱时可能把空/坏文件换成正式配置——「读起来还是
/// 默认值，但用户明明改过」这种灵异故障就是这么来的。写失败时清掉残骸，
/// 不在配置目录留 `.tmp` 垃圾。
pub fn save_config(config_dir: &std::path::Path, cfg: &AppConfig) -> Result<(), String> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static TMP_SEQ: AtomicU32 = AtomicU32::new(0);

    let path = config_path(config_dir);
    let tmp = config_dir.join(format!(
        "config.json.{}.{}.tmp",
        std::process::id(),
        TMP_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let text = serde_json::to_string_pretty(cfg).map_err(|e| format!("序列化配置失败: {e}"))?;
    let write_result = std::fs::write(&tmp, text).map_err(|e| format!("写临时配置失败: {e}"));
    if let Err(e) = write_result {
        let _ = std::fs::remove_file(&tmp); // 清残骸（可能压根没写进去）
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&tmp, &path) {
        let _ = std::fs::remove_file(&tmp); // rename 失败（跨卷/权限），别留垃圾
        return Err(format!("替换配置失败: {e}"));
    }
    Ok(())
}
