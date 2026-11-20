/**
 * 壳层通用纯函数。
 *
 * 🔴 2026-11-20 体检清仓（见 docs/项目体检报告-2026-11-20.md §1.3）：本文件曾是
 * PastePanda 迁移的杂物抽屉（1343 行 / 99 个导出），其中 77 个导出在本仓
 * 零生产引用（history/knowledge/OCR/barcode/hotkey/recording 等主题全数死亡；
 * 普查含 `src` 与 `src-mobile`）。现只保留真实在用的 22 个；新主题一律
 * 另开文件（CLAUDE.md 规则），不要再往这里堆。
 */
import { invoke } from "@tauri-apps/api/core";
import { logger } from "@/lib/logger";

export type PermissionContext = "general" | "file-receive" | "file-send" | "camera";

/** 先区分应用授权与系统文件访问，不能把 IPC 拒绝引导成「去系统开权限」。 */
export function permissionErrorInfo(error: unknown, context: PermissionContext = "general") {
  const raw = error instanceof Error ? `${error.name}: ${error.message}` : String(error || "");
  const denied = /permission|denied|notallowederror|eacces|eperm|access.*denied|权限不足|无权|拒绝访问/i.test(raw);
  const internal = /(?:tauri|invoke|command|plugin|capabilit|permissions?:\s*|\w+:allow-)/i.test(raw);
  if (internal && (denied || /not allowed|not permitted|forbidden|unauthorized/i.test(raw))) {
    return {
      kind: "internal" as const,
      title: "应用内部授权失败",
      detail: "请重新打开 DeskPanda 后重试；若仍失败，请记录此提示和操作步骤以便排查，无需调整手机权限。",
    };
  }
  if (!denied) return null;
  if (context === "camera" || /camera|摄像头|相机/i.test(raw)) {
    return {
      kind: "camera" as const,
      title: "未允许使用相机，无法扫码",
      detail: "打开手机「设置 → 应用 → DeskPanda → 权限 → 相机」，允许使用相机后返回重试；也可返回手动输入配对码。",
    };
  }
  if (context === "file-receive" || /接收目录|创建目录|create.*director/i.test(raw)) {
    return {
      kind: "file-receive" as const,
      title: "无法在当前接收位置保存文件",
      detail: "请重置接收位置，改用应用默认目录后再取文件。",
    };
  }
  if (context === "file-send" || /读不到|读取文件|read.*file/i.test(raw)) {
    return {
      kind: "file-send" as const,
      title: "无法读取所选文件",
      detail: "请重新选择文件；若文件来自网盘，请先下载到本机再发送。",
    };
  }
  return {
    kind: "unknown" as const,
    title: "此次操作未获允许",
    detail: "请重试一次；若仍失败，请记录刚才的操作和提示内容以便排查，暂时无需修改系统权限。",
  };
}

/** 设备行与详情共用的可连接状态。探测只证明当时拨得通，不改写历史在线状态。 */
export function rcDeviceStatus(
  presence: string | undefined,
  check: { state: "checking" | "reachable" | "unreachable" | "error"; checkedAt?: number } | undefined,
  channelUp: boolean | null,
  now = Date.now(),
): { label: string; tone: "ok" | "checking" | "unknown"; checkedAt?: number } {
  if (channelUp === null) return { label: "正在获取状态", tone: "checking" };
  if (!channelUp) return { label: "状态无法获取 · 通道未启动", tone: "unknown" };
  if (check?.state === "checking") return { label: "正在确认是否可连接", tone: "checking" };
  const fresh = check?.checkedAt != null && now - check.checkedAt < 45_000;
  if (fresh) {
    if (check.state === "reachable") return { label: "刚刚可连接", tone: "ok", checkedAt: check.checkedAt };
    if (check.state === "unreachable") return { label: "暂时连不上", tone: "unknown", checkedAt: check.checkedAt };
    if (check.state === "error") return { label: "状态无法获取", tone: "unknown", checkedAt: check.checkedAt };
  }
  if (presence === "live") return { label: "局域网在线", tone: "ok" };
  // U8：原「尚未确认」是实现语义（「还没探测」）；用户视角是「上一秒还在，现在没实测」
  return { label: "最近在线 · 未实测", tone: "unknown" };
}

/** 设备短连接确认的本地时间，用于列表与详情统一提示时效。 */
export function rcCheckTime(checkedAt: number): string {
  return new Date(checkedAt).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit", hour12: false });
}

