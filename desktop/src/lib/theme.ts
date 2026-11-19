/**
 * 主题定义 — 6 套配色方案（远程工作台沿用 pastePanda 的视觉体系）。
 *
 * 两套「主题」概念在本仓并存，别混：
 * - **本模块的 ThemeKey**（`ocean`/`midnight`/…）：真源是 `styles/theme.css` 的
 *   `[data-theme="…"]` 变量块，rc 的所有 CSS Module 都靠这些 `--x` 变量上色。
 * - **壳子遗留的 `.dark` class**（Tailwind dark: 前缀用）：只剩壳子原生的几个
 *   ui 组件在用。`applyTheme` 会同时维护两者，所以两边都不会掉色——但**新增样式
 *   请只用 `var(--*)`**，不要再往 `.dark` 上挂东西，那是待拆除的兼容层。
 */
import { useEffect, useState } from "react";

export type ThemeKey = "ocean" | "ocean-dark" | "midnight" | "forest" | "blossom" | "dawn";

export interface Theme {
  key: ThemeKey;
  displayName: string;
  /** 是否为暗色主题 */
  dark: boolean;
}

export const THEMES: Theme[] = [
  // key 是后端存储值 + theme.css 的 [data-theme] 锚点，改名只改 displayName。
  { key: "ocean", displayName: "月白", dark: false },
  { key: "ocean-dark", displayName: "深海", dark: true },
  { key: "midnight", displayName: "午夜", dark: true },
  { key: "forest", displayName: "森林", dark: false },
  { key: "blossom", displayName: "美乐蒂", dark: false },
  { key: "dawn", displayName: "晨曦", dark: false },
];

export const DEFAULT_THEME: ThemeKey = "ocean";

/** 壳子（Deskbase）遗留的两档值，读到时就地折算，不让它们落到兜底档。 */
const LEGACY_THEME_ALIAS: Record<string, ThemeKey> = {
  dark: "ocean-dark",
  light: "ocean",
};

/**
 * 把任意配置值归一成合法 ThemeKey。
 *
 * 历史遗留：DEFAULT_CONFIG 里曾经写着 `theme: "light"`，这个值匹配不到任何
 * `[data-theme]` 块，只能落到 `:root` 兜底才碰巧渲染成月白，设置页色板也因此
 * 一个都不高亮。改掉 DEFAULT_CONFIG 只救得了新装用户 —— 老用户后端存的仍是
 * "light"，非空所以照样一路透传下去。所以配置进 store 时先在这里过一道。
 */
export function normalizeTheme(value: unknown): ThemeKey {
  if (typeof value === "string" && LEGACY_THEME_ALIAS[value]) return LEGACY_THEME_ALIAS[value];
  return THEMES.some((t) => t.key === value) ? (value as ThemeKey) : DEFAULT_THEME;
}

/**
 * 主题是否为暗色（内部先归一，未知值落 DEFAULT_THEME 的档）。
 *
 * 服务于「不跟 `[data-theme]` 全套变量、只要亮暗二档」的宿主。
 */
export function isDarkTheme(value: unknown): boolean {
  return THEMES.find((t) => t.key === normalizeTheme(value))?.dark ?? false;
}

/** 配色提交窗口；快速连切与跨窗口回声共用一个生命周期。 */
let transitionTimer: ReturnType<typeof setTimeout> | null = null;

/**
 * 安全读 localStorage：DOM Storage 可能被策略禁用（企业策略 / 隐私模式 /
 * 第三方上下文），此时**访问 `localStorage` 属性本身就抛 SecurityError**。
 * 本模块在 `main.tsx` 的模块初始化阶段就被调用，一次未捕获异常 = 整个应用白屏。
 * 主题偏好属于纯装饰，读不到就用默认值，绝不能让应用挂掉。
 */
function safeGetItem(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function safeSetItem(key: string, value: string): boolean {
  try {
    localStorage.setItem(key, value);
    return true;
  } catch {
    // 配额满 / 被禁用：主题仍在本次会话内生效（属性已切），只是不记忆。
    return false;
  }
}

/** 读已保存的主题（壳子存过 `dark`/`light` 的老值也会被归一）。 */
export function getStoredTheme(): ThemeKey {
  return normalizeTheme(safeGetItem("theme"));
}

/** 将主题应用到 document.documentElement。 */
export function applyTheme(themeKey: ThemeKey) {
  const root = document.documentElement;
  // `.dark` 是壳子遗留的 Tailwind 开关，与 data-theme 一起维护（见文件头注释）。
  root.classList.toggle("dark", isDarkTheme(themeKey));
  if (root.getAttribute("data-theme") === themeKey) return;
  // A palette change touches the whole tree. Do not start one color/background
  // transition per descendant; the native trace showed repeated style work here.
  root.dataset.themeChanging = "true";
  root.setAttribute("data-theme", themeKey);
  if (transitionTimer !== null) clearTimeout(transitionTimer);
  transitionTimer = setTimeout(() => {
    delete root.dataset.themeChanging;
    transitionTimer = null;
  }, 350);
}

/** 切主题并持久化；返回实际生效的主题。 */
export function setTheme(themeKey: ThemeKey): ThemeKey {
  applyTheme(themeKey);
  safeSetItem("theme", themeKey);
  window.dispatchEvent(new CustomEvent<ThemeKey>("themechange", { detail: themeKey }));
  return themeKey;
}

/**
 * 亮/暗两档切换（壳子遗留入口，标题栏那颗月亮/太阳按钮用它）。
 *
 * 只认「暗 → 亮」「亮 → 暗」两档，不做主题环：6 套配色的选择属于设置页，
 * 标题栏那颗按钮的语义只有「现在太亮了/太暗了」。
 */
export function toggleTheme(): ThemeKey {
  return setTheme(isDarkTheme(getCurrentTheme()) ? "ocean" : "ocean-dark");
}

/** 获取当前主题 */
export function getCurrentTheme(): ThemeKey {
  return normalizeTheme(document.documentElement.getAttribute("data-theme") ?? DEFAULT_THEME);
}

/** 当前主题（跟随 `themechange` 事件，供设置页色板与图标同步）。 */
export function useThemeKey(): ThemeKey {
  const [key, setKey] = useState<ThemeKey>(() => getCurrentTheme());
  useEffect(() => {
    const onChange = () => setKey(getCurrentTheme());
    window.addEventListener("themechange", onChange);
    return () => window.removeEventListener("themechange", onChange);
  }, []);
  return key;
}
