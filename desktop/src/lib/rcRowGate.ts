/**
 * rcRowGate — 设置页「被控侧」各行的启用判据（画质 / 画面范围 / 会话防休眠共用）。
 *
 * 🔴 为什么收口成一个函数：这三行的禁用理由必须**说真话**，而真话有三档，
 *    抄第二遍就会漂移（2026-11-20 迁 RcCapGroup 三行时定）：
 *    ① `status === null`（还没读到 / 后端不可达）⇒ 禁用，理由「正在读取远程状态…」。
 *       **不许说「远程协助已关闭」**——加载中不是关，说错了用户以为开关坏了；
 *    ② `status.enabled === false`（后端真值，不是 `config.rc_enabled`）⇒ 禁用，
 *       理由「远程协助已关闭」。真值不会让界面显示成「能改但其实改了不生效」；
 *    ③ `rc.busy`（切换在飞）⇒ 禁用，**不给理由**——不拿「已关闭」糊弄（通道明明开着）。
 *
 * 判据用 `status.enabled` 而不是 `config.rc_enabled`：这三行配的就是「通道开着时
 * 能被怎么控」，与原 `RcCapGroup` / `RcA2SelfCard` 主开关同源。
 */
import type { RcStatus } from "./api/rc";

export interface RcRowGate {
  /** 控件是否禁用（三档任一命中即禁用）。 */
  disabled: boolean;
  /** 禁用时的悬停说明；不禁用时为 `undefined`（禁用不许是哑巴）。 */
  disabledTitle: string | undefined;
}

export function rcRowGate(status: RcStatus | null, busy: boolean): RcRowGate {
  if (status === null) {
    return { disabled: true, disabledTitle: "正在读取远程状态…" };
  }
  const off = !status.enabled;
  return {
    disabled: off || busy,
    disabledTitle: off ? "远程协助已关闭" : undefined,
  };
}
