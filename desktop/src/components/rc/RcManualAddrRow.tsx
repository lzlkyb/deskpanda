/**
 * RcManualAddrRow — 「管理此设备」里的手动直连地址行（IP 直连入口，2026-11-22）。
 *
 * 语义（🔴 与 Rust `set_manual_addr` 同一口径）：给**已配对**设备补一条直连
 * **候选**——与 presence 自动发现、中继兜底一起交给 QUIC 竞速，不覆盖任何
 * 已有路径；填错的最坏结果是这一条不通。不是「连陌生设备」的口子（身份由
 * QUIC 握手校验，UI 入口也只对 rc 配对设备出现）。
 *
 * 三态：未设置（hint + 设置）/ 编辑中（input + 保存/取消）/ 已设置（值 + 修改/清除）。
 * 裸 IP 合法：后端存「端口 0」，拨号时取对端最近公告的端口（端点端口随对端
 * 重启会变，存死只会过期）。
 *
 * 样式全复用 orgEditor 家族（与标签·备注编辑器同款虚线卡片，不另起一套）。
 * UI 稿：design/manual-addr-row-2026-11-22.html（不入库）。
 */
import { useState } from "react";
import { ArrowLeftRight } from "lucide-react";
import type { RcTargetDevice } from "@/lib/api/rc";
import styles from "./RemoteComputerA2.module.css";

export function RcManualAddrRow({
  target,
  busy,
  onSet,
}: {
  target: RcTargetDevice;
  busy: boolean;
  /** 返回是否成功（失败由调用方 toast——与 saveDeviceField 同一口径）。 */
  onSet: (nodeId: string, addr: string | null) => Promise<boolean>;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const current = target.manual_addr ?? "";

  const save = async () => {
    const ok = await onSet(target.node_id, draft.trim() || null);
    if (ok) setEditing(false);
  };

  return (
    <div className={styles.orgEditor}>
      <div className={styles.orgLabel}>
        <ArrowLeftRight size={13} aria-hidden="true" />
        直连地址（可选）
      </div>
      {editing ? (
        <div className={styles.manualAddrRow}>
          <input
            className={styles.orgInput}
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder="192.168.1.5:4919 / [fd00::9]:4919 / 裸 IP 自动补端口"
            disabled={busy}
            aria-label="直连地址"
            onKeyDown={(e) => {
              if (e.key === "Enter") void save();
              if (e.key === "Escape") setEditing(false);
            }}
          />
          <button
            type="button"
            className={styles.orgSave}
            disabled={busy || !draft.trim()}
            onClick={() => void save()}
          >
            保存
          </button>
          <button
            type="button"
            className={styles.orgSave}
            disabled={busy}
            onClick={() => setEditing(false)}
          >
            取消
          </button>
        </div>
      ) : (
        <div className={styles.manualAddrRow}>
          {current ? (
            <>
              <span className={styles.manualAddrValue}>{current}</span>
              <button
                type="button"
                className={styles.orgSave}
                disabled={busy}
                onClick={() => {
                  setDraft(current);
                  setEditing(true);
                }}
              >
                修改
              </button>
              <button
                type="button"
                className={styles.orgSave}
                disabled={busy}
                onClick={() => void onSet(target.node_id, null)}
              >
                清除
              </button>
            </>
          ) : (
            <>
              <span className={styles.manualAddrHint}>
                未设置。当局域网发现和公共中继都够不着这台设备时（隔离网段、直连网线），可手填它的 IP。
              </span>
              <button
                type="button"
                className={styles.orgSave}
                disabled={busy}
                onClick={() => {
                  setDraft("");
                  setEditing(true);
                }}
              >
                设置
              </button>
            </>
          )}
        </div>
      )}
    </div>
  );
}
