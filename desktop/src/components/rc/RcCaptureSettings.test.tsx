/**
 * 「画面」分区（RcCaptureSettings）的行为契约（2026-11-20 补设置项覆盖）。
 *
 * 这个分区为什么值得单测：它是 DeskPanda 拆分迁移后**补可达性**的产物——
 * 画质 / 画面范围两项的 Rust 命令、API、store 一直在，UI 却只存在于未迁的
 * 死代码 `RcCapGroup` 里，等于「后端能改、界面改不了」。这里钉住的正是
 * 「界面真的能改」这一层：
 * ① 两行都在，当前值取 `rc.status`（不是 config 快照）；
 * ② 点选真的发出 `rc.setQuality` / `rc.setCaptureScope`（含逐屏档）；
 * ③ 能力读取失败出错误条 + 重试，**不许落成「本机没这个能力」**（U3.5）；
 * ④ 禁用三档（加载中 / 已关闭 / 在飞）透到两个下拉，且禁用是真的点不开。
 *
 * 禁用判据本身在 `lib/rcRowGate` 单测；跨端接线（命令名 / 消费方 / 挂载）
 * 由 `__tests__/RcSettingsWiring.test.ts` 钉。能力 hook 在此 mock
 * （照 RcWorkbench.test.tsx 的 hooks mock 范式），`scopeOptions` 走真的——
 * 逐屏档的 key 就是那里算的，mock 掉它这条断言就空了。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { RcMonitorInfo, RcStatus } from "@/lib/api/rc";
import type { UseRc } from "@/hooks/useRc";
import { RcCaptureSettings } from "./RcCaptureSettings";

/** 两块屏：0 主屏 + 1 副屏。逐屏档的 label/key 由真 `scopeOptions` 算。 */
const MONITORS: RcMonitorInfo[] = [
  { index: 0, x: 0, y: 0, w: 1920, h: 1080, primary: true },
  { index: 1, x: 1920, y: 0, w: 2560, h: 1440, primary: false },
];

const h = vi.hoisted(() => ({
  /** 档位表是 hook 的输入数据（不是被测逻辑），显式给小集合。 */
  qualities: [
    { key: "auto", label: "自动", tip: "自动换档", meta: "自动换档", solo: true },
    { key: "balanced", label: "均衡", tip: "约 10fps · 宽 1280", meta: "10fps · 1280" },
    { key: "sharp", label: "清晰", tip: "约 15fps · 宽 1920", meta: "15fps · 1920" },
  ],
  monitors: null as RcMonitorInfo[] | null,
  monitorsUnknown: false,
  capsUnknown: false,
  loadError: null as string | null,
  reload: vi.fn(),
}));

vi.mock("@/hooks/useRcLocalAbility", () => ({
  useRcLocalAbility: () => ({
    monitors: h.monitors,
    caps: null,
    loadError: h.loadError,
    monitorsUnknown: h.monitorsUnknown,
    capsUnknown: h.capsUnknown,
    qualities: h.qualities,
    reload: h.reload,
  }),
}));

const status = (over: Partial<RcStatus> = {}) =>
  ({
    enabled: true,
    running: true,
    capability: "control",
    quality: "auto",
    capture_scope: "virtual",
    device_deny: {},
    joins: [],
    uno: [],
    ...over,
  }) as unknown as RcStatus;

function renderCapture(over: Partial<RcStatus> = {}, rcOver: Record<string, unknown> = {}) {
  const setQuality = vi.fn().mockResolvedValue(undefined);
  const setCaptureScope = vi.fn().mockResolvedValue(undefined);
  const rc = {
    busy: false,
    status: status(over),
    setQuality,
    setCaptureScope,
    ...rcOver,
  } as unknown as UseRc;
  render(<RcCaptureSettings rc={rc} />);
  return { setQuality, setCaptureScope };
}

/** 两个下拉的触发器（RcDropdown 的按钮：label + 当前值）。 */
const qualityBtn = () => screen.getByRole("button", { name: /^画质/ }) as HTMLButtonElement;
const scopeBtn = () => screen.getByRole("button", { name: /^画面/ }) as HTMLButtonElement;
/** 菜单是 portal（挂 body），option 直接全屏查。 */
const opt = (name: RegExp | string) => screen.getByRole("option", { name });

beforeEach(() => {
  h.monitors = MONITORS;
  h.monitorsUnknown = false;
  h.capsUnknown = false;
  h.loadError = null;
  h.reload.mockClear();
});

afterEach(() => {
  cleanup();
});

