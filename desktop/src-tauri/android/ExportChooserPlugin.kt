package com.pastepanda.deskpanda

import android.app.Activity
import android.content.Intent
import androidx.activity.result.ActivityResult
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

@InvokeArg class ExportChooserArgs { var path: String = ""; var action: String = "" }

/**
 * 手机端「打开 / 分享 / 导出」的单一出口（2026-11-20，替代知识库分享插件）。
 *
 * Rust 侧 `commands::mobile_export` 经 `register_android_plugin` 按类名反射装载，
 * 契约只有一个方法：`receivedFile({path, action})`——动作语义在
 * [ReceivedFileActions]（路径必须落在 cache/received-file-out/<uuid>/ 下，
 * 经 FileProvider 授权单文件）。本类只做两件插件机制该做的事：
 * 把 Invoke 递给帮助类，以及给「导出到其它应用」提供 ActivityResultLauncher。
 */
@TauriPlugin
class ExportChooserPlugin(private val activity: Activity) : Plugin(activity) {
    private val receivedFiles by lazy { ReceivedFileActions(activity) }

    @Command
    fun receivedFile(invoke: Invoke) { receivedFiles.start(invoke, this) }

    @ActivityCallback
    fun receivedFileExport(invoke: Invoke, result: ActivityResult) { receivedFiles.exportResult(invoke, result) }

    fun chooseExport(invoke: Invoke, intent: Intent) { startActivityForResult(invoke, intent, "receivedFileExport") }
}
