import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { ConfirmDialogHost } from "./components/ConfirmDialogHost";
import { InterfaceMotion } from "./components/InterfaceMotion";
import { ToastProvider } from "./components/Toast";
import { RcWorkbench } from "./components/rc/RcWorkbench";
import { useAppHiddenFlag } from "./hooks/useAppHiddenFlag";
import { DEFAULT_CONFIG, useAppStore, type AppConfig } from "./stores/appStore";
import { logger } from "./lib/logger";

/**
 * 应用根：远程工作台作为**一级页面**（不再像 pastePanda 那样是主窗口上的一层覆盖
 * `RcOverlay`）。
 *
 * 挂载顺序不是随手排的，四层各自兜一件事：
 * 1. `InterfaceMotion` —— 全局动效开关（`no-anim` 类）。放最外层，否则它自己被
 *    内层的崩溃带走，用户会看到「关掉动效后又开始动」。
 * 2. `ErrorBoundary` —— 工作台渲染崩溃时给可读的占位页，而不是白屏（独立窗口
 *    没有 DevTools 可开，崩溃信息必须画出来）。
 * 3. `ToastProvider` —— 工作台全树的 `useToast()` 来源。
 * 4. `ConfirmDialogHost` —— **必须挂**：`confirmDialog()` 只是把请求放进模块级槽位，
 *    真正画出来的是这个宿主。不挂的话 `await confirmDialog(...)` 永远不返回，
 *    调用方的流程静默停住（不报错也不继续）。
 */
export default function App() {
  // 窗口收进托盘/最小化时给 <html> 打 data-app-hidden，暂停全部 CSS 动画与轮询：
  // WebView2 不会为隐藏窗口节流动画，实测不做这事会白烧约 0.6 个 CPU 核。
  useAppHiddenFlag();
  // 配置进 store：rc 的偏好开关（浮条唤出 / 键盘模式 / 剪贴板自动同步）都从
  // `useAppStore().config` 读。读失败就用默认值，不能让「读不到配置」变成打不开。
  useEffect(() => {
    let alive = true;
    invoke<Partial<AppConfig>>("get_config")
      .then((cfg) => {
        if (alive) useAppStore.getState().setConfig({ ...DEFAULT_CONFIG, ...cfg });
      })
      .catch((e) => {
        logger.warn("读取配置失败，使用默认值", e);
        if (alive) useAppStore.getState().setConfig(DEFAULT_CONFIG);
      });
    return () => {
      alive = false;
    };
  }, []);

  return (
    <InterfaceMotion>
      <ErrorBoundary componentName="远程工作台">
        <ToastProvider>
          <RcWorkbench />
          <ConfirmDialogHost />
        </ToastProvider>
      </ErrorBoundary>
    </InterfaceMotion>
  );
}
