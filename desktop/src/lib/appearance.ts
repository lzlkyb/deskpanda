import { APP_SLUG } from "./about";

/** 界面材质独立于明暗与终端配色；只切根节点属性，保留页面和会话节点。 */
type Appearance = "classic" | "glass";

const KEY = `${APP_SLUG}.ui-style`;
function getStoredAppearance(): Appearance {
  try {
    return localStorage.getItem(KEY) === "classic" ? "classic" : "glass";
  } catch {
    return "glass";
  }
}

/** 在 React 挂载前恢复，避免先绘制经典界面再闪成玻璃。初始化不写偏好。 */
export function initializeAppearance(): void {
  document.documentElement.dataset.uiStyle = getStoredAppearance();
}