describe("画面分区（RcCaptureSettings）", () => {
  it("① 两行都在，当前值取 status（画质 sharp / 画面 primary）", () => {
    renderCapture({ quality: "sharp", capture_scope: "primary" });
    expect(screen.getByText("画质档")).toBeTruthy();
    expect(screen.getByText("画面范围")).toBeTruthy();
    expect(qualityBtn().textContent).toContain("清晰");
    expect(scopeBtn().textContent).toContain("仅主屏");
  });

  it("② status 缺质量/范围字段时不显示空值（回落默认档 auto / virtual）", () => {
    // 后端老版本或异常投影可能缺这两个字段；按钮显示空 = 用户不知道该配的是什么
    renderCapture({ quality: undefined, capture_scope: undefined });
    expect(qualityBtn().textContent).toContain("自动");
    expect(scopeBtn().textContent).toContain("整屏");
  });

  it("③ 点画质档 ⇒ 发 rc.setQuality(选中键)，且只发一次", () => {
    const { setQuality } = renderCapture({ quality: "auto" });
    fireEvent.click(qualityBtn());
    fireEvent.click(opt(/均衡/));
    expect(setQuality).toHaveBeenCalledTimes(1);
    expect(setQuality).toHaveBeenCalledWith("balanced");
  });

  it("④ 点画面范围 ⇒ 发 rc.setCaptureScope(选中键)；逐屏档 key 带显示器 index", () => {
    const { setCaptureScope } = renderCapture({ capture_scope: "virtual" });
    fireEvent.click(scopeBtn());
    fireEvent.click(opt(/仅主屏/));
    expect(setCaptureScope).toHaveBeenCalledTimes(1);
    expect(setCaptureScope).toHaveBeenCalledWith("primary");

    fireEvent.click(scopeBtn());
    fireEvent.click(opt("屏2"));
    expect(setCaptureScope).toHaveBeenCalledTimes(2);
    expect(setCaptureScope).toHaveBeenLastCalledWith("monitor:1");
  });

  it("⑤ 能力读取失败 ⇒ 错误条 + 「重试」调 reload（U3.5：失败 ≠ 不存在）", () => {
    h.loadError = "未能读取本机能力";
    renderCapture();
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("未能读取本机能力");
    fireEvent.click(screen.getByRole("button", { name: "重试" }));
    expect(h.reload).toHaveBeenCalledTimes(1);
  });

  it("⑥ 没失败时不摆错误条与重试钮（防常驻）", () => {
    renderCapture();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.queryByRole("button", { name: "重试" })).toBeNull();
  });

  it("⑦ 显示器列表读不到 ⇒ hint 明说「已隐藏逐屏选项」，且逐屏档真的不在菜单里", () => {
    h.monitors = null;
    h.monitorsUnknown = true;
    renderCapture();
    expect(screen.getByText("未能读取本机显示器列表，已隐藏逐屏选项")).toBeTruthy();
    fireEvent.click(scopeBtn());
    // hint 不许是假话：说了隐藏，菜单里就不能还有逐屏档
    expect(screen.queryByRole("option", { name: /^屏/ })).toBeNull();
    // 固定两档仍在——失败只藏逐屏，不把整个下拉清空
    expect(opt(/整屏/)).toBeTruthy();
    expect(opt(/仅主屏/)).toBeTruthy();
  });

  it("⑧ 正常时 hint 报「本机 N 块屏」，逐屏档按 index 列（屏1·主 / 屏2）", () => {
    renderCapture();
    expect(screen.getByText(/本机 2 块屏/)).toBeTruthy();
    fireEvent.click(scopeBtn());
    expect(opt("屏1·主")).toBeTruthy();
    expect(opt("屏2")).toBeTruthy();
  });

  it("⑨ 通道没开（后端真值 enabled=false）⇒ 两个下拉禁用 + 理由「已关闭」，且点不开菜单", () => {
    renderCapture({ enabled: false });
    for (const btn of [qualityBtn(), scopeBtn()]) {
      expect(btn.disabled).toBe(true);
      expect(btn.title).toBe("远程协助已关闭");
    }
    fireEvent.click(qualityBtn());
    expect(screen.queryByRole("listbox")).toBeNull();
  });

  it("⑩ 切换在飞（rc.busy）⇒ 禁用，但不拿「通道已关闭」当理由糊弄", () => {
    renderCapture({}, { busy: true });
    for (const btn of [qualityBtn(), scopeBtn()]) {
      expect(btn.disabled).toBe(true);
      expect(btn.title).not.toBe("远程协助已关闭");
    }
  });

  it("⑪ status 还没读到 ⇒ 禁用，理由说「正在读取」而不是「已关闭」", () => {
    const rc = {
      busy: false,
      status: null,
      setQuality: vi.fn(),
      setCaptureScope: vi.fn(),
    } as unknown as UseRc;
    render(<RcCaptureSettings rc={rc} />);
    for (const btn of [qualityBtn(), scopeBtn()]) {
      expect(btn.disabled).toBe(true);
      expect(btn.title).toBe("正在读取远程状态…");
    }
  });
});
