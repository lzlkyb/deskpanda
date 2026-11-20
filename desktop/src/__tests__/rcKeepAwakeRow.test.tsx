/**
 * 「会话防休眠」那一行的行为契约（2026-10-04 立；2026-11-20 随设置项补覆盖
 * 从死代码 `RcCapGroup` 迁到活组件 `RcKeepAwakeRow`——契约一条没改，
 * 只换渲染目标：迁过去之前这一行在 DeskPanda 里根本没有界面可达）。
 *
 * 为什么单独一个文件：这一行的失败方式全是静默的——默认值被人改成 true 就等于
 * 替用户按住了屏幕与电源计划；通道关了开关还能按 = 界面说「已开」而锁根本不会拿；
 * 判据错吃到 `config.rc_enabled` 则在切换在飞的那段窗口里显示成「能改但改了
 * 不生效」。三条都要红得下来。
 *
 * 跨端接线（命令名 / 投影 / 取锁点）由 Rust 侧 `守卫_会话防休眠六处接线成对` 钉，
 * 这里只管渲染出来的这一层；「三个 set 命令有 UI 消费方」由
 * `__tests__/RcSettingsWiring.test.ts` 钉。禁用三档（加载中 / 已关闭 / 在飞）
 * 的判据本身在 `lib/rcRowGate` 的单测里。
 */
import { describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { RcKeepAwakeRow } from "@/components/rc/RcKeepAwakeRow";
import type { RcStatus } from "@/lib/api/rc";
import type { UseRc } from "@/hooks/useRc";

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

function renderRow(over: Partial<RcStatus> = {}, rcOver: Record<string, unknown> = {}) {
  const setKeepAwake = vi.fn().mockResolvedValue(true);
  const rc = {
    busy: false,
    status: status(over),
    setKeepAwake,
    ...rcOver,
  } as unknown as UseRc;
  render(<RcKeepAwakeRow rc={rc} />);
  return { setKeepAwake };
}

/** 本仓没装 jest-dom，开关态按既有写法直接读属性。 */
const sw = () => screen.getByRole("switch") as HTMLButtonElement;

describe("会话防休眠行", () => {
  it("开关态取后端真值 status.keep_awake，不读 config", () => {
    renderRow({ keep_awake: true });
    expect(sw().getAttribute("aria-checked")).toBe("true");
    expect(screen.getByText("开")).toBeTruthy();
  });

  it("默认（后端没这个字段）= 关，不许把「未知」渲染成开", () => {
    renderRow({});
    expect(sw().getAttribute("aria-checked")).toBe("false");
  });

  it("点一下发的是取反值，且只发一次", () => {
    const { setKeepAwake } = renderRow({ keep_awake: false });
    fireEvent.click(sw());
    expect(setKeepAwake).toHaveBeenCalledTimes(1);
    expect(setKeepAwake).toHaveBeenCalledWith(true);
  });

  it("通道没开 ⇒ 禁用并说明原因（判据 = 后端真值 status.enabled）", () => {
    renderRow({ enabled: false });
    expect(sw().disabled).toBe(true);
    expect(sw().title).toBe("远程协助已关闭");
  });

  it("切换在飞（rc.busy）⇒ 禁用，且不拿「通道已关闭」当理由糊弄", () => {
    renderRow({}, { busy: true });
    expect(sw().disabled).toBe(true);
    // 通道明明开着，禁用理由却是「远程协助已关闭」就是假话
    expect(sw().title).not.toBe("远程协助已关闭");
  });

  it("status 还没读到 ⇒ 禁用，理由说「正在读取」而不是「已关闭」", () => {
    const setKeepAwake = vi.fn().mockResolvedValue(true);
    const rc = { busy: false, status: null, setKeepAwake } as unknown as UseRc;
    render(<RcKeepAwakeRow rc={rc} />);
    expect(sw().disabled).toBe(true);
    expect(sw().title).toBe("正在读取远程状态…");
  });
});
