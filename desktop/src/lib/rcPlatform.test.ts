import { describe, expect, it } from "vitest";
import { rcPlatformDegraded, rcPlatformSupported } from "./rcPlatform";

describe("rcPlatform 判据（S6 后：远程功能交付 Windows 与 macOS）", () => {
  it("windows / macos 都是受支持平台（S6 起 mac 桥接已迁入）", () => {
    expect(rcPlatformSupported("windows")).toBe(true);
    expect(rcPlatformSupported("macos")).toBe(true);
  });

  it("🔴 undefined（首帧 / 读取失败）不降级——Windows 首帧不许闪降级提示", () => {
    expect(rcPlatformDegraded(undefined)).toBe(false);
    // 与 supported 解耦：undefined 既不是「支持」也不是「不支持」，是「还不知道」
    expect(rcPlatformSupported(undefined)).toBe(false);
  });

  it("linux 等其余平台走降级态；🔴 macOS 不再降级（S6 交付）", () => {
    expect(rcPlatformDegraded("macos")).toBe(false);
    expect(rcPlatformDegraded("linux")).toBe(true);
    expect(rcPlatformDegraded("android")).toBe(true);
  });

  it("大小写敏感：Rust 恒给小写，大写不当成受支持平台（保守降级而不是假装支持）", () => {
    expect(rcPlatformSupported("Windows")).toBe(false);
    expect(rcPlatformSupported("MacOS")).toBe(false);
    expect(rcPlatformDegraded("Windows")).toBe(true);
  });

  it("空串不是合法平台值（get_app_info 恒回非空）：按降级处理", () => {
    expect(rcPlatformDegraded("")).toBe(true);
  });
});
