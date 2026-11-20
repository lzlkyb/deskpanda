/**
 * rcConnectError 的判据测试：两种已知握手失败形态必须翻成人话，
 * 其余错误必须原样透传（不许把未知错误也吞成人话——那是新的谎言）。
 * 判据串来自 net/endpoint.rs:5-15 的公网联调实录。
 */
import { describe, expect, it } from "vitest";

import { mapConnectError, rcConnectErrorText } from "./rcConnectError";

const PEER_ABORTED = "aborted by peer: peer closed connection while handshake was in progress";
const ERROR_120 = "error 120: peer doesn't support any known protocol";

describe("rcConnectErrorText", () => {
  it("对端握手期拒绝 → 指向「对方版本过旧」并可行动", () => {
    const text = rcConnectErrorText(PEER_ABORTED);
    expect(text).toContain("对方");
    expect(text).toContain("更新");
    // 原始串必须保留在后半段，用户能复述给维护者
    expect(text).toContain("aborted by peer");
  });

  it("error 120 → 指向「本机版本过旧」", () => {
    const text = rcConnectErrorText(new Error(ERROR_120));
    expect(text).toContain("本机");
    expect(text).toContain("更新");
    expect(text).toContain("error 120");
  });

  it("🔴 未知错误必须原样透传（null），不许编造人话", () => {
    expect(rcConnectErrorText("连接超时")).toBeNull();
    expect(rcConnectErrorText(new Error("dial timeout"))).toBeNull();
    expect(rcConnectErrorText("")).toBeNull();
    expect(rcConnectErrorText(null)).toBeNull();
  });
});

describe("mapConnectError", () => {
  it("命中形态 → 抛出人话 Error", () => {
    expect(() => mapConnectError(PEER_ABORTED)).toThrow(/对方/);
  });

  it("未命中 → 原样抛出（字符串错误保持字符串语义可被 errText 提取）", () => {
    const raw = "dial timeout";
    try {
      mapConnectError(raw);
      expect.unreachable("必须抛出");
    } catch (e) {
      expect(e instanceof Error && e.message === raw).toBe(true);
    }
  });
});
