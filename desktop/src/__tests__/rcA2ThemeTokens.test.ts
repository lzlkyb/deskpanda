import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { DEFAULT_THEME, normalizeTheme } from "@/lib/theme";

const root = process.cwd();
const css = readFileSync(resolve(root, "src/components/rc/RemoteComputerA2.module.css"), "utf8");
// 入口文件：pastePanda 里工作台是独立窗口 `src/rc-main.tsx`，本仓它成了**主窗口**，
// 入口就是 `src/main.tsx`（守卫要钉的东西不变：主题从哪儿来、令牌层有没有装）。
const entry = readFileSync(resolve(root, "src/main.tsx"), "utf8");
const theme = readFileSync(resolve(root, "src/styles/theme.css"), "utf8");
const workbench = readFileSync(resolve(root, "src/components/rc/RcWorkbench.tsx"), "utf8");
const errorSlot = readFileSync(
  resolve(root, "src/components/rc/RcWorkbenchErrorSlot.tsx"),
  "utf8",
);

function luminance(hex: string): number {
  const full = hex.length === 4 ? hex.replace(/./g, (value, index) => (index ? value + value : value)) : hex;
  const rgb = [1, 3, 5].map((index) => Number.parseInt(full.slice(index, index + 2), 16) / 255);
  const linear = rgb.map((value) => (value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4));
  return 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2];
}

