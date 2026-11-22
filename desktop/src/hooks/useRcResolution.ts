/**
 * useRcResolution — 发起端「分辨率」下拉的三段式状态(照 useRcCodecChoice 模式)。
 *
 * (命名避开已有的 useRcDisplayMode——那是本机全屏/缩放档,与对端物理分辨率无关。)
 *
 * 为什么走 rcApplySetting 而不是 rcSendInput:旧被控端**静默丢弃**不认识的
 * InputEvent kind——点了没反应是最难排的形态;回执(8s)能把「未确认」显形。
 * 值编码 `m{n}/WxH@hz` 见 `lib/rcDisplayMode.ts`(与 Rust 同源)。
 */
import { useEffect, useRef, useState } from "react";
import { rcApplySetting } from "@/lib/api/rc";
import {
  displayModeOptions,
  displayModeValue,
  type PeerMonitorModes,
} from "@/lib/rcDisplayMode";

export function useRcResolution({
  sessionId,
  peerModes,
  connected,
  onStatus,
}: {
  sessionId: string;
  peerModes: PeerMonitorModes[] | null | undefined;
  connected: boolean;
  onStatus: (message: string, kind: "success" | "error" | "info") => void;
}) {
  const [pending, setPending] = useState(false);
  const scope = useRef({ sessionId, alive: true, token: 0, busy: false });
  scope.current.sessionId = sessionId;
  useEffect(() => {
    const owner = scope.current;
    owner.alive = true;
    owner.busy = false;
    setPending(false);
    return () => {
      owner.alive = false;
      owner.token++;
      owner.busy = false;
    };
  }, [sessionId]);

  const options = displayModeOptions(peerModes);
  const value = displayModeValue(peerModes);

  const pick = async (key: string) => {
    if (!connected || scope.current.busy || !options.some((o) => o.key === key)) return;
    const token = ++scope.current.token;
    const current = () =>
      scope.current.alive &&
      scope.current.sessionId === sessionId &&
      scope.current.token === token;
    scope.current.busy = true;
    setPending(true);
    const restore = key.endsWith("/0x0@0");
    const label = options.find((o) => o.key === key)?.label ?? key;
    onStatus(restore ? "正在请求还原分辨率…" : `正在请求切换到 ${label}…`, "info");
    try {
      const result = await rcApplySetting(sessionId, "display_mode", key);
      if (!current()) return;
      onStatus(
        result.status === "accepted"
          ? restore
            ? "对方分辨率已还原"
            : `对方已接受 ${label}；画面切换约需 1–2 秒`
          : "请求未确认（对方版本可能较旧或暂不支持）；请查看实际画面或重试",
        result.status === "accepted" ? "success" : "info",
      );
    } catch (error) {
      if (current()) onStatus(`分辨率切换失败：${error}`, "error");
    } finally {
      if (current()) {
        scope.current.busy = false;
        setPending(false);
      }
    }
  };

  return {
    options,
    value,
    pick,
    pending,
    /** 无选项 = 对端不支持/旧版本:整块 UI 不渲染(调用方判)。 */
    supported: options.length > 0,
    disabled: !connected || pending,
  };
}

export type RcResolutionChoice = ReturnType<typeof useRcResolution>;
