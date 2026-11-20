/**
 * 入站申请入口收口守卫（2026-10-01 方案甲；2026-11-20 体检清仓后修订）。
 *
 * 背景：入站远程申请曾经有**三个** UI 入口——置顶浮层 `RcAskPop`（唯一决策现场）、
 * 主窗 `RcOverlay` 里的确认卡、远程电脑页 `RcWorkbenchOverlays` 里的同一张卡。
 * 弹框在场时三处同亮一个敲门 = 同一决策多处可点（AGENTS.md §15 反面教材），
 * 两张卡连同 pending 的 toast / 拉窗 effect 一并删除。
 *
 * 2026-11-20 体检（docs/项目体检报告-2026-11-20.md §2）：`RcOverlay.tsx` 整个
 * 组件确认零引用（壳子布局 `components/layout/`、`components/tabs/` 同批死亡），
 * 文件本身删除——断言 ① 由「源码里不出现申请卡调用」升级为「文件保持不存在」。
 *
 * 不变量被真正推翻时（例如重新设计出别的兜底），请改这里的断言并写明理由，
 * 而不是把断言删掉——「同一敲门只剩一处可点」就是这条守卫存在的意义。
 */
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const root = process.cwd();
const workbench = readFileSync(resolve(root, "src/components/rc/RcWorkbench.tsx"), "utf8");

describe("入站申请入口收口守卫（2026-10-01 方案甲）", () => {
  it("① 远程电脑页不再挂申请卡片（配对层 RcPairLayer 是另一回事，保留）", () => {
    expect(workbench).not.toContain("RcJoinRequests");
    expect(workbench).not.toContain("RcWorkbenchOverlays");
    expect(workbench).toContain("RcPairLayer");
  });

  it("② 已删组件文件保持不存在（防 git 回滚式复活）", () => {
    expect(existsSync(resolve(root, "src/components/rc/RcJoinRequests.tsx"))).toBe(false);
    expect(existsSync(resolve(root, "src/components/rc/RcWorkbenchOverlays.tsx"))).toBe(false);
    expect(existsSync(resolve(root, "src/components/rc/RcOverlay.tsx"))).toBe(false);
  });

  it("③ 壳子死代码目录不复辟（layout/tabs 于 2026-11-20 体检确认零引用后删除）", () => {
    expect(existsSync(resolve(root, "src/components/layout"))).toBe(false);
    expect(existsSync(resolve(root, "src/components/tabs"))).toBe(false);
    expect(existsSync(resolve(root, "src/lib/headerSecurityBadges.ts"))).toBe(false);
  });
});
