/**
 * IPC 镜像契约守卫：前端 `RcTargetDevice` 接口 ⊆ Rust 投影结构体。
 *
 * 背景（2026-11-20 体检实证）：`rc_targets` 的 Rust 投影结构漏了
 * `auto_accept` 字段——真值一直在 store、setter 命令也一直在写，但列表
 * 命令没把它带出来。前端 `rcTypes.ts` 早就声明了 `auto_accept?: boolean`，
 * 四处消费点（RcDeviceManageActions / RcPageFiles / RcA2DeviceDetail…）
 * 读到恒 `undefined`：开关永远显示「未开启」，且 `!undefined` 恒 true
 * 导致**只能开、关不掉**。TS 编译器对「后端少字段」零感知——这类漂移
 * 只能用显式契约钉住。
 *
 * 钉的方向是 **TS ⊆ Rust**（前端声明的每个字段，后端都必须真的下发）。
 * 反方向（Rust 多发字段）无害，不拦——serde 序列化多键前端忽略即可。
 *
 * 不变量被真正推翻时（例如有意让某字段只在新版下发），请改这里的
 * 豁免并写明理由，而不是删断言。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const root = process.cwd();
const rust = readFileSync(
  resolve(root, "src-tauri/src/commands/rc.rs"),
  "utf8",
);
const ts = readFileSync(resolve(root, "src/lib/api/rcTypes.ts"), "utf8");

/** 从 Rust `pub struct RcTargetDevice { … }` 块里抽字段名（`pub name: Type,`）。 */
function rustStructFields(src: string, name: string): string[] {
  const m = src.match(new RegExp(`pub struct ${name} \\{([\\s\\S]*?)\\n\\}`));
  if (!m) throw new Error(`Rust 源里找不到 pub struct ${name}`);
  return [...m[1].matchAll(/^\s*pub (\w+):/gm)].map((x) => x[1]!);
}

/** 从 TS `export interface RcTargetDevice { … }` 块里抽字段名（`name?:`）。 */
function tsInterfaceFields(src: string, name: string): string[] {
  const m = src.match(
    new RegExp(`export interface ${name} \\{([\\s\\S]*?)\\n\\}`),
  );
  if (!m) throw new Error(`TS 源里找不到 export interface ${name}`);
  return [...m[1].matchAll(/^\s*(\w+)\??:/gm)].map((x) => x[1]!);
}

describe("IPC 镜像契约：RcTargetDevice", () => {
  const rustFields = rustStructFields(rust, "RcTargetDevice");
  const tsFields = tsInterfaceFields(ts, "RcTargetDevice");

  it("前端声明的每个字段，Rust 投影都必须真的下发", () => {
    const missing = tsFields.filter((f) => !rustFields.includes(f));
    expect(
      missing,
      `前端 rcTypes.ts 声明了、但 Rust commands/rc.rs 的 RcTargetDevice 没下发的字段：\
${missing.join(", ")}——这就是「前端读恒 undefined」型 bug 的温床 \
（2026-11-20 的 auto_accept 正是这样漏的）`,
    ).toEqual([]);
  });

  it("🔴 auto_accept 必须两侧都在（2026-11-20 漏投影事故的钉子）", () => {
    expect(rustFields).toContain("auto_accept");
    expect(tsFields).toContain("auto_accept");
  });

  it("守卫自身没失明：两侧解析出的字段数都非零", () => {
    // 防守卫悄悄失效（比如哪天结构体改名/搬文件，上面的正则失配应当直接抛错，
    // 这里再钉一层「解析结果非空」）。
    expect(rustFields.length).toBeGreaterThan(5);
    expect(tsFields.length).toBeGreaterThan(5);
  });
});
