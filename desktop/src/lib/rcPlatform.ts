/**
 * rc 域的平台判据——「远程电脑功能在本平台交付不交付」的唯一判断处。
 *
 * 平台值来自后端 `get_app_info` 的 `platform` 字段（Rust `std::env::consts::OS`，
 * 编译期常量，恒小写：`windows` / `macos` / `linux`）。
 *
 * 🔴 判据口径（S6 交付后，2026-11-21 起）：桌面双平台（Windows + macOS）。
 * macOS 侧的采集（ScreenCaptureKit）/ 编码（VideoToolbox + AV1 dylib）/
 * 音频（CoreAudio）/ 输入注入（CGEvent）桥接自原仓迁入（`src-tauri/src/macos/`），
 * rc 命令与 Windows 同段注册。其余平台（linux 等）Rust 侧 rc 命令不在
 * `invoke_handler` 中，前端 invoke 一律失败 ⇒ 降级。
 * 见 `docs/DeskPanda-拆分方案-2026-11-19.md` 的 S6 阶段。
 *
 * 🔴 为什么 `undefined` 不当成降级：首帧（`get_app_info` 返回前）与读取失败
 * 都是 undefined。此时按「非降级」渲染——Windows 首帧不会闪一下降级提示；
 * 代价是其余平台首帧会先渲染一帧完整工作台再换成通知卡，属一次性闪烁，可接受。
 * 口径与壳子 `lib/platform.ts` 的首帧取舍一致：宁可错一帧，不误展示不可用功能。
 *
 * 为何不复用 `lib/platform.ts`：那个文件在本仓零前端消费方（消费方是未迁移的
 * 壳子设置页），且其头部注释指向本仓不存在的 `get_status`。rc 域独立成文件，
 * 与 `rcWorkbench.ts` / `rcRowGate.ts` 的域内组织一致；`platform.ts` 的去留
 * 并入死代码清理任务处理，不与功能接线耦合。
 */

/** 远程电脑功能是否在本平台交付（S6 起桌面双平台：Windows 与 macOS）。 */
export function rcPlatformSupported(platform: string | undefined): boolean {
  return platform === "windows" || platform === "macos";
}

/**
 * 是否进入「平台不支持」降级态：读到了平台值、且不在交付平台内。
 * `undefined`（未读到）不算降级——见文件头首帧取舍。
 */
export function rcPlatformDegraded(platform: string | undefined): boolean {
  return platform !== undefined && !rcPlatformSupported(platform);
}
