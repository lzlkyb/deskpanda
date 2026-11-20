/**
 * usePlatform — 读一次本机平台值（`get_app_info` 的 `platform` 字段）。
 *
 * 模块级单飞：工作台、标题栏等多处同时挂载只发一次 invoke。promise 进程内缓存
 * （不重置）——平台是编译期常量，运行期不会变；失败了也就永远停在 undefined，
 * 由调用方按「非降级」处理（见 `lib/rcPlatform.ts` 文件头），不重试。
 *
 * 为什么读 `get_app_info` 而不是 `get_status`：本仓（DeskPanda）没有后者——
 * 那是 pastePanda 壳子设置页的来源；`get_app_info` 在 main.rs 两段
 * invoke_handler 均已注册，mac 上也在。
 */
import { useEffect, useState } from "react";
import { invoke } from "@/lib/tauri";

/** `get_app_info` 的回值（只用 platform；version 留给以后的「关于」类界面）。 */
interface AppInfo {
  platform?: string;
}

let inflight: Promise<string | undefined> | null = null;

function loadPlatform(): Promise<string | undefined> {
  inflight ??= invoke<AppInfo>("get_app_info")
    .then((info) => info.platform)
    .catch(() => undefined);
  return inflight;
}

export function usePlatform(): string | undefined {
  const [platform, setPlatform] = useState<string | undefined>(undefined);

  useEffect(() => {
    let alive = true;
    void loadPlatform().then((value) => {
      if (alive) setPlatform(value);
    });
    return () => {
      alive = false;
    };
  }, []);

  return platform;
}