/**
 * 解析库内时间字符串（`"2026-10-24 09:20:01"`，Rust 落库格式）→ 毫秒数。
 *
 * 🔴 **历史时间的唯一解析口径**（规则 11 收口）：此前 events.ts 手拆一份、
 * appStore/TrashPanel/milestones 各写一份 `replace(" ", "T")`、relativeTime
 * 裸 `new Date(s)`——裸解析对带空格格式在 Safari/WebKit 返回 Invalid Date
 * （macOS 上全量卡片时间变「NaN月NaN日 周undefined」）。
 *
 * 手拆而不是 `new Date(s)`：不依赖各引擎对非 ISO 格式的宽容度。
 * 解不出来返 `NaN`，调用方自判。
 */
export function parseHistoryTime(s: string): number {
  const m = /^(\d{4})-(\d{2})-(\d{2})[ T](\d{2}):(\d{2}):(\d{2})/.exec(s);
  if (!m) return NaN;
  return new Date(+m[1], +m[2] - 1, +m[3], +m[4], +m[5], +m[6]).getTime();
}

/**
 * 配对码 → 人眼比对用的展示形态（4 位一组，如 `4182 0620`）。
 *
 * 🔴 **三条配对路共用的唯一展示口径**（规则 11 收口，2026-09-29）：
 * ① 附近配对的两端核对码、② 跨网凭证出示的 8 位码、③ 输入凭证的预览回显。
 * 此前三处各自 `slice(0,4) + " " + slice(4)`，一旦有位数的第二来源就会分叉。
 *
 * 分组宽度 4 而不是 3：配对码是 **8 位**（`lan_pair::PAIR_CODE_DIGITS`），
 * 4+4 正好两半，与「前一半 / 后一半」的念法对齐；3+3+2 是电话号码的读法，
 * 用在这里会让人念错。
 *
 * 非数字字符（空串、半截输入）原样返回——**不抛异常、不补零**：
 * 输入框正打着「604」时显示「604」比显示「0604」更贴近用户刚敲的东西。
 */
export function formatPairCode(code: string): string {
  const digits = code.replace(/\D/g, "");
  if (digits.length <= 4) return digits;
  const chunks: string[] = [];
  for (let i = 0; i < digits.length; i += 4) chunks.push(digits.slice(i, i + 4));
  return chunks.join(" ");
}

/**
 * 配对凭证 → 8 位核对数（FNV-1a 32 位取末 8 位）。
 *
 * 用来把「二维码 / 那一长串接入码」折成一个和**附近配对**同规格的 8 位数:
 * 用户在两条路上看到的都是 8 位、都是 4+4 分组，不需要先判断自己在走哪条路
 * （2026-09-29 统一入口的诉求）。展示一律走 [`formatPairCode`]。
 *
 * 🔴 **它不新增安全强度**，写在这里免得后人误读：
 * 凭证本身是自签的（`sync/invite.rs` 模块头论证过，中间人换成自己那份时
 * 两端折出来的数照样一致）。中间人防护仍然是两件已有的事——
 * ① 生成方那一侧的确认（写白名单的一侧）＋ 指纹；② 附近配对的 SAS 数字
 * （换公钥 → 两端必不一致）。本函数只解决「三种长度的码并存」的一致性问题。
 *
 * 纯函数、无环境依赖：**两端各算各的**（生成方折自己的码，输入方折刚粘的码），
 * 同一串输入必然同一个结果，因此不需要后端参与，也没有第二处口径。
 */
export function pairCodeDigest(code: string): string {
  let h = 0x811c9dc5; // FNV-1a offset basis（32 位）
  for (let i = 0; i < code.length; i++) {
    // 直接吃整个 UTF-16 码元（不做 & 0xff 掩码）：凭证是 ASCII（`PP1-` +
    // base58 + CRC），掩码只会让非 ASCII 输入白白多撞车。移动端实现同一口径
    // 时逐码元 XOR 即可，不必引 TextEncoder。
    h ^= code.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0; // FNV prime；>>> 0 保持无符号
  }
  return String(h % 100000000).padStart(8, "0");
}

/** 截断文本 */
export function truncate(text: string, maxLen: number): string {
  if (!text) return "";
  const cleaned = text.replace(/\r/g, " ").replace(/\n/g, " ");
  // 按 Unicode 码点切分，避免 UTF-16 代理对（如 emoji）被从中间切断产生孤立代理项
  const codePoints = Array.from(cleaned);
  return codePoints.length > maxLen ? codePoints.slice(0, maxLen).join("") + "..." : cleaned;
}

