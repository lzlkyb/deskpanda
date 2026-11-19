/**
 * appStore —— 全局配置（跨重启记忆的设置）在前端的唯一镜像。
 *
 * 从 pastePanda 的 1421 行 appStore 精简而来：那个 store 还管剪贴板历史、栈、
 * 笔记、标签等本产品不存在的东西，整搬会把一整套无关依赖（semantic / tableSplit /
 * todo 锚点）拖进来。这里只留**远程会话真正读得到的字段**，其余按需再加。
 *
 * 真值的分工（别搞混，搞混了就是「界面开着、后端拒绝」）：
 * - `rc_clip_auto` / `rc_hover_reveal` / `rc_key_mode`：纯界面偏好，后端从不读，
 *   载体就是这份 config（Rust `config.rs` 里同名同义字段，serde 持久化到
 *   `config.json`）。⚠️ 若 Rust 结构体不登记这些键，`save_config` 会静默丢弃它们
 *   —— 现象是「改完重启又变回来」。
 * - `rc_enabled` / `rc_keep_awake`：**不是**这里说了算，真值在 SQLite `config` 表，
 *   由 `rc_set_enabled` / `rc_set_keep_awake` 命令写；界面判断一律用
 *   `rc_get_status` 返回的 `status.enabled`（见 `settings/rcGroups/RcCapGroup.tsx`
 *   的注释）。放在这里只为类型完整，不承担持久化职责。
 *
 * 为什么 `updateConfig` 是同步 void：调用点（`useRcHoverReveal` / `useRcKeyMode` /
 * `useRcClipAutoPref`）都是「先本地生效→再串行落盘→失败回滚」的模式，落盘由它们
 * 自己 `invoke("save_config")` 并负责回滚。store 只做内存态，不吞掉失败。
 */
import { create } from "zustand";
import { setMotionPreference } from "@/lib/motionPreference";

export interface AppConfig {
  /** 关窗时释放界面内存（壳子字段，后端用）。 */
  release_webview_on_close: boolean;
  /** 开机自启镜像值（壳子字段，后端用）。 */
  autostart_enabled: boolean;
  /** 远程会话：剪贴板自动同步。默认开。读取口径 `lib/rcClipAuto.ts`。 */
  rc_clip_auto: boolean;
  /** 远程会话：浮条顶缘 hover 唤出。默认开。读取口径 `lib/rcHoverReveal.ts`。 */
  rc_hover_reveal: boolean;
  /** 远程会话：键盘模式 `type` / `direct`。读取口径 `lib/rcKeyMode.ts`。 */
  rc_key_mode: string;
  /** 窗口动画开关（`setMotionPreference` 的持久化镜像）。 */
  window_animation?: boolean;
  /** 主题。壳子以 `localStorage` 为主，这里只是配置副本。 */
  theme?: string;
  /** 前端侧扩展键：未知键原样透传，避免每加一个偏好就改两次结构体。 */
  [key: string]: unknown;
}

export const DEFAULT_CONFIG: AppConfig = {
  release_webview_on_close: true,
  autostart_enabled: false,
  rc_clip_auto: true,
  rc_hover_reveal: true,
  rc_key_mode: "type",
  window_animation: true,
};

interface AppState {
  config: AppConfig;
  /** 是否已经从后端读过一次。未读过时不要用 config 的值做「用户选择」判断。 */
  configLoaded: boolean;
  /**
   * 来源应用图标的提取结果缓存（`null` = 提取过但不出来，别再试）。
   * 消费者是 `lib/source-mappings.ts`，它用 `getState()` 读写以避免循环引用。
   */
  realIconCache: Record<string, string | null>;
  setConfig: (config: AppConfig) => void;
  updateConfig: (partial: Partial<AppConfig>) => void;
  setRealIconUrl: (key: string, url: string | null) => void;
}

export const useAppStore = create<AppState>((set) => ({
  config: DEFAULT_CONFIG,
  configLoaded: false,
  realIconCache: {},
  setRealIconUrl: (key, url) => set((s) => ({ realIconCache: { ...s.realIconCache, [key]: url } })),
  setConfig: (config) => {
    if (typeof config.window_animation === "boolean") setMotionPreference(config.window_animation);
    set({ config, configLoaded: true });
  },
  updateConfig: (partial) => {
    set((s) => ({ config: { ...s.config, ...partial } }));
    if (typeof partial.window_animation === "boolean") setMotionPreference(partial.window_animation);
  },
}));
