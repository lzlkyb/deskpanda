/**
 * IPC 镜像契约守卫：前端 `NetDiagSnapshot` 接口 ⊆ Rust 投影结构体。
 *
 * 与 `RcTargetDeviceMirror.test.ts` 同一套思路（2026-11-20 auto_accept 漏投影
 * 事故的推广项，P0 方案稿 §4.3）：TS 编译器对「后端少字段」零感知——前端声明了
 * `home_relay` 而后端没下发的话，读到的恒 `null`，诊断卡整行静默消失。
 *
 * 钉的方向是 **TS ⊆ Rust**（前端声明的每个字段，Rust 都必须真的序列化下发）；
 * 反方向（Rust 多发）无害——serde 多键前端忽略即可。
 *
 * 不变量被有意推翻时改豁免并写理由，不要删断言。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const root = process.cwd();
const rust = readFileSync(
  resolve(root, "src-tauri/src/net/diagnostics.rs"),
  "utf8",
);
const ts = readFileSync(resolve(root, "src/lib/api/rcNetDiag.ts"), "utf8");

/** 从 Rust `pub struct NetDiagSnapshot { … }` 块里抽字段名（`pub name: Type,`）。 */
function rustStructFields(src: string, name: string): string[] {
  const m = src.match(new RegExp(`pub struct ${name} \\{([\\s\\S]*?)\\n\\}`));
  if (!m) throw new Error(`Rust 源里找不到 pub struct ${name}`);
  return [...m[1].matchAll(/^\s*pub (\w+):/gm)].map((x) => x[1]!);
}

/** 从 TS `export interface NetDiagSnapshot { … }` 块里抽字段名（`name?:`）。 */
function tsInterfaceFields(src: string, name: string): string[] {
  const m = src.match(
    new RegExp(`export interface ${name} \\{([\\s\\S]*?)\\n\\}`),
  );
  if (!m) throw new Error(`TS 源里找不到 export interface ${name}`);
  return [...m[1].matchAll(/^\s*(\w+)\??:/gm)].map((x) => x[1]!);
}

describe("IPC 镜像契约：NetDiagSnapshot", () => {
  const rustFields = rustStructFields(rust, "NetDiagSnapshot");
  const tsFields = tsInterfaceFields(ts, "NetDiagSnapshot");

  it("前端声明的每个字段，Rust 快照都必须真的下发", () => {
    const missing = tsFields.filter((f) => !rustFields.includes(f));
    expect(
      missing,
      `前端 rcNetDiag.ts 声明了、但 Rust diagnostics.rs 没下发的字段：\
${missing.join(", ")}——后端少字段时前端读恒 null/undefined，且编译器毫无感知`,
    ).toEqual([]);
  });

  it("🔴 home_relay / home_relay_connected 两侧都在（P0 C1 的钉子）", () => {
    expect(rustFields).toContain("home_relay");
    expect(tsFields).toContain("home_relay");
    expect(rustFields).toContain("home_relay_connected");
    expect(tsFields).toContain("home_relay_connected");
  });

  it("守卫自身没失明：两侧解析出的字段数都非零", () => {
    expect(rustFields.length).toBeGreaterThan(8);
    expect(tsFields.length).toBeGreaterThan(8);
  });
});
