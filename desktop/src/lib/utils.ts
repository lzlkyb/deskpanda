import { invoke } from "@tauri-apps/api/core";
import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";
import { motionPreference } from "@/lib/motionPreference";
import { parseEventRange } from "@/lib/eventLabel";
import { logger } from "@/lib/logger";

/** Time navigation operates on local calendar dates, including DST and year boundaries. */
export function historyLocalDate(date: Date): string {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}

export function historyDateOffset(day: string, offset: number): string {
  const [y, m, d] = day.split("-").map(Number);
  return historyLocalDate(new Date(y, m - 1, d + offset));
}

export function historyTimeWindow(day: string, startHour = 0, endHour = 24): `window:${string}` {
  const [y, m, d] = day.split("-").map(Number);
  const at = (hour: number) => {
    const date = new Date(y, m - 1, d, hour);
    return `${historyLocalDate(date)} ${String(date.getHours()).padStart(2, "0")}:00:00`;
  };
  return `window:${at(startHour)}~${at(endHour)}`;
}

export function parseHistoryWindow(value: string): { start: string; end: string } | null {
  const match = /^window:(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})~(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})$/.exec(value);
  if (!match || match[1] >= match[2]) return null;
  return { start: match[1], end: match[2] };
}

/** One gate for fetching, displaying and paginating database-backed history queries. */
export function isHistoryQueryActive(search: string, timeFilter: string, sourceFilter: string): boolean {
  return !!search.trim() || timeFilter !== "all" || !!sourceFilter;
}

export function historyTimeLabel(value: string, now = new Date()): string {
  if (value === "all") return "全部时间";
  if (value === "today") return "今天";
  if (value === "week") return "近 7 天";
  if (value === "month") return "近 30 天";
  const range = parseHistoryWindow(value) ?? parseEventRange(value);
  if (!range) return "时间";
  const day = range.start.slice(0, 10);
  const monday = historyDateOffset(historyLocalDate(now), -((now.getDay() + 6) % 7));
  if (range.start === `${monday} 00:00:00` && range.end === `${historyDateOffset(monday, 7)} 00:00:00`) return "本周";
  const prefix = day === historyLocalDate(now) ? "今天" : day === historyDateOffset(historyLocalDate(now), -1) ? "昨天"
    : `${day.slice(0, 4) === String(now.getFullYear()) ? "" : `${day.slice(0, 4)} 年 `}${Number(day.slice(5, 7))} 月 ${Number(day.slice(8, 10))} 日`;
  const start = range.start.slice(11, 16);
  const end = range.end.slice(0, 10) === historyDateOffset(day, 1) && range.end.slice(11) === "00:00:00" ? "24:00" : range.end.slice(11, 16);
  if (start === "00:00" && end === "24:00") return prefix;
  if (range.end.slice(0, 10) !== day && end !== "24:00") return `${prefix}起的时间范围`;
  const slot = start === "00:00" && end === "12:00" ? "上午" : start === "12:00" && end === "18:00" ? "下午" : start === "18:00" && end === "24:00" ? "晚上" : `${start}–${end}`;
  return `${prefix} · ${slot}`;
}

/** Parsing a list is not evidence of query intent. Keep manual conversion separate. */
export function sqlListRelevance(values: unknown[]): { score: number; text: string } {
  const parts = values.map(value => String(value).trim());
  if (parts.length < 2) return { score: 0, text: "没有足够的数据值" };
  const prose = parts.some(value =>
    /[。！？!?]/.test(value) ||
    /^(如果|但是|因为|所以|而且|不过|然后|请|现在|我们|用户).{3,}/.test(value) ||
    /\b(if|but|because|please|however|then|we|you|let me)\b/i.test(value) ||
    (value.length >= 10 && /(需要|应该|不会|不能|正在|已经|可以|没有)/.test(value)) ||
    value.split(/\s+/).length >= 5);
  if (prose) return { score: 0, text: "内容更像连续叙述，不自动推荐 SQL IN" };
  const identifiers = parts.every(value => /^-?\d+(?:\.\d+)?$/.test(value)) ||
    parts.every(value => /^[A-Za-z0-9_-]+$/.test(value) && /\d/.test(value));
  if (identifiers) return { score: 0.85, text: `识别到 ${parts.length} 个数字或短标识符，保留原始数据值` };
  return { score: 0.45, text: `识别到 ${parts.length} 个名称或文本值，查询意图尚不明确` };
}

/** Strict round trip and readable UTF-8 prevent ordinary ASCII becoming a decode suggestion. */
export function decodeBase64Text(input: string): string | null {
  const clean = input.replace(/\s/g, "").replace(/-/g, "+").replace(/_/g, "/");
  if (!/^[A-Za-z0-9+/]+={0,2}$/.test(clean) || clean.length % 4 === 1) return null;
  try {
    const binary = atob(clean);
    if (btoa(binary).replace(/=+$/, "") !== clean.replace(/=+$/, "")) return null;
    const text = new TextDecoder("utf-8", { fatal: true }).decode(Uint8Array.from(binary, char => char.charCodeAt(0)));
    // Binary control bytes are not a useful text decode result; tab/newline remain valid.
    // eslint-disable-next-line no-control-regex
    return text.trim() && !/[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]/.test(text) ? text : null;
  } catch { return null; }
}

