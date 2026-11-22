/**
 * rcDisplayMode — 档位表构建 + 全链路接线守卫(照 RcTargetDeviceMirror 家族)。
 * 接线守卫钉的是 2026-09-17 老教训的变体:「后端能做、界面没接」——
 * caps 有 display_modes 而会话 UI 不渲染,功能等于不存在。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  displayModeOptions,
  displayModeValue,
  encodeDisplayMode,
  type PeerMonitorModes,
} from "./rcDisplayMode";

const modes = (over: Partial<PeerMonitorModes> = {}): PeerMonitorModes => ({
  index: 0,
  current: { w: 3840, h: 2160, hz: 160 },
  modes: [
    { w: 3840, h: 2160, hz: 160 },
    { w: 1920, h: 1080, hz: 60 },
  ],
  ...over,
});

describe("rcDisplayMode 档位表", () => {
  it("编码与 Rust 同源格式;还原哨兵以 /0x0@0 结尾", () => {
    expect(encodeDisplayMode(0, 1920, 1080, 60)).toBe("m0/1920x1080@60");
    expect(encodeDisplayMode(1, 0, 0, 0)).toBe("m1/0x0@0");
  });

  it("每屏「原始(还原)」置顶 + 各档带显示器号", () => {
    const out = displayModeOptions([modes()]);
    expect(out[0]?.label).toBe("原始(还原)");
    expect(out[0]?.key).toBe("m0/0x0@0");
    expect(out).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ label: "3840×2160@160", meta: "显示器1" }),
        expect.objectContaining({ label: "1920×1080@60" }),
      ]),
    );
  });

  it("当前档不在列表时补「(当前)」项——按钮值永远有出处", () => {
    const out = displayModeOptions([
      modes({ current: { w: 1600, h: 900, hz: 60 }, modes: [{ w: 1920, h: 1080, hz: 60 }] }),
    ]);
    expect(out).toEqual(
      expect.arrayContaining([expect.objectContaining({ label: "1600×900@60(当前)" })]),
    );
  });

  it("多屏各自成组;空/null 不出选项", () => {
    const out = displayModeOptions([modes(), modes({ index: 1, current: { w: 2560, h: 1440, hz: 60 }, modes: [{ w: 2560, h: 1440, hz: 60 }] })]);
    expect(out.filter((o) => o.key.endsWith("/0x0@0"))).toHaveLength(2);
    expect(displayModeOptions(null)).toEqual([]);
    expect(displayModeOptions([])).toEqual([]);
  });

  it("按钮当前值取第一块屏的当前档", () => {
    expect(displayModeValue([modes()])).toBe("m0/3840x2160@160");
    expect(displayModeValue(null)).toBe("");
  });
});

describe("接线守卫:分辨率调节全链路", () => {
  const root = process.cwd();
  const read = (rel: string) => readFileSync(resolve(root, rel), "utf8");

  it("Rust→caps→UI 每一环都在(display_modes/渲染点)", () => {
    const caps = read("src-tauri/src/rc/inbound_tasks.rs");
    expect(caps).toContain('"display_modes"');
    const view = read("src/components/rc/RcSessionView.tsx");
    expect(view).toContain("useRcResolution(");
    expect(view).toContain("resolution={resolutionChoice}");
    const more = read("src/components/rc/RcCapsuleMore.tsx");
    expect(more).toContain('label="分辨率"');
    const drawer = read("src/components/rc/RcControlDrawer.tsx");
    expect(drawer).toContain('"display"');
  });
});
