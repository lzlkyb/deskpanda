// @vitest-environment jsdom
import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("@/lib/tauri", () => ({ invoke: invokeMock }));

/**
 * 每例取一份全新的 usePlatform 模块实例——单飞缓存是**模块级**的，
 * 复用同一实例会让第二条用例直接吃到上一条已 resolve 的 promise（invoke 计数为 0）。
 */
async function freshUsePlatform() {
  vi.resetModules();
  return (await import("./usePlatform")).usePlatform;
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe("usePlatform（get_app_info 单飞）", () => {
  it("读到 platform 字段", async () => {
    invokeMock.mockResolvedValue({ version: "1.0.0", platform: "macos" });
    const usePlatform = await freshUsePlatform();

    const h = renderHook(() => usePlatform());

    await waitFor(() => expect(h.result.current).toBe("macos"));
    expect(invokeMock).toHaveBeenCalledWith("get_app_info");
  });

  it("🔴 多处同时挂载只发一次 invoke（模块级单飞，平台是编译期常量）", async () => {
    invokeMock.mockResolvedValue({ platform: "windows" });
    const usePlatform = await freshUsePlatform();

    const a = renderHook(() => usePlatform());
    const b = renderHook(() => usePlatform());
    await waitFor(() => {
      expect(a.result.current).toBe("windows");
      expect(b.result.current).toBe("windows");
    });

    expect(invokeMock).toHaveBeenCalledTimes(1);
  });

  it("invoke 失败回落 undefined（调用方按非降级处理，且不重试）", async () => {
    invokeMock.mockRejectedValue(new Error("command not found"));
    const usePlatform = await freshUsePlatform();

    const h = renderHook(() => usePlatform());
    await waitFor(() => expect(h.result.current).toBeUndefined());

    // 失败也走同一条已缓存的 promise：再挂一个组件不会再发一次
    const b = renderHook(() => usePlatform());
    await waitFor(() => expect(b.result.current).toBeUndefined());
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });
});
