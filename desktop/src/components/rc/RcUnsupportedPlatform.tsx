/**
 * RcUnsupportedPlatform — 非 Windows 平台的降级通知卡（S6：远程功能只交付 Windows）。
 *
 * 由 RcWorkbench 在 `rcPlatformDegraded(platform)` 为真时挂到主区；侧栏与错误槽
 * 在同一条件下退场，本卡就是降级态的全部内容——「为什么不能用 / 去哪用 / 本机
 * 还能用什么」三句话说清，不让用户对着一堆「命令不存在」报错盲猜。
 *
 * 样式类名（.unsupported*）在 RemoteComputerA2.module.css，取值全部来自
 * theme.css 的 warning 三件套与卡片令牌。
 */
import { MonitorX } from "lucide-react";
import styles from "./RemoteComputerA2.module.css";

export function RcUnsupportedPlatform({ platform }: { platform?: string }) {
  /* 标题带平台名：唯一现实受众是 macOS（mac 构建保留、只出壳子，见 S6）；
     linux 等未发行平台走通用措辞，不谎称「不是 mac 却说是 mac」。 */
  const title = platform === "macos" ? "远程电脑暂不支持 macOS" : "远程电脑暂不支持当前系统";

  return (
    <div className={styles.unsupportedWrap}>
      <div className={styles.unsupported} role="status">
        <div className={styles.unsupportedHead}>
          <span className={styles.unsupportedIcon} aria-hidden="true">
            <MonitorX size={20} />
          </span>
          <span className={styles.unsupportedTitle}>{title}</span>
        </div>
        <div className={styles.unsupportedBody}>
          远程画面、声音与键鼠控制依赖 <b>Windows 平台</b>的屏幕采集与编码能力，当前版本只在 Windows 电脑上提供。请在 Windows 电脑上安装 DeskPanda，用另一台电脑连接使用。
        </div>
        <div className={styles.unsupportedNote}>
          本机仍可正常使用托盘常驻、检查更新与退出。当前平台：<code>{platform ?? "未知"}</code>
        </div>
      </div>
    </div>
  );
}
