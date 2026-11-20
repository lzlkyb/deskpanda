/**
 * 设置页「被控侧三项」的接线守卫（2026-11-20 补设置项覆盖）。
 *
 * 背景：拆分迁移后，画质 / 画面范围 / 会话防休眠这三项出现过一个**全链路只有
 * 前端断一环**的形态——Rust 命令（`rc_set_quality` / `rc_set_capture_scope` /
 * `rc_set_keep_awake`）、API 封装、store action、`useRc` 投影全齐，UI 却只
 * 存在于未迁的死代码 `RcCapGroup` 里：后端能改、界面改不了。同时页脚
 * 「更多远程设置在主窗口」的跳转（`pp:open-settings-rc`）在本仓无人监听
 * （主窗口即工作台、壳子设置页没有远程分区），点击静默无响应。
 *
 * 本文件把「这三项从命令到界面」的每一环钉住，任一环再断就红：
 *   ① 命令名在 API 封装里（rcCommands.ts 的 invoke 名逐字同后端）；
 *   ② store action 真的调这三个封装（不是只 import）；
 *   ③ useRc 把三个 action 投影出来（组件拿到的是 `rc.setXxx`）；
 *   ④ 组件真的消费（RcCaptureSettings 两行 / RcKeepAwakeRow 一行）；
 *   ⑤ 设置页真的挂载这两个组件；
 *   ⑥ 死链接不复辟：src 里不再出现该事件名的**字符串字面量**，
 *      两个组件也不再挂 `onOpenSettings` prop、页脚文案不回来。
 *
 * ⑥ 的扫描**排除测试文件**（同 rcCredNaming 的口径）：注释里提旧事件名是
 * 历史叙述（RcPageSettings 文件头就有），允许；守卫要拦的是代码里的字面量。
 * 不变量被真正推翻时（例如重新设计出别的跳转），请改这里并写明理由，
 * 而不是删断言——「这三项在 DeskPanda 里必须界面可达」就是本守卫的意义。
 */
import { readFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const root = process.cwd();
const read = (rel: string) => readFileSync(resolve(root, rel), "utf8");

const rcCommands = read("src/lib/api/rcCommands.ts");
const rcStore = read("src/stores/rcStore.ts");
const useRc = read("src/hooks/useRc.ts");
const capture = read("src/components/rc/RcCaptureSettings.tsx");
const keepAwake = read("src/components/rc/RcKeepAwakeRow.tsx");
const pageSettings = read("src/components/rc/RcPageSettings.tsx");
const workbench = read("src/components/rc/RcWorkbench.tsx");

/** 递归列出 src 下的 ts/tsx **非测试**文件（目录不存在返回空，不炸收集期）。 */
function sourceFiles(dir: string): string[] {
  let entries;
  try {
    entries = readdirSync(resolve(root, dir), { withFileTypes: true });
  } catch {
    return [];
  }
  const out: string[] = [];
  for (const e of entries) {
    const rel = `${dir}/${e.name}`;
    if (e.isDirectory()) out.push(...sourceFiles(rel));
    else if (/\.tsx?$/.test(e.name) && !e.name.includes(".test.")) out.push(rel);
  }
  return out;
}

const srcFiles = sourceFiles("src");

describe("设置页三项接线守卫（2026-11-20）", () => {
  it("① 三个 set 命令的 invoke 名在 API 封装里（逐字同后端）", () => {
    expect(rcCommands).toContain('invoke("rc_set_quality"');
    expect(rcCommands).toContain('invoke("rc_set_capture_scope"');
    expect(rcCommands).toContain('invoke("rc_set_keep_awake"');
  });

  it("② store action 真的调这三个封装（只 import 不调 = 没接上）", () => {
    // 同一行内 `setXxx: … rcSetXxx(`——action 定义与它调的封装贴在一起
    expect(rcStore).toMatch(/setQuality:.*rcSetQuality/);
    expect(rcStore).toMatch(/setCaptureScope:.*rcSetCaptureScope/);
    expect(rcStore).toMatch(/setKeepAwake:.*rcSetKeepAwake/);
  });

  it("③ useRc 把三个 action 投影给组件（rc.setQuality 等不是凭空来的）", () => {
    expect(useRc).toContain("setQuality: a.setQuality");
    expect(useRc).toContain("setCaptureScope: a.setCaptureScope");
    expect(useRc).toContain("setKeepAwake: a.setKeepAwake");
  });

  it("④ 「画面」分区两行真的发命令（onPick 断掉就红）", () => {
    expect(capture).toContain("rc.setQuality(");
    expect(capture).toContain("rc.setCaptureScope(");
  });

  it("④ 「会话防休眠」行真的发命令", () => {
    expect(keepAwake).toContain("rc.setKeepAwake(");
  });

  it("⑤ 设置页挂载这两个组件（挂载漏了 = 界面不可达，正是本次修的形态）", () => {
    expect(pageSettings).toContain("<RcCaptureSettings");
    expect(pageSettings).toContain("<RcKeepAwakeRow");
  });

  it("⑥ 死链接不复辟：src 代码里不出现该事件名的字符串字面量", () => {
    // 字面量判据（带引号）而不是裸词：注释/历史叙述用反引号提它，允许；
    // 真写进代码的 emit("pp:open-settings-rc") / listen("pp:open-settings-rc")
    // 一定带引号。RcPageSettings 文件头那句历史注释不会被误伤。
    const hits = srcFiles
      .map((f) => [f, read(f)] as const)
      .filter(([, src]) => src.includes('"pp:open-settings-rc"'))
      .map(([f]) => f);
    expect(hits, `这些文件又写上了死事件名：${hits.join(", ")}`).toEqual([]);
  });

  it("⑥ 两处组件不再接收 onOpenSettings prop，页脚文案不回来", () => {
    expect(workbench).not.toContain("onOpenSettings");
    expect(pageSettings).not.toContain("onOpenSettings");
    // 页脚文案同 ⑥ 上一条的口径：文件头注释用「」引述它是历史叙述，允许；
    // 要拦的是**运行时形态**——JSX 文本节点或字符串字面量。
    expect(pageSettings).not.toContain(">更多远程设置在主窗口<");
    expect(pageSettings).not.toContain('"更多远程设置在主窗口"');
  });

  it("扫描范围没写错（防路径 typo 让上面几条空转）", () => {
    expect(srcFiles).toContain("src/components/rc/RcCaptureSettings.tsx");
    expect(srcFiles).toContain("src/components/rc/RcKeepAwakeRow.tsx");
    expect(srcFiles).toContain("src/components/rc/RcPageSettings.tsx");
    expect(srcFiles.length).toBeGreaterThan(100);
  });
});
