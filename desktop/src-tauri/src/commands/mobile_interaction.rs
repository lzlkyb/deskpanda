//! 手机端交互（2026-11-20 迁自 pastePanda `commands/mobile_interaction.rs`）。
//!
//! 两条命令都是「壳子级」：桌面构建下 `init()` 是空 setup、命令体直接 Ok——
//! 与 `rc::keepalive` / `update_android` 同款「桌面 no-op」处理，因此
//! `app.rs` 的插件链不必写平台 cfg。
//!
//! # kind 白名单
//!
//! `mobile_interaction_haptic` 只放行 `ready` / `confirm` / `reject` 三种触感：
//! 白名单在 Rust 侧而不是 Kotlin 侧，前端传错值时报的是「不支持的触感类型」
//! 而不是 Kotlin 的 IllegalArgumentException（后者到前端只剩一句 opaque error）。

#[cfg(target_os = "android")]
use tauri::Manager;
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Runtime,
};

#[cfg(target_os = "android")]
pub struct MobileInteraction<R: Runtime>(tauri::plugin::PluginHandle<R>);

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("mobile-interaction")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            app.manage(MobileInteraction(api.register_android_plugin(
                "com.pastepanda.deskpanda",
                "MobileInteractionPlugin",
            )?));
            let _ = (app, api);
            Ok(())
        })
        .build()
}

#[tauri::command]
pub async fn mobile_interaction_set(app: AppHandle, enabled: bool) -> Result<(), String> {
    #[cfg(target_os = "android")]
    return app
        .state::<MobileInteraction<tauri::Wry>>()
        .0
        .run_mobile_plugin::<()>("setEnabled", serde_json::json!({ "enabled": enabled }))
        .map_err(|_| "手机返回交互未就绪".to_string());
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, enabled);
        Ok(())
    }
}

#[tauri::command]
pub async fn mobile_interaction_haptic(app: AppHandle, kind: String) -> Result<(), String> {
    if !matches!(kind.as_str(), "ready" | "confirm" | "reject") {
        return Err("不支持的触感类型".into());
    }
    #[cfg(target_os = "android")]
    return app
        .state::<MobileInteraction<tauri::Wry>>()
        .0
        .run_mobile_plugin::<()>("haptic", serde_json::json!({ "kind": kind }))
        .map_err(|_| "手机触感反馈不可用".to_string());
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(())
    }
}
