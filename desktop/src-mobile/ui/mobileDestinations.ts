import { FolderOpen, Monitor, Settings2 } from "lucide-react";

// Stable ids preserve page state when a capability adds a new destination.
// Phase-two entries join this registry when their actual content is available.
// 2026-11-20 迁入 deskpanda 时删去 knowledge（知识库不在本仓范围）。
export const MOBILE_DESTINATIONS = [
  { id: "devices", label: "设备", Icon: Monitor },
  { id: "files", label: "文件", Icon: FolderOpen },
  { id: "settings", label: "设置", Icon: Settings2 },
] as const;

export type MobileDestination = (typeof MOBILE_DESTINATIONS)[number]["id"];
