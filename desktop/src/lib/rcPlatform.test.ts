import { describe, expect, it } from "vitest";
import { rcPlatformDegraded, rcPlatformSupported } from "./rcPlatform";

describe("rcPlatform 判据（S6：远程功能只交付 Windows）", () => {
  it("windows 是唯一受支持平台", () => {
    expect(rcPlatformSupported("windows")).toBe(true);
  });

  it("🔴 undefined（首帧 / 读取失败）不降级——Windows 首帧不许闪降级提示", () => {
    expect(rcPlatformDegraded(undefined)).toBe(false);
    // 与 supported 解耦：undefined 既不是「支持」也不是「不支持」，是「还不知道」
    expect(rcPlatformSupported(undefined)).toBe(false);
  });

  it("macos / linux 走降级态", () => {
    expect(rcPlatformDegraded("macos")).toBe(true);
    expect(rcPlatformDegraded("linux")).toBe(true);
  });

  it("大小写敏感：Rust 恒给小写，大写不当成 Windows（保守降级而不是假装支持）", () => {
    expect(rcPlatformSupported("Windows")).toBe(false);
    expect(rcPlatformDegraded("Windows")).toBe(true);
  });

  it("空串不是合法平台值（get_app_info 恒回非空）：按降级处理", () => {
    expect(rcPlatformDegraded("")).toBe(true);
  });
});
