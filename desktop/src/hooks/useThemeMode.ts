import { useThemeKey, type ThemeKey } from "../lib/theme";

/**
 * 订阅全局主题（返回 ThemeKey，6 套配色之一）。
 *
 * `lib/theme.ts` 的 `applyTheme()` 切换时会派发 `themechange` 事件，`useThemeKey`
 * 接住即可，无需各组件再维护一份状态。需要「只管亮暗」的调用方用
 * `isDarkTheme(useThemeMode())` 判定。
 */
export function useThemeMode(): ThemeKey {
  return useThemeKey();
}
