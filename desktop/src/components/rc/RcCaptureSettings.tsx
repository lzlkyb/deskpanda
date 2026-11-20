/**
 * RcCaptureSettings — 工作台设置页「画面」分区（2026-11-20 补设置项覆盖）。
 *
 * 两行：画质档（`rc_set_quality`）/ 画面范围（`rc_set_capture_scope`）。
 * 迁自 pastePanda 的 `RcCapGroup` 同款两行——那个组件在本仓是**死代码**
 * （`RcSection` 未迁，无人挂载），导致这两项设置在 DeskPanda 里完全不可达：
 * Rust 命令、API、store 全齐，唯独没有界面。原页脚「更多远程设置在主窗口」
 * 的跳转链接本就指向这两项，而本仓主窗口即工作台、壳子设置页没有远程分区
 * ⇒ 链接 emit 出去无人监听、点击静默无响应（已随本次一并删除）。
 *
 * 本机能力（显示器 / 编码）由 `useRcLocalAbility` 供数：档位表随能力过滤
 * （高帧率 / 4K60 只在本机达标时出现），读失败出错误条 + 重试——
 * **禁止落成「本机没这个能力」**（U3.5：失败 ≠ 不存在）。
 *
 * 控件用会话浮条同款的 `RcDropdown`（portal 到 body，设置页里不会被
 * `.setCard` 的圆角/层叠上下文截到）。禁用判据收口在 `lib/rcRowGate`。
 */
import { useRcLocalAbility } from "@/hooks/useRcLocalAbility";
import type { UseRc } from "@/hooks/useRc";
import { rcRowGate } from "@/lib/rcRowGate";
import type { RcCaptureScope, RcQuality } from "@/lib/api/rc";
import { scopeOptions } from "@/lib/rcScope";
import { RcDropdown } from "./RcDropdown";
import styles from "./RemoteComputer.module.css";

export function RcCaptureSettings({ rc }: { rc: UseRc }) {
  const ability = useRcLocalAbility();
  const status = rc.status;
  const gate = rcRowGate(status, rc.busy);
  // 后端默认档（`lib/rcQuality` 的 DEFAULT_QUALITY 同值）；status 还没读到时
  // 按钮不显示空值，读到后由 RcDropdown 自己回正确值。
  const quality = (status?.quality ?? "auto") as RcQuality;
  // 未知范围按最宽的「整屏」，不猜成更窄的（同 `lib/rcScope` 的口径）。
  const scope = (status?.capture_scope ?? "virtual") as RcCaptureScope;

  return (
    <section className={styles.setCard}>
      <h3 className={styles.setSecTitle} id="rc-set-capture">
        画面
      </h3>
      {ability.loadError && (
        <div className={styles.setLoadError} role="alert">
          <span>{ability.loadError}</span>
          <button
            type="button"
            className={styles.setRetryBtn}
            onClick={() => void ability.reload()}
          >
            重试
          </button>
        </div>
      )}
      <div className={styles.setRow}>
        <div className={styles.setRowInfo}>
          <div className={styles.setRowTitle}>画质档</div>
          <div className={styles.setRowHint}>被控端编码档位，默认按链路自动换挡</div>
        </div>
        <RcDropdown
          label="画质"
          value={quality}
          options={ability.qualities}
          columns={2}
          disabled={gate.disabled}
          disabledTitle={gate.disabledTitle}
          onPick={(k) => void rc.setQuality(k)}
        />
      </div>
      <div className={styles.setRow}>
        <div className={styles.setRowInfo}>
          <div className={styles.setRowTitle}>画面范围</div>
          <div className={styles.setRowHint}>
            {ability.monitorsUnknown
              ? "未能读取本机显示器列表，已隐藏逐屏选项"
              : `本机 ${ability.monitors?.length ?? 0} 块屏 · 这里配的是本机作为被控端时对方能看到哪一块屏`}
          </div>
        </div>
        <RcDropdown
          label="画面"
          value={scope}
          options={scopeOptions(ability.monitors ?? [])}
          columns={1}
          disabled={gate.disabled}
          disabledTitle={gate.disabledTitle}
          onPick={(k) => void rc.setCaptureScope(k)}
        />
      </div>
    </section>
  );
}
