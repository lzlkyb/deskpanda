/**
 * 剪贴板读写（原生通道）。
 *
 * 为什么不用 Web API：`navigator.clipboard.writeText` 在 WebView2 里会 **resolve
 * 却不写入**（判据：复制「成功」≠ 剪贴板真的变了），界面显示「已复制」而粘出来
 * 是旧内容；`readText` 还会弹「是否允许读取剪贴板」的浏览器权限框。
 *
 * 复制请优先用 `lib/utils.ts` 的 `copyToClipboard`（它带「回读不一致」判定与
 * 受控回落）；这里只提供最薄的两个原语。
 */
import { invoke } from "@tauri-apps/api/core";
import { logger } from "@/lib/logger";

/**
 * 读剪贴板纯文本。读不到返回空串——调用方拿它预填，不该阻断流程。
 */
export async function readClipboardText(): Promise<string> {
  try {
    return await invoke<string>("read_clipboard_text");
  } catch (e) {
    logger.error("读取剪贴板失败", e);
    return "";
  }
}

/**
 * 写纯文本到剪贴板。写入后由 Rust 侧回读校验，不一致会抛错。
 */
export async function copyOnly(text: string): Promise<void> {
  await invoke("copy_only", { text });
}
