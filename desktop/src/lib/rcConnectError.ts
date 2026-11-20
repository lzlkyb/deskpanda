/**
 * 远程会话「连不上」错误的人话分类。
 *
 * 背景（docs/项目体检报告-2026-11-20.md §2 / 交接文档 §4.1）：rc 协议没有版本
 * 协商，版本号只写在 ALPN 尾缀（`pastepanda/rc/1`）——两端 DeskPanda 版本不
 * 兼容时连接在 QUIC 握手层就被拒，用户看到的却是 iroh 的裸错误串
 * （`aborted by peer … closed during the handshake` /
 * `error 120: peer doesn't support any known protocol`），完全指不到
 * 「两边版本不匹配」这个唯一可行动的原因（`net/endpoint.rs:5-15` 有实测记录）。
 *
 * 本模块只做**文案映射**，不猜测更多：两种已知形态 → 一句能行动的人话，
 * 原始串保留在后半段（排查时用户能复述给维护者）。其余错误原样透传，
 * 交由既有 `errText` 口径。
 */

/** 判据来自 net/endpoint.rs 的公网联调实录（2026-10-02），两端各见过一种。 */
export function rcConnectErrorText(error: unknown): string | null {
  const raw = typeof error === "string" ? error : error instanceof Error ? error.message : "";
  if (!raw) return null;
  // error 120 = 本机端点不认识对端的任何 ALPN——本机版本旧于对端。
  if (/error 120|doesn't support any known protocol/i.test(raw)) {
    return `连接失败：对方运行的是更新的 DeskPanda，本机版本过旧、协议不兼容。请把本机更新到最新版后再试。（原始错误：${raw}）`;
  }
  // aborted by peer during handshake = 对端端点不认识我们的 ALPN——对端版本旧于本机
  // （或对端远程功能未开启；那句另有提示，这里的判据限定在握手期被拒）。
  if (/aborted by peer[\s\S]*handshake|closed during the handshake/i.test(raw)) {
    return `连接失败：对方使用的 DeskPanda 版本可能过旧，与当前协议不兼容。请让对方更新到最新版后再试。（原始错误：${raw}）`;
  }
  return null;
}

/** 给 rcStore 的发起动作用：命中已知形态就替换文案，否则原样抛出。 */
export function mapConnectError(error: unknown): never {
  const text = rcConnectErrorText(error);
  if (text) throw new Error(text);
  throw error instanceof Error ? error : new Error(String(error ?? ""));
}
