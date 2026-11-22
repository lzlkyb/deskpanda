/**
 * 远端分辨率调节(②,2026-11-22)的档位表与文案唯一真源。
 *
 * 类型镜像 Rust `rc/display_mode.rs` 的 serde 形状(caps 帧 `display_modes` 字段);
 * 编码串 `"m{monitor}/{w}x{h}@{hz}"` 与 Rust `encode_setting` 同源——回执 value
 * 逐字匹配靠它,别在别处手拼。
 */
import type { RcDropdownOption } from "@/components/rc/RcDropdown";

/** 一块显示器的一个显示模式(hz=0 = 平台未报刷新率)。 */
interface PeerDisplayMode {
  w: number;
  h: number;
  hz: number;
}

/** 一块显示器的模式集合(caps 过线)。 */
export interface PeerMonitorModes {
  index: number;
  current: PeerDisplayMode;
  modes: PeerDisplayMode[];
}

/** settings 回执通道的 value 编码(Rust `display_mode::encode_setting` 同款)。 */
export function encodeDisplayMode(monitor: number, w: number, h: number, hz: number): string {
  return `m${monitor}/${w}x${h}@${hz}`;
}

/** 模式 → 按钮展示值(`3840×2160@160`;hz=0 省略)。 */
function displayModeLabel(m: PeerDisplayMode): string {
  return `${m.w}×${m.h}${m.hz > 0 ? `@${m.hz}` : ""}`;
}

/**
 * 下拉选项:每屏「原始(还原)」置顶 + 各档(显示器号进 meta)。
 * 当前模式不在档位表里时补一项(显示「当前」标记)——按钮上的值永远有出处。
 */
export function displayModeOptions(
  list: PeerMonitorModes[] | null | undefined,
): RcDropdownOption<string>[] {
  if (!list?.length) return [];
  const out: RcDropdownOption<string>[] = [];
  for (const mon of list) {
    out.push({
      key: encodeDisplayMode(mon.index, 0, 0, 0),
      label: "原始(还原)",
      tip: `还原显示器${mon.index + 1}到会话前分辨率;会话结束也会自动还原`,
      meta: `显示器${mon.index + 1}`,
    });
    const keys = new Set(mon.modes.map(encode3));
    for (const m of mon.modes) {
      out.push({
        key: encodeDisplayMode(mon.index, m.w, m.h, m.hz),
        label: displayModeLabel(m),
        tip: `把对方的显示器${mon.index + 1}切成 ${displayModeLabel(m)}`,
        meta: `显示器${mon.index + 1}`,
      });
    }
    if (!keys.has(encode3(mon.current))) {
      out.push({
        key: encodeDisplayMode(mon.index, mon.current.w, mon.current.h, mon.current.hz),
        label: `${displayModeLabel(mon.current)}(当前)`,
        tip: "对方当前正在使用的分辨率",
        meta: `显示器${mon.index + 1}`,
      });
    }
  }
  return out;
}

function encode3(m: PeerDisplayMode): string {
  return encodeDisplayMode(-1, m.w, m.h, m.hz);
}

/** 按钮当前值:第一块屏的当前档(下拉 value;多屏时只作按钮显示,不影响选项)。 */
export function displayModeValue(
  list: PeerMonitorModes[] | null | undefined,
): string {
  const first = list?.[0];
  if (!first) return "";
  return encodeDisplayMode(first.index, first.current.w, first.current.h, first.current.hz);
}