/** Imperative animations use the same gate as CSS and Framer, including hidden WebViews. */
export function canAnimateInterface(): boolean {
  return motionPreference.getSnapshot() && !document.hidden
    && (!document.documentElement.dataset.interfaceMotion || document.documentElement.dataset.interfaceMotion === "full")
    && !window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** 合并 Tailwind 类名，自动去重和解决冲突 */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
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

/** Local knowledge failures must not expose note contents or private filesystem paths. */
export function knowledgeErrorText(error: unknown): string {
  const raw = error instanceof Error ? error.message : String(error ?? "");
  if (raw.startsWith("MOBILE_EDIT_REJECTED:folder_missing")) return "原文件夹已被移除，请重新选择文件夹后保存。";
  if (raw.startsWith("MOBILE_EDIT_REJECTED:tag_missing")) return "所选标签已被移除，请重新选择标签后保存。";
  if (raw.startsWith("MOBILE_EDIT_REJECTED:title_empty")) return "请填写标题，再保存修改。";
  if (/disk.*full|storage.*full|database or disk is full|no space|空间不足/i.test(raw))
    return "手机存储空间不足，请腾出空间后重试。";
  if (/locked|busy|正在保存|版本已更新/i.test(raw))
    return "内容正在保存或已更新，请稍后重试。";
  if (/invoke|tauri_internal|__TAURI|not found.*command/i.test(raw))
    return "无法连接应用后台，请在最新版 DeskPanda 应用内使用。";
  return "本机操作未能完成，请重试。已有内容不会因这次失败被删除。";
}

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

/** 相对时间格式化 */
export function relativeTime(timeStr: string): string {
  if (!timeStr) return "";
  const now = new Date();
  const t = parseHistoryTime(timeStr);
  if (Number.isNaN(t)) return timeStr; // 脏数据回显原文，而不是「NaN月NaN日 周undefined」
  const diffMs = now.getTime() - t;
  const diffMin = Math.floor(diffMs / 60000);
  const diffHour = Math.floor(diffMs / 3600000);
  const diffDay = Math.floor(diffMs / 86400000);

  if (diffMin < 1) return "刚刚";
  if (diffMin < 60) return `${diffMin}分钟前`;
  if (diffHour < 24) return `${diffHour}小时前`;
  if (diffDay === 1) return "昨天";
  if (diffDay < 7) return `${diffDay}天前`;

  const date = new Date(t);
  const month = date.getMonth() + 1;
  const day = date.getDate();
  const weekdays = ["日", "一", "二", "三", "四", "五", "六"];
  const weekday = weekdays[date.getDay()];
  return `${month}月${day}日 周${weekday}`;
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
 * 用来把「二维码 / 那一长串接入码」折成一个和**附近配对**同规格的 8 位数：
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

/** Storage labels use IEC units; the shared formatter already divides by 1024. */
export function formatStorageBytes(bytes: number): string {
  return formatBytes(bytes).replace(/\b(K|M|G)B\b/g, "$1iB");
}

export function storageTableBytes(table: { table_bytes: number | null; index_bytes: number | null }): number | null {
  return table.table_bytes === null || table.index_bytes === null ? null : table.table_bytes + table.index_bytes;
}

/** 剥离 HTML 标签，返回纯文本 */
export function stripHtml(text: string): string {
  try {
    const doc = new DOMParser().parseFromString(text, "text/html");
    return (doc.body.textContent || "").replace(/\n{3,}/g, "\n\n").trim();
  } catch {
    // 兜底：正则剥离
    return text.replace(/<[^>]*>/g, "").replace(/&nbsp;/g, " ").replace(/&amp;/g, "&").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, '"').trim();
  }
}

/** 类型图标配置 — SVG 路径数据 */
export const TYPE_ICONS: Record<string, { svgPath: string; color: string }> = {
  text:    { svgPath: "M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01", color: "#9E9E9E" },
  link:    { svgPath: "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71", color: "#07C160" },
  email:   { svgPath: "M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.5 19.5 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72c.127.96.361 1.903.7 2.81a2 2 0 0 1-.45 2.11L8.09 9.91a16 16 0 0 0 6 6l1.27-1.27a2 2 0 0 1 2.11-.45c.907.339 1.85.573 2.81.7A2 2 0 0 1 22 16.92z", color: "#007AFF" },
  code:    { svgPath: "M16 18l6-6-6-6M8 6l-6 6 6 6", color: "#5856D6" },
  phone:   { svgPath: "M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.5 19.5 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72c.127.96.361 1.903.7 2.81a2 2 0 0 1-.45 2.11L8.09 9.91a16 16 0 0 0 6 6l1.27-1.27a2 2 0 0 1 2.11-.45c.907.339 1.85.573 2.81.7A2 2 0 0 1 22 16.92z", color: "#FF9500" },
  image:   { svgPath: "M3 3h18v18H3V3zm0 0l13 10-5 4-3-2-5 4", color: "#FF9500" },
  file:    { svgPath: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8l-6-6zm0 0v6h6", color: "#FFCC00" },
};

// ==================== AI 用量估算 ====================

/**
 * 粗略估算一段文本大约占多少 token。
 *
 * 用途：变换卡在“还没发送”时告诉用户这次大概多大体量，好判断要不要真发。
 *
 * 算法：CJK（中日韩）按 1.5 token/字，其余字符按 4 字符/token。
 *
 * **这只是量级参考，不是准确值**：真实分词由各家 tokenizer 决定，
 * 同一段中文在不同服务商下可以差到 ±50%（DeepSeek/通义这类中文词表明显更省）。
 * 所以**展示时必须带“≈”**，不能拿它去算钱。
 *
 * 用 for...of 而不是下标遍历：后者会把 emoji 等代理对拆成两个“字符”。
 */
export function estimateTokens(text: string): number {
  if (!text) return 0;
  let cjk = 0;
  let other = 0;
  for (const ch of text) {
    if (/[一-鿿぀-ヿ가-힯]/.test(ch)) cjk++;
    else other++;
  }
  return Math.max(1, Math.round(cjk * 1.5 + other / 4));
}

/** 字数（按**码点**数，emoji 算 1 个）。与 estimateTokens 配套展示。
 *  不用 text.length：它数的是 UTF-16 代码单元，一个 emoji 会被算成 2。 */
export function countChars(text: string): number {
  return text ? [...text].length : 0;
}

/** 紧凑计数：上万不写全，1234 → `1.2k`。
 *
 * 收口缘由（规则 #11）：原本只存在于 `DocEditor.tsx` 里的一个局部箭头函数，
 * 笔记行的字数条要的是同一个格式。两份数字格式化必定会分岔（一边改阀值、
 * 一边改小数位），而分岐后双方看上去都「没错」，根本不会有人发现。 */
export function fmtCount(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n);
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
 * 解析 file / image 条目的 `content` 字段为路径数组。
 *
 * `content` 的存法不止一种（历史原因）：JSON 数组、JSON 单字符串、
 * 裸路径、换行分隔的多路径都可能，所以四种都得接。
 *
 * 收口缘由（规则 #11）：之前共存三份实现且**行为不一致**——
 * pasteTransform 那份 JSON 解析失败后按换行切分，Card 那两份直接把整个
 * content 当单路径，于是同一条多文件记录在不同地方能解出不同的个数。
 * 这里取并集：先 JSON，再按换行切，每项 trim 后去空。
 */
export function parseFilePaths(content: string): string[] {
  if (!content) return [];
  try {
    const parsed: unknown = JSON.parse(content);
    if (Array.isArray(parsed)) {
      // 只收字符串项：Card 那份用的是 map(String)，会把数字/null 变成
      // "1"/"null" 当成路径交下去。丢掉非字符串才是对的。
      return parsed.filter((p): p is string => typeof p === "string").map((p) => p.trim()).filter(Boolean);
    }
    if (typeof parsed === "string" && parsed.trim()) return [parsed.trim()];
  } catch {
    /* 不是 JSON，当纯路径处理 */
  }
  return content
    .split("\n")
    .map((p) => p.trim())
    .filter(Boolean);
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

/** OCR 文本里可被"识别即操作"的实体类型。 */
export type OcrEntityType = "url" | "phone" | "email";

export interface OcrEntity {
  type: OcrEntityType;
  value: string;
  start: number;
  end: number;
}

/**
 * 从文本中提取 URL / 手机号 / 邮箱（本地正则，零出网，符合隐私红线）。
 *
 * 仅用保守规则避免误判：
 * - URL: `https?://` 起手
 * - 手机号: 大陆 11 位 `1[3-9]\d{9}`（前后非数字）
 * - 邮箱: 标准 `name@host.tld`
 * 不识别纯连续数字串（发票号/订单号等），避免把普通数字当电话。
 *
 * 多规则命中时按 start 排序并去重叠（URL 里的数字不会被判成电话）。
 */
export function extractEntities(text: string): OcrEntity[] {
  const raw: OcrEntity[] = [];
  const collect = (type: OcrEntityType, re: RegExp) => {
    re.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = re.exec(text)) !== null) {
      raw.push({ type, value: m[0], start: m.index, end: m.index + m[0].length });
    }
  };
  collect("url", /https?:\/\/[^\s]+/gi);
  collect("email", /[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}/g);
  collect("phone", /(?<!\d)1[3-9]\d{9}(?!\d)/g);
  collect("phone", /(?<!\d)0\d{2,3}[\s-]\d{7,8}(?!\d)/g);

  raw.sort((a, b) => a.start - b.start || a.end - b.end);
  const out: OcrEntity[] = [];
  let lastEnd = -1;
  for (const e of raw) {
    if (e.start >= lastEnd) {
      out.push(e);
      lastEnd = e.end;
    }
  }
  return out;
}

/**
 * 从图片条目的占位文本（如 `[图片] 554x265`）提取尺寸。
 * 匹配失败返回 null（非图片占位、格式异常都视为无尺寸信息）。
 * 收口缘由（规则 #11）：占位格式由后端 clipboard_monitor 写入，
 * 需要显示尺寸的调用点（主窗口卡片，后续托盘弹窗等）统一走这里，不各自解析。
 */
export function parseImagePlaceholderSize(text: string): { width: number; height: number } | null {
  const m = /^\[图片\]\s*(\d+)\s*[xX×]\s*(\d+)/.exec((text || "").trim());
  if (!m) return null;
  return { width: Number(m[1]), height: Number(m[2]) };
}

/** 图片卡片 OCR 状态（由 useCardOcr 产出，后端回填的 ocr_text 不属于此状态）。 */
export type ImageOcrState = {
  /** idle=未发起（排队中）；ocr=识别中；done=已出结果；fail=识别失败 */
  status: "idle" | "ocr" | "done" | "fail";
  /** 识别文本；仅 status=done 时有意义（空串=识别过但无文字） */
  text?: string;
};

/** 图片卡片标题/副行显示所需的最小输入（Card.tsx 的 item 子集）。 */
export interface ImageCardDisplayInput {
  type: string;
  text: string;
  content: string;
  ocr_text?: string;
}

export interface ImageCardDisplay {
  /** 卡片标题（cardTitle）最终文本 */
  title: string;
  /** 副行尺寸徽标文本（如 "554×265"），无尺寸信息时为 undefined */
  sizeText?: string;
  /** OCR 徽标文本（如 "已识别 48 字"），仅识别成功且有文字时出现 */
  ocrLabel?: string;
}

/**
 * 图片卡片显示决策（纯函数，供 Card.tsx 渲染 + 单测）。
 *
 * 状态优先级（与后端持久化语义严格对应）：
 * 1. 有 OCR 文本（后端回填 ocr_text 非空，或前端识别完成非空）→ 标题=OCR 文本
 * 2. 识别过但无文字（ocr_text === "" 或前端识别完成空串）→ 标题="未识别到文字"
 * 3. 正在识别 → 标题="识别图片文字中…"
 * 4. 其余（未识别/失败/无路径）→ 标题=文件名（content basename），回退「图片」
 *
 * 尺寸从占位文本 `[图片] WxH` 提取，独立于 OCR 状态——没有 OCR 结果也显示。
 */
export function resolveImageCardDisplay(item: ImageCardDisplayInput, ocrState?: ImageOcrState): ImageCardDisplay {
  const size = parseImagePlaceholderSize(item.text || "");
  const sizeText = size ? `${size.width}×${size.height}` : undefined;

  // 后端 Rust Option::None 序列化为 null（不是字段缺失），必须同时排除 null 与 undefined。
  // 后端回填（含空串）是持久化权威；前端状态只在后端没给（新条目）时兜底。
  const ocrText = item.ocr_text != null ? item.ocr_text : ocrState?.status === "done" ? ocrState.text : undefined;

  // 与 Card.tsx 原 title 逻辑同口径的 500 字符截断：OCR 全文可能很长，
  // 超长文本进 DOM / 高亮 split 会拖垮列表（M24 同类问题）。
  const clamp = (t: string) => {
    const flat = t.slice(0, 501).replace(/\r?\n/g, " ").trim();
    return flat.length > 500 ? flat.slice(0, 500) + "…" : flat;
  };

  if (ocrText !== undefined && ocrText !== "") {
    return { title: clamp(ocrText), sizeText, ocrLabel: `已识别 ${ocrText.length} 字` };
  }
  if (ocrText === "") {
    return { title: "未识别到文字", sizeText };
  }
  if (ocrState?.status === "ocr") {
    return { title: "识别图片文字中…", sizeText };
  }
  const name = (item.content || "").split(/[/\\]/).pop();
  return { title: name || "图片", sizeText };
}

/**
 * 获取图片条目的**完整** OCR 文本（复制用，非截断标题）。
 *
 * 与 resolveImageCardDisplay 同源同判据：后端回填（item.ocr_text）优先，
 * 前端实时识别结果（ocrState）兜底；两者都无或为 null → 返回 null（无文字）。
 * 空串表示「识别过但无文字」，同样返回 null——没有可复制的内容。
 *
 * 收口缘由（规则 #11）：Popover「复制文字」按钮与右键「复制识别文字」项
 * 必须复制同一份完整文本，禁止各自取截断标题或各写判断。
 */
export function getImageOcrFullText(
  item: ImageCardDisplayInput,
  ocrState?: ImageOcrState,
): string | null {
  // 同 resolveImageCardDisplay 的合并逻辑；!= null 兼容后端序列化的 null
  const text = item.ocr_text != null ? item.ocr_text : ocrState?.status === "done" ? ocrState.text : undefined;
  if (text === undefined || text === "") return null;
  return text;
}

// ========== 二维码/条码识别（本地解码，不联网——规则 16.4 同 OCR 不受 AI 门控） ==========

/** 与后端 Rust BarcodeHit 一一对应（commands/barcodes.rs 序列化结果）。 */
export interface BarcodeHit {
  /** rxing BarcodeFormat 的 Display 小写名，如 "qrcode" / "code128" */
  format: string;
  text: string;
  /** 包围盒（图片像素坐标）：[[minX,minY],[maxX,minY],[maxX,maxY],[minX,maxY]] */
  points: [number, number][];
}

/** 图片卡片条码解码状态（由 useCardBarcodes 产出；后端回填 barcodes 不走此状态）。 */
export type ImageBarcodeState = {
  /** idle=排队中；decode=解码中；done=已出结果；fail=解码失败 */
  status: "idle" | "decode" | "done" | "fail";
  /** 解码结果；仅 status=done 时有意义（空数组=解码过但无码） */
  hits?: BarcodeHit[];
};

/** 图片条码显示所需的最小输入（Card.tsx 的 item 子集）。 */
export interface ImageBarcodeInput {
  barcodes?: BarcodeHit[] | null;
}

// 2D 码制集合（徽章上统称「QR」，其余归「条码」）。
const QR_2D_FORMATS = new Set([
  "qrcode", "microqrcode", "rmqrcode", "datamatrix", "aztec", "pdf417", "maxicode",
]);

/** 是否 2D 码（QR/DataMatrix/PDF417 等）；其余按 1D 条码归类。 */
export function isQrBarcodeFormat(format: string): boolean {
  return QR_2D_FORMATS.has((format || "").toLowerCase());
}

/** 码制 → 显示标签（设计稿形态：QRCode / Code128；后端 rxing Display 是小写名）。 */
const BARCODE_FORMAT_LABELS: Record<string, string> = {
  qrcode: "QRCode",
  microqrcode: "Micro QRCode",
  rmqrcode: "rMCode",
  datamatrix: "DataMatrix",
  aztec: "Aztec",
  pdf417: "PDF417",
  maxicode: "MaxiCode",
  code128: "Code128",
  code39: "Code39",
  code93: "Code93",
  codabar: "Codabar",
  itf: "ITF",
  ean13: "EAN-13",
  ean8: "EAN-8",
  upca: "UPC-A",
  upce: "UPC-E",
};

export function barcodeFormatLabel(format: string): string {
  const key = (format || "").toLowerCase();
  return BARCODE_FORMAT_LABELS[key] ?? format.toUpperCase();
}

/**
 * 合并「后端回填 barcodes」与「前端实时解码状态」，返回最终码列表。
 * 与 getImageOcrFullText 同判据：后端 != null 是持久化权威（含空数组=解过无码），
 * 前端状态只在后端没给（新条目）且解码完成时兜底。
 * 返回 null = 尚无结论（未解码/解码中/失败），调用方据此不显示徽章。
 */
export function getImageBarcodes(
  item: ImageBarcodeInput,
  barcodeState?: ImageBarcodeState,
): BarcodeHit[] | null {
  if (item.barcodes != null) return item.barcodes;
  if (barcodeState?.status === "done") return barcodeState.hits ?? [];
  return null;
}

/**
 * 卡片角标文本（设计稿：`▦ QR ×1 · 条码 ×1`）。
 * 无码 / 结论未出 → undefined（徽章不渲染）；未知码制统称「码」。
 */
export function barcodeBadgeLabel(hits: BarcodeHit[] | null): string | undefined {
  if (!hits || hits.length === 0) return undefined;
  let qr = 0;
  let bar = 0;
  let other = 0;
  for (const h of hits) {
    const f = (h.format || "").toLowerCase();
    if (isQrBarcodeFormat(f)) qr++;
    else if (BARCODE_FORMAT_LABELS[f]) bar++;
    else other++;
  }
  const parts: string[] = [];
  if (qr > 0) parts.push(`QR ×${qr}`);
  if (bar > 0) parts.push(`条码 ×${bar}`);
  if (other > 0) parts.push(`码 ×${other}`);
  const label = `▦ ${parts.join(" · ")}`;
  return label.length > 18 ? `▦ 码 ×${hits.length}` : label;
}

/** 码列表按行拼接（「复制全部」用）。空列表返回空串。 */
export function joinBarcodeTexts(hits: BarcodeHit[]): string {
  return hits.map((h) => h.text).join("\n");
}

/** points 包围盒 → CSS 定位用的 {x,y,width,height}（图片像素坐标）。 */
export function barcodeBoundingRect(points: [number, number][]): { x: number; y: number; width: number; height: number } | null {
  if (!points || points.length === 0) return null;
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const [x, y] of points) {
    if (x < minX) minX = x;
    if (x > maxX) maxX = x;
    if (y < minY) minY = y;
    if (y > maxY) maxY = y;
  }
  if (!Number.isFinite(minX)) return null;
  return { x: minX, y: minY, width: maxX - minX, height: maxY - minY };
}

