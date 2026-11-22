// @vitest-environment jsdom
/**
 * useRcResolution 三段式(照 useRcCodecChoice.test 范式):
 * 接受/未确认/失败三条反馈路径 + 还原哨兵的文案分叉。
 */
import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const applySetting = vi.fn();
vi.mock("@/lib/api/rc", () => ({ rcApplySetting: (...a: unknown[]) => applySetting(...a) }));

import { useRcResolution } from "./useRcResolution";

const peerModes = [
  { index: 0, current: { w: 3840, h: 2160, hz: 160 }, modes: [{ w: 1920, h: 1080, hz: 60 }] },
];

function setup(over: Partial<Parameters<typeof useRcResolution>[0]> = {}) {
  const onStatus = vi.fn();
  const hook = renderHook(() =>
    useRcResolution({ sessionId: "s1", peerModes, connected: true, onStatus, ...over }),
  );
  return { hook, onStatus };
}

afterEach(() => vi.clearAllMocks());

describe("useRcResolution", () => {
  it("accepted:先「正在请求」后成功;busy 期间禁点", async () => {
    applySetting.mockResolvedValue({ status: "accepted", value: "m0/1920x1080@60" });
    const { hook, onStatus } = setup();
    expect(hook.result.current.supported).toBe(true);
    await act(async () => {
      await hook.result.current.pick("m0/1920x1080@60");
    });
    expect(applySetting).toHaveBeenCalledWith("s1", "display_mode", "m0/1920x1080@60");
    const kinds = onStatus.mock.calls.map((c) => c[1]);
    expect(kinds[0]).toBe("info");
    expect(kinds.at(-1)).toBe("success");
    expect(onStatus.mock.calls.at(-1)?.[0]).toContain("1920×1080");
    expect(hook.result.current.pending).toBe(false);
  });

  it("unconfirmed:回执超时不伪装成功", async () => {
    applySetting.mockResolvedValue({ status: "unconfirmed", value: "x" });
    const { hook, onStatus } = setup();
    await act(async () => {
      await hook.result.current.pick("m0/1920x1080@60");
    });
    expect(onStatus.mock.calls.at(-1)?.[1]).toBe("info");
    expect(onStatus.mock.calls.at(-1)?.[0]).toContain("未确认");
  });

  it("还原哨兵走独立文案", async () => {
    applySetting.mockResolvedValue({ status: "accepted", value: "m0/0x0@0" });
    const { hook, onStatus } = setup();
    await act(async () => {
      await hook.result.current.pick("m0/0x0@0");
    });
    expect(onStatus.mock.calls[0][0]).toContain("还原");
    expect(onStatus.mock.calls.at(-1)?.[0]).toContain("已还原");
  });

  it("对端无 caps:supported=false 且不渲染不发送", async () => {
    const { hook, onStatus } = setup({ peerModes: null });
    expect(hook.result.current.supported).toBe(false);
    await act(async () => {
      await hook.result.current.pick("m0/1920x1080@60"); // options 为空,直接挡
    });
    expect(applySetting).not.toHaveBeenCalled();
    expect(onStatus).not.toHaveBeenCalled();
  });

  it("断链/失败:报 error,busy 解除", async () => {
    applySetting.mockRejectedValue(new Error("写流超时"));
    const { hook, onStatus } = setup();
    await act(async () => {
      await hook.result.current.pick("m0/1920x1080@60");
    });
    expect(onStatus.mock.calls.at(-1)?.[1]).toBe("error");
    expect(hook.result.current.pending).toBe(false);
  });
});
