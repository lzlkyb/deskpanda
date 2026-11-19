import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import App from "./App";
import { UpdateProvider } from "./contexts/UpdateContext";
import { applyTheme, getStoredTheme } from "./lib/theme";
import { initializeAppearance } from "./lib/appearance";

// 样式装载顺序决定谁压谁，别随手调：
// 1. 壳子（Tailwind + reset + 壳子组件令牌）
import "./index.css";
import "./interaction-refinements.css";
import "./appearance.css";
// 2. 远程工作台的令牌层与全局样式（`var(--*)` 全套在这里定义）。
//    rc 的所有 CSS Module 都消费这些变量，少 import 一个就是整片「无样式」——
//    而且构建不报错（CSS Module 照样哈希出类名，只是变量全空）。
import "./styles/theme.css";
import "./styles/globals.css";
import "./styles/buttons.css";
import "./styles/dialog.css";

// 首帧前先上色，避免先画一帧默认主题再闪成用户选的（FOUC）。
//
// 主题的**唯一真源是 localStorage**（与 rc 的界面偏好同性质：只影响本机观感，
// 不参与任何协议判断）。不放进 `config.json`：那是跨设备/可手改的用户设置，
// 同一件事存两份必然分叉。单窗口阶段也不需要 `theme-changed` 跨窗同步——
// 同进程内 `setTheme()` 直接生效；HUD / 会话胶囊落地后在这里补一条 listen。
applyTheme(getStoredTheme());
initializeAppearance();

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchOnWindowFocus: false,
      retry: 1,
      staleTime: 3000,
    },
  },
});

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <UpdateProvider>
        <App />
      </UpdateProvider>
    </QueryClientProvider>
  </React.StrictMode>,
);