/** 是否 http/https 链接（预览抽屉「↗ 打开链接」仅对 URL 出现）。 */
export function isHttpUrl(text: string): boolean {
  return /^https?:\/\//i.test((text || "").trim());
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



/**
 * 派发一条失败提示（规则 #11 收口；规则 15.3「静默失败比报错难查一个量级」）。
 *
 * `lib/api/*` 不是组件、拿不到 `useToast()`，项目既有做法是直接派发
 * `app-toast` 窗口事件（主窗 App.tsx 监听）。这里把那一行收成函数，
 * 让新增的失败提示文案格式一致、以后要加节流也只改一处。
 *
 * **只用于用户主动触发的操作失败**。后台读取（获取标签 / 统计 / 缩略图等）
 * 失败不要用它——用户没要求过的事情失败了还弹提示，只是噪音。
 *
 * 注：既有 45 处内联的 `app-toast` 派发未一并迁移（行为完全相同，
 * 属于独立的整理工作，不在本次改动范围）。
 *
 * @param action 用户能看懂的动作描述，如「添加标签」；不要用内部函数名
 * @param err    原始错误，仅取 message 追加在后面，便于用户复述给维护者
 */
export function toastActionFailed(action: string, err?: unknown): void {
  const detail = err instanceof Error ? err.message : err != null ? String(err) : "";
  const message = detail ? `${action}失败：${detail}` : `${action}失败`;
  window.dispatchEvent(new CustomEvent("app-toast", { detail: { message, type: "error" } }));
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

const HOTKEY_ALIASES: Record<string, string> = { control: "ctrl", option: "alt", command: "meta", cmd: "meta", super: "meta", win: "meta" };
/** Canonical modifier aliases for conflict detection. */
export function normalizeHotkeyCombo(combo: string): string {
  const parts = combo.toLowerCase().split("+").map((p) => HOTKEY_ALIASES[p.trim()] ?? p.trim()).filter(Boolean);
  const order = ["ctrl", "alt", "shift", "meta"];
  return [...order.filter((m) => parts.includes(m)), ...parts.filter((p) => !order.includes(p))].join("+");
}
export function formatHotkey(combo: string, platform = typeof navigator === "undefined" ? "" : navigator.platform): string {
  if (!combo?.trim()) return "未设置";
  const mac = /mac/i.test(platform);
  const labels: Record<string, string> = {
    ctrl: mac ? "Control" : "Ctrl", alt: mac ? "Option" : "Alt", shift: "Shift", meta: mac ? "Command" : "Win",
    printscreen: "PrtSc", scrolllock: "ScrollLock", pause: "Pause",
  };
  return combo.split("+").map((p) => p.trim()).filter(Boolean).map((p) => {
    const key = HOTKEY_ALIASES[p.toLowerCase()] ?? p.toLowerCase();
    return labels[key] ?? (/^f\d{1,2}$/i.test(key) || key.length === 1 ? key.toUpperCase() : key.charAt(0).toUpperCase() + key.slice(1));
  }).join(" + ");
}
/** Modified shortcuts use physical letter/digit codes, including Option-generated glyphs. */
export function hotkeyMainKey(event: { key: string; code: string; ctrlKey: boolean; altKey: boolean; metaKey: boolean }, platform = typeof navigator === "undefined" ? "" : navigator.platform): string {
  if (/mac/i.test(platform) && (event.ctrlKey || event.altKey || event.metaKey)) {
    if (/^Key[A-Z]$/.test(event.code)) return event.code.slice(3).toLowerCase();
    if (/^Digit[0-9]$/.test(event.code)) return event.code.slice(5);
  }
  return event.key;
}

/** Search handlers accept Command on Mac and Control elsewhere. */
export function primaryShortcutLabel(key: string, platform = typeof navigator === "undefined" ? "" : navigator.platform): string {
  return formatHotkey(`${/mac/i.test(platform) ? "meta" : "ctrl"}+${key}`, platform).replace(/ \+ /g, "+");
}

export function primarySearchShortcut(platform = typeof navigator === "undefined" ? "" : navigator.platform): string {
  return primaryShortcutLabel("f", platform);
}

/** Window commands accept both Control and Command; global bindings remain physical. */
export function primaryModifierHeld(event: { ctrlKey: boolean; metaKey?: boolean }): boolean {
  return event.ctrlKey || !!event.metaKey;
}

/** Undefined may use a default; a configured empty binding is disabled. */
export function activeConfiguredHotkey(value: string | undefined, fallback?: string): string | undefined {
  return (value ?? fallback)?.trim() || undefined;
}

export function configuredShortcutLabel(value: string | undefined, fallback: string, platform = typeof navigator === "undefined" ? "" : navigator.platform): string {
  const combo = activeConfiguredHotkey(value, fallback);
  return combo ? formatHotkey(combo, platform).replace(/ \+ /g, "+") : "已禁用";
}

/** A configured empty string disables the shortcut; never substitute a default badge. */
export function toolShortcutLabel(
  key: string,
  fallback: string | undefined,
  config: { sequential_hotkey?: string; rec_hotkey?: string },
  platform = typeof navigator === "undefined" ? "" : navigator.platform,
): string | undefined {
  const raw = key === "sequential" ? config.sequential_hotkey
    : key === "screenrec" ? config.rec_hotkey : fallback;
  const selected = activeConfiguredHotkey(raw, fallback);
  if (!selected) return undefined;
  if (key === "diffedit") return primaryShortcutLabel("shift+d", platform);
  return formatHotkey(selected, platform).replace(/ \+ /g, "+");
}

/** Claude Desktop 的探测和写入使用同一平台路径。 */
export function claudeDesktopPaths(platform = typeof navigator === "undefined" ? "" : navigator.platform) {
  const detectPath = /mac/i.test(platform)
    ? "~/Library/Application Support/Claude"
    : "~/AppData/Roaming/Claude";
  return { detectPath, configPath: `${detectPath}/claude_desktop_config.json` };
}

/** Mac capture pixels use primary density; a spanning WebView can have a different backing DPR. */
export function capturePixelRatio(
  physicalWidth: number | undefined,
  cssWidth = typeof window === "undefined" ? 0 : window.innerWidth,
  backingRatio = typeof window === "undefined" ? 1 : window.devicePixelRatio || 1,
  platform = typeof navigator === "undefined" ? "" : navigator.platform,
): number {
  const ratio = Number(physicalWidth) / cssWidth;
  return /Mac/i.test(platform) && Number.isFinite(ratio) && ratio > 0 && cssWidth > 0
    ? ratio : backingRatio;
}

/** WebKit may omit JSON error positions. Preserve the error without inventing a line. */
export function validateJson(text: string): { valid: boolean; line?: number; position?: number; message?: string; value?: unknown } {
  try {
    return { valid: true, value: JSON.parse(text) };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    const positionMatch = message.match(/position (\d+)/);
    const lineMatch = message.match(/\(line (\d+)/);
    const position = positionMatch ? Math.min(text.length, Number(positionMatch[1])) : undefined;
    const line = lineMatch ? Number(lineMatch[1]) : position !== undefined ? text.slice(0, position).split("\n").length : undefined;
    return { valid: false, position, line, message };
  }
}

export function jsonValidationLabel(text: string, validation: ReturnType<typeof validateJson>): string {
  if (!text.trim()) return "等待输入";
  if (validation.valid) return "✓ 有效";
  return validation.line ? `✕ 第 ${validation.line} 行错误` : "✕ JSON 格式错误";
}

/** Native collection returns verified portable refs; never interpolate arbitrary URI Markdown. */
export function knowledgeCollectedContent(text: string, images: string[], existing = "") {
  const references = [...new Set(images)].filter(src => /^pp-asset:[a-f0-9]{32}\.(?:png|jpe?g|gif|webp|bmp|ico)$/i.test(src));
  const added = references.filter(src => !existing.includes(`](${src})`)).map(src => `![收集的图片](${src})`);
  return [existing, text, ...added].filter(part => part.trim()).join("\n\n");
}

/** Asset errors are a code contract. Never display raw transport errors or peer filesystem paths. */
export function knowledgeAssetErrorText(error: unknown) {
  let candidate: unknown = error;
  if (typeof error === "string") { try { candidate = JSON.parse(error); } catch { /* Unknown errors stay generic. */ } }
  const code = candidate && typeof candidate === "object" && "code" in candidate ? String(candidate.code) : "";
  const messages: Record<string, string> = {
    sync_disabled: "知识库同步已关闭，请在同步面板开启后重试。",
    unauthorized: "这台电脑尚未授权知识库同步，请先在同步面板连接。",
    paused: "这台电脑的知识库同步已暂停，请恢复后重试。",
    cancelled: "已停止取得图片，正文与已有图片仍保留。",
    busy: "已有图片正在取得，请稍后重试。",
    unsupported: "这台电脑暂不支持单张补图。请升级电脑，或在同步面板运行一次正常同步。",
    offline: "暂时无法连接电脑，请确认电脑在线后重试。正文仍可阅读。",
    missing: "电脑上也没有这张图片，请在电脑核对原图。",
    invalid: "图片引用已变化或不受支持，请载入笔记新版后重试。",
    too_large: "这张图片超过单张10MB限制，请在电脑压缩后重试。",
    integrity: "图片校验未通过，未保存不完整内容。可以重试取得。",
    io: "图片未能保存，请检查手机存储空间后重试。",
  };
  return messages[code] || "图片未能取得，请重试。正文与已有内容仍保留。";
}

/** Recognize a shared link without turning an ordinary long note into an article import. */
export function knowledgeArticleUrl(text: string): string | null {
  if (text.length > 4000 || text.trim().split("\n").length > 4) return null;
  const matches = text.match(/https?:\/\/[^\s<>]+/gi) || [];
  if (matches.length !== 1) return null;
  try {
    const raw = text.trim() === matches[0] ? matches[0] : matches[0].replace(/[。，；！？)）]+$/, "");
    const url = new URL(raw);
    if (url.username || url.password || url.href.length > 2048) return null;
    return url.href;
  } catch { return null; }
}

/** Only map the server-authorized pending image; unrelated article links stay untouched. */
export function knowledgeArticleImageIndex(images: { url: string; local: string | null }[], src: string): number {
  return images.findIndex(image => image.url === src || image.local === src);
}

/** Article failures use the same visible feedback without exposing transport/SQL diagnostics. */
export function knowledgeArticleErrorText(error: unknown): string {
  const raw = error instanceof Error ? error.message : String(error ?? "");
  if (/这篇文章已收藏/.test(raw)) return "这篇文章已经收藏过，请查看已有笔记。本次输入仍保留。";
  if (/原笔记已被修改/.test(raw)) return "原笔记已被修改，未覆盖正文或备注。请查看已有笔记。";
  if (/原笔记已删除|原笔记已不可用/.test(raw)) return "原笔记已不在本机，请返回知识库核对后再收藏。";
  if (/收集.*已更新|图片已经变化|文章收集已更新/.test(raw)) return "内容已更新，请重新读取后重试。当前内容仍保留。";
  if (/已有20条|分享过多/.test(raw)) return "待收集文章较多，请先保存已有内容后再收藏。";
  if (/已有文章|已有图片/.test(raw)) return "正在处理其他收藏，请稍后重试。";
  if (/抓取失败|页面返回 HTTP|读取页面失败|重定向|可读正文|正文.*不可用/.test(raw)) return "暂时无法取得可读正文。请重试、查看原文，或仅存链接。";
  if (/内网|本地.*地址/.test(raw)) return "此处只支持公开文章链接，不支持本机或内网地址。";
  if (/图片.*限制|128张|分辨率|正文.*过长|文章.*过长/.test(raw)) return "内容或配图超出手机处理限制。链接和已有内容仍保留，可先仅存链接。";
  if (/图片.*(?:读取|不可用|中断|变化)/.test(raw)) return "这张图片暂时无法读取，可以重试或查看原文。正文仍保留。";
  return knowledgeErrorText(error);
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
/** Shared recording clock; backend elapsed time already excludes paused intervals. */
export function formatRecordingTime(ms: number): string {
  const sec = Math.floor(Math.max(0, Number.isFinite(ms) ? ms : 0) / 1000);
  const parts = [Math.floor(sec / 60) % 60, sec % 60];
  if (sec >= 3600) parts.unshift(Math.floor(sec / 3600));
  return parts.map(part => String(part).padStart(2, "0")).join(":");
}

/** Show a saved artifact's parent without assuming a platform separator. */
export function recordingParentDir(path: string): string {
  const index = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  if (index < 0) return "";
  if (index === 0) return path.slice(0, 1);
  if (index === 2 && path[1] === ":") return path.slice(0, 3);
  return path.slice(0, index);
}

/** Editing positions preserve milliseconds, including sub-second recordings. */
export function formatRecordingPosition(ms: number): string {
  const value = Math.floor(Math.max(0, Number.isFinite(ms) ? ms : 0));
  return `${formatRecordingTime(value)}.${String(value % 1000).padStart(3, "0")}`;
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

/**
 * 秒数格式化为中文时长（精确到秒），用于运行时长 / 已运行列展示。
 *
 * 超过一天后进一位到“天”：服务跑上几天是常态，“86小时40分12秒”读起来很费劲。
 * 只给最大两级单位（如“3天14小时”而不是“3天14小时22分13秒”）：精度再高对“跑了多久”
 * 这个问题没有意义，而卡片右上角宽度有限。不足一分钟时仍给到秒，刚启动时能看到跳。
 */

export function formatUptime(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const d = Math.floor(total / 86400);
  const h = Math.floor((total % 86400) / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  if (d > 0) return `${d}天${h}小时`;
  if (h > 0) return `${h}小时${m}分`;
  if (m > 0) return `${m}分${s}秒`;
  return `${s}秒`;
}

/** 版本号统一格式化为 "vX.Y.Z"（已带 v 则不重复加）。各处版本展示共用，避免前缀漂移。 */

export function formatVersion(v?: string): string {
  if (!v) return "?";
  return v.startsWith("v") ? v : `v${v}`;
}

/**
 * 毫秒耗时格式化为一眼可读的中文文本，自动换算单位（微秒/毫秒/秒/分），避免用户看到
 * 大数字（如 10000ms）还要心算，也不用英文缩写（ms/s）而用中文单位。
 * - <1ms：换算成微秒（审计写盘等微秒级耗时仍能看清，不四舍五入成 0）。
 * - <10ms：毫秒保留 1 位小数（区分 1.2毫秒与 8.7毫秒）。
 * - <1000ms：毫秒取整。
 * - <60s：换算成秒（保留 1 位小数，如 10000ms → "10.0秒"）。
 * - ≥60s：换算成 "X分Y秒"。
 */

export function formatDurationMs(ms: number): string {
  if (ms <= 0) return "0毫秒";
  if (ms < 1) return `${Math.round(ms * 1000)}微秒`;
  if (ms < 10) return `${ms.toFixed(1)}毫秒`;
  if (ms < 1000) return `${Math.round(ms)}毫秒`;
  const totalSeconds = ms / 1000;
  if (totalSeconds < 60) return `${totalSeconds.toFixed(1)}秒`;
  const m = Math.floor(totalSeconds / 60);
  const s = Math.round(totalSeconds % 60);
  return `${m}分${s}秒`;
}

/* ─── Token 用量展示（纯函数，共用，规则 11）─── */

/**
 * token 数格式化：≥1 万显示「x.x万」（保留 1 位小数，去尾随 .0），否则千分位原值。
 * 口径是估算值（见后端 audit.rs），展示层不做任何「账单」暗示。
 */

export function fmtTokens(n: number): string {
  if (n < 10000) return n.toLocaleString("en-US");
  const w = n / 10000;
  return `${w >= 100 ? Math.round(w) : Math.round(w * 10) / 10}万`;
}

/** 日期键 "2026-09-27" → 短标签 "9/27"。非法输入原样返回。 */

export function shortDateKey(date: string): string {
  const m = /^\d{4}-(\d{2})-(\d{2})$/.exec(date);
  if (!m) return date;
  return `${Number(m[1])}/${Number(m[2])}`;
}

/* ─── 更新历史「已读」状态（localStorage，纯函数，规则 11）─── */

const CHANGELOG_LAST_SEEN_KEY = "shell_changelog_last_seen";

/** 读取用户上次看到的版本（未看过返回 null）。 */

export function getLastSeenVersion(): string | null {
  try {
    return localStorage.getItem(CHANGELOG_LAST_SEEN_KEY);
  } catch {
    return null;
  }
}

/** 记录用户已看到某版本（通常是当前最新版）。 */

export function setLastSeenVersion(version: string): void {
  try {
    localStorage.setItem(CHANGELOG_LAST_SEEN_KEY, version);
  } catch {
    /* localStorage 不可用时静默忽略，仅影响红点提示 */
  }
}

/** 语义版本比较：a>b 返回正数，a<b 返回负数，相等 0。 */

export function compareVersion(a: string, b: string): number {
  // 按段取前导整数（parseInt 容忍 "1-rc1" 之类后缀，取到 1）；真正非数字段安全退化为 0。
  // 旧实现用 Number("1-rc1") 得 NaN、再被 || 0 吞成 0，会静默丢掉该段的真实数字。
  const seg = (s: string) => {
    const n = parseInt(s, 10);
    return Number.isNaN(n) ? 0 : n;
  };
  const pa = a.split(".").map(seg);
  const pb = b.split(".").map(seg);
  for (let i = 0; i < 3; i++) {
    const diff = (pa[i] || 0) - (pb[i] || 0);
    if (diff) return diff;
  }
  return 0;
}

/** 统计比 lastSeen 更新的版本数量（用于「更新」Tab 未读红点）。 */

export function countUnreadVersions(versions: string[], lastSeen: string | null): number {
  if (!lastSeen) return versions.length; // 从未看过 → 全部未读
  return versions.filter((v) => compareVersion(v, lastSeen) > 0).length;
}

/**
 * 绝对时间（备份 createdAt 形如 "YYYY-MM-DD HH:MM:SS"）转中文相对时间：
 * 刚刚 / X 分钟前 / X 小时前 / X 天前 / X 个月前 / X 年前。
 * 解析失败（如"未知时间"）原样返回，避免崩溃。
 */

export function formatRelativeTime(dateStr: string): string {
  if (!dateStr) return "";
  const s = dateStr.includes("T") ? dateStr : dateStr.replace(" ", "T");
  const d = new Date(s);
  if (isNaN(d.getTime())) return dateStr;
  const diffSec = Math.floor((Date.now() - d.getTime()) / 1000);
  if (diffSec < 0) return dateStr; // 未来时间（时钟异常）回退原值
  if (diffSec < 60) return "刚刚";
  const min = Math.floor(diffSec / 60);
  if (min < 60) return `${min} 分钟前`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr} 小时前`;
  const day = Math.floor(hr / 24);
  if (day < 30) return `${day} 天前`;
  const mon = Math.floor(day / 30);
  if (mon < 12) return `${mon} 个月前`;
  return `${Math.floor(mon / 12)} 年前`;
}

/**
 * 相对时间，但入参是 **unix 秒**。
 *
 * 历史会话表里存的是秒（`file_mtime / 1000`），转成 ISO 字符串再交给
 * `formatRelativeTime` 去解析纯属绕路 —— 但比在库里存一份字符串好：
 * 时间戳能被排序、能被比较，字符串不能。
 */

export function formatRelativeSecs(sec: number): string {
  if (!Number.isFinite(sec) || sec <= 0) return "";
  return formatRelativeTime(new Date(sec * 1000).toISOString());
}

/** 字节数格式化为带单位的人类可读文本（B/KB/MB/GB），SecurityTab 与版本历史弹框共用（规则 11）。 */

export function formatBytesPerSec(bytesPerSec: number): string {
  return `${formatBytes(bytesPerSec)}/s`;
}

/**
 * 清理 SFTP 报错里的 PTY 噪声：scp/ssh 在 PTY 下会输出 `\r` 进度条、ANSI 转义、
 * 多余空行。剥掉后只留人类可读的错误（权限拒绝 / 路径不存在 / 连接失败等）。
 */

export function cleanErr(raw: unknown): string {
  const s = String(raw ?? "");
  return s
    // eslint-disable-next-line no-control-regex -- 本函数的职责就是剥掉 PTY 的控制字符
    .replace(/\[[0-9;]*m/g, "") // ANSI 颜色
    // eslint-disable-next-line no-control-regex -- 同上：退格是 scp 进度条的刷新方式
    .replace(/[\r]/g, "") // 回车 / 退格（进度条覆盖）
    .split("\n")
    .map((l) => l.trimEnd())
    .filter((l) => l.trim().length > 0)
    .slice(-6) // 只取末尾关键几行
    .join("\n")
    .trim();
}

/* ─── 白名单排除规则 glob（与 Rust 侧 security/path.rs 同语义，编辑器实时预览用）─── */

/**
 * fnmatch 式 glob：`**` 跨层（零或多段）、`*` 单段内任意字符、`?` 单段内单字符。
 * 与 Rust 实现的对齐约定：
 * - 模式含 `/` → 对相对路径整体匹配；不含 `/` → 对任意一段的段名匹配；
 * - 本应用是 Windows 桌面端，匹配统一不区分大小写；
 * - `\` 与 `/` 都视作路径分隔符。
 * 🔴 预览仅供人工参考，**强制执行在后端 path.rs**——两处语义如需调整必须同步改。
 */

export function globMatch(pattern: string, path: string): boolean {
  const norm = (s: string) => s.replace(/\\/g, "/").toLowerCase();
  const p = norm(pattern);
  const t = norm(path);
  const segMatch = (pat: string[], txt: string[]): boolean => {
    const [ph, ...prest] = pat;
    if (ph === undefined) return txt.length === 0;
    if (ph === "**") {
      for (let n = 0; n <= txt.length; n++) {
        if (segMatch(prest, txt.slice(n))) return true;
      }
      return false;
    }
    const [th, ...trest] = txt;
    if (th === undefined) return false;
    return segFnmatch(ph, th) && segMatch(prest, trest);
  };
  if (p.includes("/")) return segMatch(p.split("/"), t.split("/"));
  return t.split("/").some((seg) => segMatch([p], [seg]));
}

/** 单段内 fnmatch：`*` 任意串、`?` 单字符、其余字面量相等（递归回溯，段很短）。 */
function segFnmatch(pattern: string, text: string): boolean {
  if (pattern.length === 0) return text.length === 0;
  const c = pattern[0];
  const rest = pattern.slice(1);
  if (c === "*") {
    for (let n = 0; n <= text.length; n++) {
      if (segFnmatch(rest, text.slice(n))) return true;
    }
    return false;
  }
  if (text.length === 0) return false;
  if (c === "?") return segFnmatch(rest, text.slice(1));
  return text[0] === c && segFnmatch(rest, text.slice(1));
}

/**
 * 判断键盘事件目标是否是「可编辑元素」（输入框 / 多行文本 / 下拉框 / contentEditable）。
 *
 * 全局快捷键（Ctrl+K 命令面板、Ctrl+1~N 切页、Ctrl+, 开设置、CC 页 Ctrl+N 新对话）在焦点
 * 落在输入区时必须让路，否则用户输密码、敲命令参数会被快捷键劫持。原先 App.tsx 的键盘监听
 * 里写了一份内联闭包，CC 页快捷键同样需要 ⇒ 按规则 11 抽成公共纯函数。
 *
 * 用「鸭子类型」判据而非 `instanceof HTMLElement`：真实 KeyboardEvent.target 一定是真实
 * Node，两者判据等价（document/window 无 tagName、SVG 的 tagName 小写且无 isContentEditable），
 * 但本函数因此可在 node 环境的单元测试里直接用普通对象验证，不必为一个小函数起 jsdom 用例文件。
 */

export function isEditableTarget(el: EventTarget | null): boolean {
  if (!el || typeof el !== "object") return false;
  const node = el as { tagName?: unknown; isContentEditable?: unknown };
  if (typeof node.tagName !== "string") return false;
  return (
    node.tagName === "INPUT" ||
    node.tagName === "TEXTAREA" ||
    node.tagName === "SELECT" ||
    node.isContentEditable === true
  );
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

/**
 * 浮层「点外面关闭」的公共判定：面板自身里的按下不算外点；**本面板的开关按钮**
 * （`data-cc-toggle="<owner>"`）也不算 —— 不豁免它的话，capture 的 pointerdown
 * 先把面板收掉，随后的 click 再把面板翻开，这个按钮就永远关不掉自己的浮层。
 *
 * `owner` 之外的开关按钮**仍然算外点**（点了别家开关 ⇒ 先收自己，click 去开别家），
 * 与没有这个 bug 之前的行为一致。
 */

export function isOutsideDismiss(
  e: PointerEvent,
  box: HTMLElement | null,
  owner: string,
): boolean {
  if (box?.contains(e.target as Node)) return false;
  const target = e.target;
  return !(target instanceof Element && target.closest(`[data-cc-toggle="${owner}"]`));
}
