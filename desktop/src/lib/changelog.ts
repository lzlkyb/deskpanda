// ─── Types ─────────────────────────────────────────────

export type ChangeCategoryType = "feat" | "fix" | "change" | "security" | "perf" | "tech" | "stability" | "uiux" | "other";

export interface ChangeItem {
  text: string;
  /** 有什么用：一句话价值说明（仅「新增」类需要，用于发版弹框功能卡片） */
  why?: string;
  /** 怎么用：操作步骤（1–3 步），用于发版弹框功能卡片 */
  how?: string[];
  /** 配图：插图 key（ocr / mosaic / eraser，由 Canvas 实时绘制，零图片文件）；也可写真实图片路径（如 shots/ocr.jpg），此时走 <img> 且失败降级 */
  media?: string;
}

export interface ChangeGroup {
  label: string;
  items: ChangeItem[];
}

export interface ChangeCategory {
  type: ChangeCategoryType;
  name: string;        // Display name (e.g. "新增", "修复")
  items?: ChangeItem[];
  groups?: ChangeGroup[];
}

export interface ChangelogEntry {
  version: string;
  date: string;
  summary: string;
  categories: ChangeCategory[];
}

// ─── Utilities ─────────────────────────────────────────

/** Count total items in a category */
export function countCategoryItems(cat: ChangeCategory): number {
  if (cat.items) return cat.items.length;
  if (cat.groups) return cat.groups.reduce((sum, g) => sum + g.items.length, 0);
  return 0;
}