function contrast(a: string, b: string): number {
  const [bright, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (bright + 0.05) / (dark + 0.05);
}

describe("远程电脑 A2 真实主题守卫", () => {
  it("A2 只消费 PastePanda 语义变量，不写死颜色、渐变或主题分支", () => {
    expect(css).not.toMatch(/#[0-9a-f]{3,8}\b/i);
    expect(css).not.toMatch(/(?:linear|radial)-gradient\(/);
    expect(css).not.toContain("backdrop-filter");
    expect(css).not.toContain("[data-theme");
    expect(css).not.toContain("!important");

    for (const token of [
      "--app-bg",
      "--section-bg",
      "--card-bg",
      "--text-primary",
      "--text-secondary",
      "--border-color",
      "--accent",
      "--accent-light",
      "--accent-solid",
      "--text-on-accent",
      "--success",
      "--warning",
      "--danger",
    ]) {
      expect(css, `A2 应消费 ${token}`).toContain(`var(${token})`);
    }
    expect(theme.match(/--text-on-accent\s*:/g)?.length).toBe(6);
    expect(css).not.toContain("var(--toggle-on-label)");
  });

  /* 详情头横幅的底：A2 文件禁 gradient 字面量（上一条），所以渐变只能装在
     令牌里。令牌漏一套主题的后果不是报错，是该主题下这一横条没有底色 ——
     tsc / vitest 都看不出来，只有这条守卫能拦。 */
  it("详情头横幅令牌 --rc-head-bg 六套主题齐全，且表达式一致、从令牌派生", () => {
    const values = [...theme.matchAll(/^\s*--rc-head-bg:\s*(.+);/gm)].map((m) => m[1].trim());
    expect(values, "--rc-head-bg 应有 6 处（六套主题各一）").toHaveLength(6);

    for (const value of values) {
      expect(value, `不应写死颜色：${value}`).not.toMatch(/#[0-9a-f]{3,8}\b/i);
      expect(value, `应从现有令牌派生：${value}`).toMatch(/var\(--/);
    }
    // --accent / --card-bg 各自跟着主题走，表达式本身不该分叉
    expect(new Set(values).size, "--rc-head-bg 在六套主题里的表达式应完全相同").toBe(1);

    // 而消费方必须走这个令牌，不能在 A2 里自己拼一份渐变（那会被上一条拦下）
    expect(css).toContain("var(--rc-head-bg)");
  });

  /* hero 插画六件套（屏体/边框/颈/座/光晕台/标语笔触）与 --rc-head-bg 同一纪律：
     漏一套主题 = 该主题下插画缺层，tsc/vitest 全看不出来，只有这条守卫能拦。 */
  it("hero 插画令牌六套主题齐全、表达式一致且从令牌派生", () => {
    for (const token of [
      "--rc-hero-screen",
      "--rc-hero-bezel",
      "--rc-hero-neck",
      "--rc-hero-base",
      "--rc-hero-pedestal",
      "--rc-slogan-line",
    ]) {
      const values = [...theme.matchAll(new RegExp(`^\\s*${token}:\\s*(.+);`, "gm"))].map((m) => m[1].trim());
      expect(values, `${token} 应有 6 处（六套主题各一）`).toHaveLength(6);
      for (const value of values) {
        expect(value, `${token} 不应写死颜色：${value}`).not.toMatch(/#[0-9a-f]{3,8}\b/i);
        expect(value, `${token} 应从现有令牌派生：${value}`).toMatch(/var\(--/);
      }
      expect(new Set(values).size, `${token} 在六套主题里的表达式应完全相同`).toBe(1);
      expect(css, `A2 应消费 ${token}`).toContain(`var(${token})`);
    }
  });

  /* 入口这一层最容易犯的错是「主题写死」和「令牌层没装」：
     前者让用户的主题选择失效，后者让整个工作台变成无样式——而**构建两者都不报错**
     （CSS Module 照样哈希类名，只是 `var(--*)` 全空），只有这条守卫能拦。 */
  it("入口从本地偏好读主题、不写死，并装齐令牌层", () => {
    // 主题来自 `getStoredTheme()`（localStorage 单一真源），不是写死的字面量。
    // ⚠️ 与 pastePanda 的差异：那里工作台是独立窗口，还要 `invoke("get_config")`
    // 读配置 + 监听 `"theme-changed"` 跟随主窗口；本仓工作台就是主窗口、单窗口运行，
    // 同进程 `setTheme()` 直接生效，跨窗同步那条等 HUD/胶囊窗口落地再加回来。
    expect(entry).toContain("applyTheme(getStoredTheme())");
    expect(entry).not.toMatch(/applyTheme\("(?:ocean|ocean-dark|midnight|forest|blossom|dawn)"\)/);
    // 令牌层与全局样式：缺任何一个都是「整片无样式但不报错」。
    for (const sheet of ["./styles/theme.css", "./styles/globals.css", "./styles/buttons.css", "./styles/dialog.css"]) {
      expect(entry, `入口应 import ${sheet}`).toContain(sheet);
    }
    // 顺带钉住「主题归一」这条路还在：未知值（老配置里的 "light"）必须能落到合法档。
    expect(normalizeTheme("light")).toBe(DEFAULT_THEME);
    expect(normalizeTheme("midnight")).toBe("midnight");
  });

  it("六套主题的主按钮文字都达到普通文字 4.5:1 对比度", () => {
    const pairs = [...theme.matchAll(/--accent-solid:\s*(#[0-9a-f]{6});[^]*?--text-on-accent:\s*(#[0-9a-f]{3,6});/gi)];
    expect(pairs).toHaveLength(6);
    for (const [, background, text] of pairs) {
      expect(contrast(background, text), `${text} on ${background}`).toBeGreaterThanOrEqual(4.5);
    }
  });

  it("非会话页面也常驻展示并可关闭远程错误", () => {
    // 2026-09-23：错误条拆进 RcWorkbenchErrorSlot（.tsx ≤ 300 红线）。守卫跟着迁移
    // ——只钉外壳不够：外壳在、里面没挂 RcErrorPanel，同样是「错误看不见」。
    expect(workbench).toContain("rc.error &&");
    expect(workbench).toContain("<RcWorkbenchErrorSlot");
    expect(workbench).toContain("onDismiss={rc.clearError}");
    expect(errorSlot).toContain("<RcErrorPanel");
    expect(errorSlot).toContain("onDismiss={onDismiss}");
  });
});