/**
 * 字节数 → 人类可读（B/KB/MB/GB）。**全项目唯一权威版**（规则 11 收口，
 * 2026-09-22）：此前 `lib/rcFile.ts` 与 `lib/imageFormat.ts` 各有一份、
 * `JsonBody.tsx` 还有一份无守卫内联拷贝，三份口径互不一致。
 *
 * 语义（取 rcFile 版超集——文件传输通道上限 8 GiB，必须留 GB 档）：
 * - `0` 是**合法值**（空文件），返回 `"0 B"`，不是「无」；
 * - 非法输入（负数 / NaN / ±∞）返回 `"—"`——那才是「没有数据」。
 * 旧 `imageFormat` 版把 0 与非法混为 `"—"`、MB 两位小数，已统一到本版。
 */
export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n < 0) return "—";
  if (n < 1024) return `${Math.round(n)} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

/** 速率 → `6.2 MB/s`。低于 1 KB/s 显示 `—`（还没量到，别报假数）。 */
export function formatRate(bytesPerSec: number): string {
  if (!Number.isFinite(bytesPerSec) || bytesPerSec < 1024) return "—";
  return `${formatBytes(bytesPerSec)}/s`;
}

/**
 * 把 catch 到的未知错误转成可展示文案。
 *
 * Tauri 的 invoke 失败时抛的是**字符串**（Rust 端 `Err(String)`），
 * 而前端自己的异常抛的是 Error，两者得分开取：
 * 直接 String(e) 会把 Error 变成 "Error: xxx"，把普通对象变成 "[object Object]"。
 */
export function errText(e: unknown, fallback: string): string {
  if (typeof e === "string" && e.trim()) return e;
  if (e instanceof Error && e.message) return e.message;
  const s = e == null ? "" : String(e);
  return s && s !== "[object Object]" ? s : fallback;
}

/**
 * 复制文本到剪贴板。
 *
 * **优先走 Rust `copy_only`**（src-tauri/src/commands/paste.rs → paste_engine::copy_only）：
 * 它有 10 次递增退避重试、真实的错误信号，以及写后回读校验，还能向剪贴板监听线程
 * 报备「这是应用自己写的」。WebView2 的 `navigator.clipboard.writeText` 在窗口未聚焦 /
 * 失去用户激活时会 **resolve 却不写**（假成功）——前端据此点亮「✓ 已复制」，
 * 用户粘出来还是旧内容；这个坑在图片 OCR 面板的「复制全部」上被真实碰到过。
 *
 * 兜底顺序：
 * - 移动端 / 无 Tauri 环境 → `copy_only` 返回 Err → 落到 `writeText`；
 * - `copy_only` 报「回读不一致」→ **不再回落**，直接判失败：剪贴板已被别的程序抢占，
 *   此时 Web API 的 resolve 同样不可信，回落只会把失败又伪装成成功。
 *
 * 返回是否成功，便于调用方做内联反馈（如 toast 内的「已复制 ✓」）。
 * 收口缘由（规则 #11）：全项目 40+ 处内联 `navigator.clipboard` 散落，
 * 统一经此函数，避免重复且保证兜底一致。
 */
export async function copyToClipboard(text: string): Promise<boolean> {
  try {
    await invoke("copy_only", { text });
    return true;
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    // 回读不一致 = 写入没落地，别再拿浏览器的 resolve 当成功（见上方注释）。
    if (msg.includes("回读不一致")) {
      logger.warn("[copy] 原生写入后回读不一致：", msg);
      return false;
    }
    logger.warn("[copy] 原生复制不可用，回落 Web Clipboard：", msg);
  }
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

/**
 * 远端光标形状 → CSS `cursor` 值（被控端 `rc-cursor-changed` 事件携带）。
 *
 * 桌面与手机端**共用**这一个映射（规则 11 单一数据源）：桌面把它设到
 * 画布上，手机端按它挑本地光标图形。桌面在 `hooks/useRcCursor.ts`，
 * 手机端在 `src-mobile/session/`。
 *
 * @returns CSS cursor 串；`null` = 没有等价的本地形状（arrow / unknown），
 *   调用方用自己的默认图形（桌面 B1 overlay 箭头、手机端 SVG 箭头）。
 */
export function cursorCssFor(shape: string | null): string | null {
  switch (shape) {
    case "ibeam":
      return "text";
    case "wait":
      return "wait";
    case "cross":
      return "crosshair";
    case "size_nwse":
      return "nwse-resize";
    case "size_nesw":
      return "nesw-resize";
    case "size_ns":
      return "ns-resize";
    case "size_we":
      return "ew-resize";
    case "size_all":
      return "move";
    case "no":
      return "not-allowed";
    case "hand":
      return "pointer";
    case "app_starting":
      return "progress";
    case "up_arrow":
      return "default";
    case "hidden":
      return "none";
    default:
      return null;
  }
}

/** Classify only reported platform data; a missing OS must not imply a computer. */
export function rcDeviceKind(os?: string | null): "phone" | "tablet" | "computer" | "unknown" {
  const platform = (os ?? "").trim().toLowerCase();
  if (/ipad|ipados|tablet/.test(platform)) return "tablet";
  if (/android|ios|iphone|harmonyos/.test(platform)) return "phone";
  if (/windows|macos|mac os|darwin|linux/.test(platform)) return "computer";
  return "unknown";
}

export function rcDeviceTypeLabel(os?: string | null): string {
  return { phone: "手机", tablet: "平板", computer: "电脑", unknown: "设备" }[rcDeviceKind(os)];
}

/** Shared presentation for RC codec notifications, HUD and waiting stage. */
export function remoteCodecLabel(codec: string): string {
  switch (codec) {
    case "h264": return "H.264";
    case "hevc": return "HEVC";
    case "av1": return "AV1";
    case "jpeg": return "JPEG";
    default: return "未知编码";
  }
}

/** Only text outcomes can be merged; rich messages keep their own source identity. */
export function mobileFeedbackIdentity(tone: string, title: unknown, detail: unknown): string | null {
  const text = (value: unknown) => value == null || typeof value === "string" || typeof value === "number";
  return text(title) && text(detail) ? JSON.stringify([tone, title ?? "", detail ?? ""]) : null;
}

/** Recovery actions also remain on their task; the toast is never their only entry. */
export function mobileFeedbackReadingTime(hasAction: boolean, hasDetail: boolean): number {
  return hasAction || hasDetail ? 6000 : 4000;
}

/** Constrain actual content, excluding centered object-fit padding, after zoom or viewport resize. */
export function clampRcViewportOffset(offset: number, size: number, scale: number, fittedSize = size): number {
  const inset = (size - fittedSize) / 2;
  const scaled = fittedSize * scale;
  if (scaled <= size) return (size - scaled) / 2 - inset * scale;
  return Math.max(size - (inset + fittedSize) * scale, Math.min(0 - inset * scale, offset));
}

/** 控制柄及邻近按钮留在可见区域；远端指针独立移动，仍可到达电脑画面边缘。 */
export function clampRcFloatingMousePosition(x: number, y: number, width: number, height: number, controlWidth = 208, controlHeight = 116) {
  const marginX = Math.min(controlWidth / 2 + 4, width / 2);
  const minY = Math.min(96, height / 2);
  return {
    x: Math.max(marginX, Math.min(width - marginX, x)),
    y: Math.max(minY, Math.min(Math.max(minY, height - (controlHeight - 28)), y)),
  };
}

// ==================== 壳子（Deskbase）保留的工具函数 ====================

export async function copyText(
  text: string,
  onSuccess: () => void,
  onError?: (e: unknown) => void,
): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    onSuccess();
  } catch (e) {
    onError?.(e);
  }
}

/** 字节数格式化为带单位的人类可读文本（B/KB/MB/GB），SecurityTab 与版本历史弹框共用（规则 11）。 */

export function formatBytesPerSec(bytesPerSec: number): string {
  return `${formatBytes(bytesPerSec)}/s`;
}

/**
 * 键盘事件是否发生在输入法组词（IME composition）过程中。
 *
 * 组词期间的 Enter 是「上屏候选词」、Esc 是「收起候选窗」、逗号是普通标点——
 * 都不是对当前 UI 的操作。所有 Enter/Esc/逗号 类提交处理的入口都应先用它放行，
 * 否则中文输入法会被快捷键劫持（候选词还没选，命令就执行了）。
 *
 * 两种判据并用的原因：Safari 的 keydown 在组词中 `isComposing` 恒为 false 但
 * keyCode 会变成 229；反之旧内核只置 `isComposing`。取并集最稳。
 * 鸭子类型（同 isEditableTarget）：React 合成事件传 `e.nativeEvent`，
 * 原生事件直接传 `e`，两者都具备这两个属性，也因此可在 node 单测里直接验证。
 */

export function isImeComposing(e: { isComposing?: unknown; keyCode?: unknown }): boolean {
  return e.isComposing === true || e.keyCode === 229;
}
