/**
 * RcKeepAwakeRow — 工作台设置页「通用」分区的会话防休眠行（2026-11-20 补设置项覆盖）。
 *
 * 迁自 pastePanda `RcCapGroup` 的同行（该组件在本仓是死代码，见 RcCaptureSettings
 * 文件头）。行为契约由 `rcKeepAwakeRow.test.tsx` 钉住五条，全部是静默失败的形态：
 * ① 开关态取后端真值 `status.keep_awake`，**不读 config**——config 是上次运行的
 *    快照，真值在 SQLite；
 * ② 后端没这个字段 = 关，不许把「未知」渲染成开；
 * ③ 点一下发反值，且只发一次；
 * ④ 通道没开（`status.enabled` 假）⇒ 禁用并说明原因；
 * ⑤ 切换在飞（`rc.busy`）⇒ 禁用，但不拿「通道已关闭」当理由糊弄。
 * ④⑤ 与「加载中」三档的完整判据收口在 `lib/rcRowGate`（本行与画面分区共用）。
 */
import type { UseRc } from "@/hooks/useRc";
import { rcRowGate } from "@/lib/rcRowGate";
import settings from "@/components/Settings.module.css";
import styles from "./RemoteComputer.module.css";

export function RcKeepAwakeRow({ rc }: { rc: UseRc }) {
  // ① 后端真值；② 缺省关（`!!` 把 undefined 压成 false，不渲染成「开」）。
  const keepAwake = !!rc.status?.keep_awake;
  const gate = rcRowGate(rc.status, rc.busy);

  return (
    <div className={styles.setRow}>
      <div className={styles.setRowInfo}>
        <div className={styles.setRowTitle}>会话防休眠</div>
        <div className={styles.setRowHint}>有人连着时本机不睡、屏幕不熄；下次连接起生效</div>
      </div>
      <span className={keepAwake ? styles.selfChipOn : styles.selfChipOff}>
        {keepAwake ? "已开启" : "未开启"}
      </span>
      <button
        type="button"
        role="switch"
        aria-checked={keepAwake}
        aria-label="会话防休眠"
        className={`${settings.sToggle} ${keepAwake ? settings.on : settings.off}`}
        disabled={gate.disabled}
        title={gate.disabledTitle}
        onClick={() => void rc.setKeepAwake(!keepAwake)}
      >
        <span className={settings.sToggleThumb} />
        <span className={settings.sToggleLabel}>{keepAwake ? "开" : "关"}</span>
      </button>
    </div>
  );
}
