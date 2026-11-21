import type { Transition } from "framer-motion";

/**
 * 动效令牌（两套并存，键名不重叠——刻意如此）。
 *
 * 两套的量纲不同：`MOTION` 是**毫秒**（壳子 `index.css` 的 token 对齐，给
 * usePresence / useAutoAnimateRM / useDialogDismiss 用），`WB_MOTION` 是**秒 + 贝塞尔
 * 数组**（framer-motion 用，`theme.css` 的 CSS 时长与它对齐）。别混着取。
 */

/** 壳子（Deskbase）动效：毫秒档位，与 index.css 的 token 对齐。 */
export const MOTION = {
  feedback: 100,
  standard: 160,
  enter: 180,
  exit: 140,
  ease: "cubic-bezier(0.16, 1, 0.3, 1)",
} as const;

/** 远程工作台动效：一套 cadence 管 controls / content / layers。 */
const WB_MOTION = {
  fast: 0.15,
  base: 0.2,
  panel: 0.3,
  large: 0.4,
  ease: [0.2, 0, 0, 1],
  enter: [0.05, 0.7, 0.1, 1],
} as const;

export const CONTROL_TRANSITION: Transition = { type: "tween", duration: WB_MOTION.fast, ease: WB_MOTION.ease };
export const CONTENT_TRANSITION: Transition = { type: "tween", duration: WB_MOTION.base, ease: WB_MOTION.ease };
export const PANEL_TRANSITION: Transition = { type: "tween", duration: WB_MOTION.panel, ease: WB_MOTION.enter };
