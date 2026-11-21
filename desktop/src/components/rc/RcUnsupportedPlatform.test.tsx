/**
 * RcUnsupportedPlatform 守卫单测（S6 降级通知卡）。
 *
 * 三条断言各管一件事：标题说清「不支持」、正文给出「去哪用」、附注交代「本机还
 * 能用什么」并回显平台值（用户反馈问题时截图能直接看到平台）。
 */
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { RcUnsupportedPlatform } from "./RcUnsupportedPlatform";

describe("RcUnsupportedPlatform（降级通知；S6 后受众只剩 linux 等平台）", () => {
  it("标题走通用措辞，正文指路桌面双平台", () => {
    render(<RcUnsupportedPlatform platform="linux" />);

    expect(screen.getByText("远程电脑暂不支持当前系统")).toBeTruthy();
    expect(screen.getByText(/请在 Windows 或 macOS 电脑上安装 DeskPanda/)).toBeTruthy();
  });

  it("附注交代本机仍可用的部分，并回显平台原值", () => {
    render(<RcUnsupportedPlatform platform="linux" />);

    expect(screen.getByText(/托盘常驻、检查更新与退出/)).toBeTruthy();
    expect(screen.getByText("linux")).toBeTruthy();
  });

  it("🔴 标题不再按平台分支：mac 已交付（S6），不存在「暂不支持 macOS」的文案", () => {
    render(<RcUnsupportedPlatform platform="linux" />);

    expect(screen.queryByText(/暂不支持 macOS/)).toBeNull();
    expect(screen.queryByText(/暂不支持 Linux/)).toBeNull();
  });

  it("平台值缺失（理论上前提是 degraded 必有值）时回显「未知」而不是空白", () => {
    render(<RcUnsupportedPlatform />);

    expect(screen.getByText("远程电脑暂不支持当前系统")).toBeTruthy();
    expect(screen.getByText("未知")).toBeTruthy();
  });
});
