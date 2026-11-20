/**
 * rcRowGate 的三档真话（2026-11-20）。
 *
 * 每条都钉「禁用 + 理由」这一对：只有禁用没有理由 = 哑巴控件；理由说错
 * （把加载中说成已关、把 busy 说成已关）= 界面撒谎。期望值逐字写死，
 * 不遍历被测常量自证。
 */
import { describe, expect, it } from "vitest";
import { rcRowGate } from "./rcRowGate";
import type { RcStatus } from "./api/rc";

const status = (over: Partial<RcStatus> = {}) =>
  ({ enabled: true, ...over }) as unknown as RcStatus;

describe("rcRowGate", () => {
  it("通道开着且不在飞 ⇒ 不禁用", () => {
    const g = rcRowGate(status(), false);
    expect(g.disabled).toBe(false);
    expect(g.disabledTitle).toBeUndefined();
  });

  it("后端真值 enabled=false ⇒ 禁用 + 理由「远程协助已关闭」", () => {
    const g = rcRowGate(status({ enabled: false }), false);
    expect(g.disabled).toBe(true);
    expect(g.disabledTitle).toBe("远程协助已关闭");
  });

  it("切换在飞（busy）⇒ 禁用，且不拿「远程协助已关闭」当理由糊弄", () => {
    const g = rcRowGate(status(), true);
    expect(g.disabled).toBe(true);
    expect(g.disabledTitle).not.toBe("远程协助已关闭");
  });

  it("status 还没读到 ⇒ 禁用 + 理由说「正在读取」，不说「已关闭」", () => {
    const g = rcRowGate(null, false);
    expect(g.disabled).toBe(true);
    expect(g.disabledTitle).toBe("正在读取远程状态…");
    expect(g.disabledTitle).not.toContain("关闭");
  });

  it("busy 与 off 同时命中 ⇒ 理由取「已关闭」（更具体的那个）", () => {
    const g = rcRowGate(status({ enabled: false }), true);
    expect(g.disabled).toBe(true);
    expect(g.disabledTitle).toBe("远程协助已关闭");
  });
});
