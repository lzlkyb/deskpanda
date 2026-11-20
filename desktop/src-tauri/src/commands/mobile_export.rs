//! 手机端「导出 / 分享」选择器插件（2026-11-20 新建，替代知识库分享插件）。
//!
//! # 为什么是这一个文件而不是迁 KnowledgeShare
//!
//! 原仓 `mobile_received_file.rs` 的文件动作经 `mobile_knowledge_share::
//! KnowledgeShare`（知识库分享插件）递到 Kotlin 侧，而**知识库不在本仓范围**。
//! 抽掉它的代价如果落成「在 received_file 里内联一段 JNI 注册」，
//! 就是把插件生命周期（manage / try_state）和业务命令耦在一起——下一个
//! 消费方（比如以后的「导出到其它应用」）还得再抄一遍。所以这里留一个
//! ≤30 行的纯装载层：契约只有 `receivedFile({path, action})` 一个方法，
//! Kotlin 侧对应 `ExportChooserPlugin`（系统分享面板 / 打开方式）。
//!
//! 桌面 no-op：`init()` 是空 setup，`MobileExport` 类型只在 Android 存在
//! （与 `rc::keepalive` 同款处理），因此 app.rs 的插件链不必写平台 cfg。

#[cfg(target_os = "android")]
use tauri::Manager;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime,
};

// 🔴 字段必须 `pub`：`mobile_received_file.rs` 的命令体（Android 分支）直接
// `app.state::<MobileExport<tauri::Wry>>().0.run_mobile_plugin(...)` 拿
// PluginHandle 发 JNI。私有字段在那里是 E0616（2026-11-20 Android check 实测）。
#[cfg(target_os = "android")]
pub struct MobileExport<R: Runtime>(pub tauri::plugin::PluginHandle<R>);

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("mobile-export")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            app.manage(MobileExport(api.register_android_plugin(
                "com.pastepanda.deskpanda",
                "ExportChooserPlugin",
            )?));
            let _ = (app, api);
            Ok(())
        })
        .build()
}

#[cfg(test)]
mod tests {
    /// 桌面构建里 `init()` 必须存在且可调用（插件链无平台分支的前提）。
    /// 真实验证在 Android 侧：注册失败时 setup 返回 Err，应用启动即报。
    #[test]
    fn desktop_init_is_callable() {
        let _ = super::init::<tauri::Wry>();
    }
}
