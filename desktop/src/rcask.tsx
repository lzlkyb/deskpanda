import { setMotionPreference } from "./lib/motionPreference";
import { MotionPolicy } from "./components/MotionPolicy";
/**
 * rcask —— 入站远程申请**置顶浮层**（窗口 `rc-ask`）的独立入口。
 *
 * 它存在的理由：告知本来就有（主窗里的申请卡 + toast），但主窗是非置顶的、而且可能
 * 整场都没打开——用户正被全屏应用盖着时，120s 确认窗就在没人看见的地方走完。这块
 * 浮层只做一件事：把「有人要远程这台电脑」顶到手头的活之上，答完就消失。
 *
 * 🔴 保持极简：**独立 webview、独立 JS 上下文**，主窗口的 store 与 DOM 事件一个都拿
 * 不到。状态只能挂载时 `rc_ask_state()` 主动问一次，之后跟着 `rc-session-changed` 重拉。
 * 唯一例外是主题：`get_config` 是壳子的全平台命令，浮层压在别的应用之上，必须跟系统
 * 主题走（浅色主题下深玻璃会糊成一块黑）。
 */
import React from "react";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { WindowCrashFallback } from "./components/WindowCrashFallback";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { RcAskPop } from "./components/rc/RcAskPop";
import { logger } from "./lib/logger";
import { applyTheme, DEFAULT_THEME, normalizeTheme } from "./lib/theme";
import "./styles/globals.css";
import "./styles/theme.css";
import "./styles/buttons.css";

window.addEventListener("error", (event) => {
  logger.error("入站申请浮层未捕获错误", event.error || event.message);
});
window.addEventListener("unhandledrejection", (event) => {
  logger.error("入站申请浮层未处理的 Promise 拒绝", event.reason);
  event.preventDefault();
});

// 先按默认主题上色（防首帧闪），再用后端配置校正。
// 本产品单窗口阶段没有跨窗口 `theme-changed` 广播（见 main.tsx 的注释），
// 所以这里只读一次——浮层存活时间以秒计，主题中途变更的概率可以忽略。
applyTheme(DEFAULT_THEME);
invoke<{ theme?: string; window_animation?: boolean }>("get_config")
  .then((cfg) => {
    if (typeof cfg?.window_animation === "boolean") setMotionPreference(cfg.window_animation);
    if (cfg?.theme) applyTheme(normalizeTheme(cfg.theme));
  })
  .catch(() => {
    /* 读取失败时保持默认主题 */
  });

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <MotionPolicy>
      <ErrorBoundary
        componentName="远程协助浮层"
        fallback={<WindowCrashFallback label="远程协助浮层" />}
      >
        <RcAskPop />
      </ErrorBoundary>
    </MotionPolicy>
  </React.StrictMode>,
);
