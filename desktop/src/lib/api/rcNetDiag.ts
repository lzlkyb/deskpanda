/**
 * 本机网络诊断 IPC（R0，对应 `src-tauri/src/commands/net_diag.rs`）。
 *
 * 独立文件的原因：`rcTypes.ts` 已 395 行贴着 `.ts ≤ 400` 红线，而诊断类型
 * 是独立小域（不进设备列表/会话流），不挤它。字段名与 Rust 一致（snake_case）。
 * 失败由调用方 toast，这里不吞。
 */
import { invoke } from "@tauri-apps/api/core";


/** 打洞可行性判定（与 Rust `PunchExpectation` 对齐，serde snake_case）。 */
export type PunchExpectation =
  | "direct_likely"
  | "direct_possible"
  | "relay_only"
  | "udp_blocked"
  | "captive"
  | "unknown";

export interface FamilyMapping {
  udp: boolean;
  mapping_varies: boolean | null;
  observed: string | null;
}

interface RelayLatencyEntry {
  probe: string;
  relay: string;
  ms: number;
}

export interface NetDiagSnapshot {
  udp_v4: boolean;
  udp_v6: boolean;
  mapping_varies_by_dest_ipv4: boolean | null;
  mapping_varies_by_dest_ipv6: boolean | null;
  global_v4: string | null;
  global_v6: string | null;
  captive_portal: boolean | null;
  preferred_relay: string | null;
  relay_latencies_ms: RelayLatencyEntry[];
  verdict: PunchExpectation;
  family_v4: FamilyMapping;
  family_v6: FamilyMapping;
  /** 当前 home 中继 URL（P0 C1）。null = 未选中/未启用中继。 */
  home_relay: string | null;
  /** home 中继是否已连接（iroh 1.1 只有已连/未连两态）。 */
  home_relay_connected: boolean;
}

export interface NetDiag {
  snapshot: NetDiagSnapshot | null;
  unavailable_reason: string;
}

/** 取本机网络画像 + 打洞可行性判定（本地显示用，不上传）。 */
export function rcNetDiag(): Promise<NetDiag> {
  return invoke<NetDiag>("rc_net_diag");
}

/** 判定 → 展示文案。口径与 Rust `PunchExpectation::zh_summary` 一致，别在这里再编。 */
export function punchSummary(v: PunchExpectation): string {
  switch (v) {
    case "direct_likely":
      return "映射稳定，可尝试直连";
    case "direct_possible":
      return "UDP 可用，对称性待更多探测样本";
    case "relay_only":
      return "对称 NAT：iroh 会放弃直连、恒走中继";
    case "udp_blocked":
      return "UDP 被阻断，只能走中继回落";
    case "captive":
      return "网络需网页认证（captive portal）";
    case "unknown":
      return "网络探测尚未完成";
  }
}

/** 中继 URL → host；解析失败原样返回（URL 来自自家后端，但显示层不该炸）。 */
function relayHostOf(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

/** 当前中继一行的展示值（「URL host · 已连接/未连接」）。null = 无中继态整行不出。 */
export function homeRelayLabel(s: NetDiagSnapshot): string | null {
  if (!s.home_relay) return null;
  return `${relayHostOf(s.home_relay)} · ${s.home_relay_connected ? "已连接" : "未连接"}`;
}

/** 会话 HUD「路径」行的展示值：中继路径且知道 home 中继时附上域名
 * （P0 C1，验收 §6.5「会话走自建」的判据格）；其余路径原样。 */
export function pathRowValue(pathKind: string, path: string, homeRelay?: string | null): string {
  return pathKind === "relay" && homeRelay ? `${path}（${relayHostOf(homeRelay)}）` : path;
}

/** 映射观测的单族描述（对称性三态 + 无 UDP 的兜底）。 */
export function mappingLabel(f: FamilyMapping): string {
  if (!f.udp) return "UDP 未通";
  if (f.mapping_varies === true) return "对称（随目的变）";
  if (f.mapping_varies === false) return "稳定";
  return "样本不足";
}
