/**
 * RcWorkbench 骨架的守卫单测（2026-09-21，A 方案稿批次 2 + 批5）。
 *
 * 钉住两件事：
 * ① 会话态**不渲染**工作台标题栏（画面铺满整个工作台，不再白留一条 48px）；
 * ② 非会话态照旧渲染。批7 起它是**自绘标题栏**——品牌 + 通道状态位 + 窗口按钮，
 *    兼作窗口拖拽区（窗口已 `decorations(false)`），所以这里同时钉住那三个按钮在。
 *
 * 判据本身收口在 `hidesWorkbenchTitleBar`（lib/rcWorkbenchA2，另有一组纯函数用例），
 * 这里补的是**消费侧**——`{!chromeHidden && <RcA2TitleBar/>}` 若写成 `chromeHidden &&`，
 * 纯函数用例查不出来，只有渲染断言挡得住（同类「写完了但没接上」的缺口，
 * 见 2026-09-21 的 RcEmptyGuide selfEnabled 漏传）。
 *
 * 批5 补的是第三件：会话历史从这一个 hook 出去要**同时**到侧栏筛选与内容区列表，
 * 只接一半（侧栏有了筛选、列表还自己拉一份）在界面上看不出来，只有断言「两处的
 * 数字来自同一份快照」能挡住。
 *
 * 会话视图 / 配对层 / 四个 hook 全 mock：本用例只关心骨架接线。
 */
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { RcSession, RcStatus, RcTargetDevice } from "@/lib/api/rc";

const h = vi.hoisted(() => ({
  status: null as RcStatus | null,
  targets: [] as RcTargetDevice[],
  probeTargets: vi.fn().mockResolvedValue(undefined),
  refreshTargets: vi.fn(),
  visible: true,
  error: null as string | null,
  end: vi.fn().mockResolvedValue(true),
  stageMount: vi.fn(),
  stageUnmount: vi.fn(),
}));
/** S6 平台降级用例：控制 usePlatform 的回值（默认 undefined = 首帧未读到）。 */
const platformValue = vi.hoisted(() => ({ value: undefined as string | undefined }));
/** 批7 审查补：toast 要能被断言——「通道未启动时点检测」的修复靠的就是它。 */
const toastSpy = vi.hoisted(() => vi.fn());

vi.mock("@/components/Toast", () => ({ useToast: () => ({ toast: toastSpy }) }));
vi.mock("@/components/settings/RcPairLayer", () => ({ RcPairLayer: () => null }));
vi.mock("./RcStage", async () => {
  const { useEffect } = await import("react");
  return { RcStage: () => {
    useEffect(() => { h.stageMount(); return () => { h.stageUnmount(); }; }, []);
    return <div>会话画面（已 mock）</div>;
  } };
});
vi.mock("@/hooks/useRc", () => ({
  useRc: () => ({
    status: h.status,
    error: h.error,
    busy: false,
    isOpError: false,
    targets: h.targets,
    reachability: {},
    targetsLoaded: true,
    targetsError: null,
    refresh: vi.fn(),
    refreshIdentity: vi.fn(),
    refreshTargets: h.refreshTargets,
    probeTargets: h.probeTargets,
    startChannel: vi.fn().mockResolvedValue(true),
    end: h.end,
    setEnabled: vi.fn(),
    setDeviceAllowed: vi.fn(),
    setDeviceTrust: vi.fn(),
    setDeviceAutoAccept: vi.fn(),
    clearError: vi.fn(),
  }),
}));
vi.mock("@/hooks/useWindowVisible", () => ({ useWindowVisible: () => h.visible }));
vi.mock("@/hooks/usePlatform", () => ({ usePlatform: () => platformValue.value }));
vi.mock("@/hooks/useRcLaunch", () => ({
  useRcLaunch: () => ({
    cap: "control",
    setDefaultCap: vi.fn(),
    lastPeer: null,
    lastAttempt: null,
    doRequest: vi.fn().mockResolvedValue(true),
    forgetDevice: vi.fn(),
  }),
}));
vi.mock("@/hooks/useRcWorkbenchClose", () => ({ useRcWorkbenchClose: () => {} }));

/** 会话历史 mock：两条都属同一台设备，用来验证侧栏计数与列表出自同一份数据。 */
const hist = vi.hoisted(() => ({
  list: [
    {
      peer: "peer-a",
      peer_name: "工作电脑",
      capability: "control",
      dir: "outbound",
      started_ms: 5000,
      ended_ms: 6000,
      duration_ms: 1000,
      reason: "正常结束",
    },
    {
      peer: "peer-a",
      peer_name: "工作电脑",
      capability: "view",
      dir: "outbound",
      started_ms: 4000,
      ended_ms: 4500,
      duration_ms: 500,
      reason: "用户结束会话",
    },
  ],
  reload: vi.fn(),
}));
vi.mock("@/hooks/useRcHistory", () => ({
  useRcHistory: () => ({ list: hist.list, loading: false, err: null, reload: hist.reload }),
}));

import { RcWorkbench } from "./RcWorkbench";

describe("等待/被控期间的工具导航", () => {
  beforeEach(() => {
    h.targets = [];
    h.end.mockReset().mockResolvedValue(true);
    h.stageMount.mockClear(); h.stageUnmount.mockClear(); toastSpy.mockClear();
  });
  it.each(["outbound_pending", "inbound_active"] as const)("%s 期间可打开全部工具页且始终能停止会话", async (phase) => {
    h.status = status(phase, true);
    render(<RcWorkbench />);
    const stopLabel = phase === "outbound_pending" ? "取消申请" : "结束会话";
    for (const page of ["文件", "记录", "设置"]) {
      fireEvent.click(screen.getByRole("button", { name: page }));
      expect(screen.getByRole("region", { name: "当前远程会话" })).toBeTruthy();
      expect(screen.getByRole("button", { name: stopLabel })).toBeTruthy();
      expect(screen.getByText("会话画面（已 mock）").closest("[hidden]")).toBeTruthy();
    }
    expect(screen.getByRole("group", { name: "默认发起方式" })).toBeTruthy();
    expect(h.end).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: stopLabel }));
    await waitFor(() => expect(h.end).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(toastSpy).toHaveBeenCalledWith(phase === "outbound_pending" ? "已取消远程申请" : "已结束远程会话", "success"));
  });
  it("等待中切工具页和返回、收到同意后保持同一会话组件", () => {
    h.status = status("outbound_pending", true);
    const view = render(<RcWorkbench />);
    fireEvent.click(screen.getByRole("button", { name: "记录" }));
    fireEvent.click(screen.getByRole("button", { name: "返回会话" }));
    expect(screen.getByText("会话画面（已 mock）").closest("[hidden]")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "记录" }));
    h.status = status("outbound_active", true);
    view.rerender(<RcWorkbench />);
    expect(screen.queryByRole("region", { name: "当前远程会话" })).toBeNull();
    expect(screen.queryByRole("region", { name: "会话记录" })).toBeNull();
    expect(screen.getByText("会话画面（已 mock）").closest("[hidden]")).toBeNull();
    expect(h.stageMount).toHaveBeenCalledTimes(1);
    expect(h.stageUnmount).not.toHaveBeenCalled();
  });
});

/** 后端只回传会话位；这里只摆本用例会用到的字段。 */
function status(phase: RcSession["phase"] | null, running = false): RcStatus {
  return {
    enabled: false,
    capability: "control",
    session: phase
      ? {
          id: "s-1",
          peer: "peer-a",
          peer_name: "工作电脑",
          capability: "control",
          phase,
          started_ms: 1_700_000_000_000,
          granted: true,
        }
      : null,
    pending: [],
    joins: [],
    device_deny: {},
    running,
    quality: "auto",
    capture_scope: "virtual",
  } as RcStatus;
}

describe("RcWorkbench 骨架：会话态收标题栏（A 方案稿 批2 / 批7 自绘）", () => {
  it("出站会话态不渲染标题栏，画面接管整个工作台", () => {
    h.status = status("outbound_active");
    const { container } = render(<RcWorkbench />);

    expect(screen.queryByRole("banner")).toBeNull();
    // 标题栏里那格通道状态位也不该在（它是标题栏的一部分）
    expect(screen.queryByText(/远程通道已开启|通道未启动/)).toBeNull();
    // 会话画面本身要在
    expect(screen.getByText("会话画面（已 mock）")).toBeTruthy();
    // 也不该反过来把侧栏留下（会话态是置顶的全宽面）
    expect(container.querySelector("[data-rc-root]")).toBeTruthy();
  });

  it("空闲态渲染标题栏：品牌 + 通道状态位 + 三个自绘窗口按钮", () => {
    h.status = status(null);
    render(<RcWorkbench />);

    expect(screen.getByRole("banner")).toBeTruthy();
    // 🔴 必须限定在 banner 里：2026-09-29 常驻配对卡的空态文案也含
    // 「DeskPanda」（「另一台电脑安装并打开 DeskPanda」），全局 getByText
    // 会一次匹配到两个。品牌这件事本来就只属于标题栏那一格。
    expect(within(screen.getByRole("banner")).getByText("DeskPanda")).toBeTruthy();
    // mock 的 running=false → 状态位落在「可点的补救入口」那一支（不是只读文字）
    expect(screen.getByRole("button", { name: /通道未启动 · 点击开启/ })).toBeTruthy();
    // 批7：窗口改 decorations(false) 后系统按钮没了，这三个得自己画
    for (const name of ["最小化", "最大化", "关闭"]) {
      expect(screen.getByRole("button", { name })).toBeTruthy();
    }
  });

  it("等待对方同意的态也留标题栏", () => {
    h.status = status("outbound_pending");
    render(<RcWorkbench />);

    expect(screen.getByRole("banner")).toBeTruthy();
  });
});

describe("RcWorkbench 接线：会话历史一源两处（批5）", () => {
  it("从设备最近会话查看全部时，记录页筛选当前设备", () => {
    const original = hist.list;
    hist.list = [...original, { ...original[0], peer: "peer-b", peer_name: "家里的电脑", reason: "B 的记录" }];
    h.status = status(null, true);
    h.targets = [{ node_id: "peer-a", name: "工作电脑", source: "rc", conn_state: "ready", presence: "live", last_seen: 0, denied: false }];
    try {
      render(<RcWorkbench />);
      fireEvent.click(screen.getByRole("button", { name: "查看此设备全部记录" }));
      expect(screen.getByRole("button", { name: /工作电脑，2 条记录/ }).getAttribute("aria-pressed")).toBe("true");
      expect(screen.queryByText("B 的记录")).toBeNull();
    } finally {
      hist.list = original;
      h.targets = [];
    }
  });
  it("记录页：侧栏换成「按设备筛选」，计数与内容区列表同源", () => {
    h.status = status(null);
    render(<RcWorkbench />);

    fireEvent.click(screen.getByRole("button", { name: "记录" }));

    // 侧栏：设备列表让位给筛选列表，计数取自同一份 mock
    expect(screen.getByText("按设备筛选")).toBeTruthy();
    expect(screen.getByRole("button", { name: /全部设备，2 条会话记录/ })).toBeTruthy();
    expect(screen.getByRole("button", { name: /工作电脑，2 条记录/ })).toBeTruthy();
    expect(screen.queryByRole("searchbox", { name: "搜索设备" })).toBeNull();
    // 内容区：同一条记录的列表本体
    expect(screen.getByRole("region", { name: "会话记录" })).toBeTruthy();
    expect(screen.getByText("正常结束")).toBeTruthy();
  });

  it("记录页带「清空记录」入口——日志可见可删除的删除口在本页", () => {
    h.status = status(null);
    render(<RcWorkbench />);

    fireEvent.click(screen.getByRole("button", { name: "记录" }));
    expect(screen.getByRole("button", { name: /清空记录/ })).toBeTruthy();
  });

  it("筛选到某台设备后，只有它的记录留在列表里", () => {
    h.status = status(null);
    render(<RcWorkbench />);

    fireEvent.click(screen.getByRole("button", { name: "记录" }));
    fireEvent.click(screen.getByRole("button", { name: /工作电脑，2 条记录/ }));

    expect(screen.getByRole("button", { name: /工作电脑，2 条记录/ }).getAttribute("aria-pressed")).toBe(
      "true",
    );
    expect(screen.getByText("正常结束")).toBeTruthy();
  });
});

/**
 * 批7 行为审查补的钉子（2026-09-22）。
 *
 * 探测入口从标题栏下放到设备行之后，「通道未启动」这个前置条件**不再由「不渲染设备
 * 列表」代管**（已作死代码删除的旧址侧栏 RcWorkbenchSide 就是靠不渲染来躲的），必须
 * 由 `probeOne` 显式挡。
 * 实证过的失败形态：通道未启动时点「检测」→ `rc_probe_targets` 调用 **0** 次、
 * 无 toast、无 error，按钮文案原地不动 —— 静默死按钮。
 *
 * 两条成对写：① 挡得住；② **不许挡住正常路径**（守卫写错方向是很常见的回归）。
 */
const OFFLINE_TARGET: RcTargetDevice = {
  node_id: "peer-a",
  name: "工作电脑",
  conn_state: "ready",
  last_seen: Date.now() - 3_600_000,
  denied: false,
  source: "rc",
  presence: "never",
  last_path: "lan",
  trusted: false,
  auto_accept: false,
};

describe("RcWorkbench 接线：自动确认与通道前置", () => {
  beforeEach(() => {
    h.targets = [];
    toastSpy.mockReset();
    h.probeTargets.mockReset();
    h.probeTargets.mockResolvedValue(undefined);
    h.refreshTargets.mockReset().mockImplementation(async () => h.targets);
    h.visible = true;
  });

  it("通道未启动时状态明确可见，且不发探测", async () => {
    h.status = status(null); // running=false
    h.targets = [OFFLINE_TARGET];
    render(<RcWorkbench />);

    // 🔴 就绪判据 = 设备行真的渲染出来了（它要等 refreshTargets 落地），不是「我冲了一轮微任务」。
    // 紧随其后的「没发探测」是条负断言：数据没到就断言，等于在探测还没来得及发生时就宣判。
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /选择设备“工作电脑”/ }).textContent).toContain("状态无法获取 · 通道未启动"),
    );
    expect(h.probeTargets).not.toHaveBeenCalled();
  });

  it("通道已启动时自动探测，无需手动点设备行", async () => {
    h.status = status(null, true); // running=true
    h.targets = [OFFLINE_TARGET];
    render(<RcWorkbench />);

    await waitFor(() => expect(h.probeTargets).toHaveBeenCalledWith(["peer-a"]));

    expect(toastSpy).not.toHaveBeenCalledWith(expect.stringContaining("远程通道未启动"), "error");
  });
});

describe("RcWorkbench 自动确认设备", () => {
  beforeEach(() => {
    h.targets = [OFFLINE_TARGET];
    h.status = status(null, true);
    h.visible = true;
    h.probeTargets.mockReset().mockResolvedValue(undefined);
    h.refreshTargets.mockReset().mockImplementation(async () => h.targets);
  });

  it("打开可见工作台后自动确认设备，无需先点检测", async () => {
    render(<RcWorkbench />);
    await waitFor(() => expect(h.probeTargets).toHaveBeenCalledWith(["peer-a"]));
  });

  it("隐藏工作台不会自动探测设备", async () => {
    h.visible = false;
    render(<RcWorkbench />);
    // useRcAutoCheck 在 !enabled/!visible 时 effect 第一句就 return（src/hooks/useRcAutoCheck.ts:30），
    // 连 refreshTargets 都不发 ⇒ 这条没有任何异步数据可等，flush 只是让 mount 的 effect 跑完，
    // 负断言量的是「那一刻之后到没到探测分支」；同 describe 里可见那条已证「会探测」，两半合起来才成立。
    // waitguard:allow: 本用例零取数，微任务 flush 不是就绪信号
    await act(async () => { await Promise.resolve(); });
    expect(h.probeTargets).not.toHaveBeenCalled();
  });
});

/**
 * S6 平台降级接线（2026-11-20）：非 Windows 上 rc 命令在 Rust 侧整模块
 * `#[cfg(windows)] 门控 ⇒ invoke 一律失败。降级态的四件事在这层汇合：
 * 主区换通知卡 / 侧栏整体不渲染 / 错误条抑制 / 标题栏中格出静态徽章。
 *
 * 三条成对：① mac 降级；② **Windows 不许被误伤**（守卫写反方向是常见回归）；
 * ③ 首帧 undefined 不降级（Windows 首帧不许闪通知卡）。
 */
describe("RcWorkbench 接线：S6 平台降级（非 Windows）", () => {
  beforeEach(() => {
    h.targets = [];
    h.status = status(null, false);
    h.error = null;
    platformValue.value = undefined;
  });

  it("🔴 macOS（S6 交付后）：完整工作台照常渲染，无降级痕迹", () => {
    platformValue.value = "macos";
    render(<RcWorkbench />);

    // 与 Windows 同款：工具行在、通知卡不在
    expect(screen.getByRole("button", { name: "文件" })).toBeTruthy();
    expect(screen.queryByText(/暂不支持/)).toBeNull();
  });

  it("linux 等未交付平台：主区只剩通知卡；侧栏（设备列表 / 工具行）整体不渲染", () => {
    platformValue.value = "linux";
    render(<RcWorkbench />);

    expect(screen.getByText("远程电脑暂不支持当前系统")).toBeTruthy();
    // 侧栏退场的两个探针：设备页搜索框与底部工具行
    expect(screen.queryByRole("searchbox", { name: "搜索设备" })).toBeNull();
    for (const tool of ["文件", "记录", "设置"]) {
      expect(screen.queryByRole("button", { name: tool })).toBeNull();
    }
    // 标题栏仍在（窗口 decorations(false)，拖拽区与窗口按钮不能少）
    expect(screen.getByRole("banner")).toBeTruthy();
    for (const name of ["最小化", "最大化", "关闭"]) {
      expect(screen.getByRole("button", { name })).toBeTruthy();
    }
  });

  it("linux 等未交付平台：标题栏中格是静态徽章，不出「通道未启动 · 点击开启」死按钮", () => {
    platformValue.value = "linux";
    render(<RcWorkbench />);

    expect(screen.getByText("远程电脑不支持当前系统")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /通道未启动/ })).toBeNull();
  });

  it("🔴 Windows 零影响：降级分支不许误伤正常态（通知卡 / 徽章 / 侧栏都在原样）", () => {
    platformValue.value = "windows";
    render(<RcWorkbench />);

    expect(screen.queryByText("远程电脑暂不支持")).toBeNull();
    expect(screen.queryByText("远程电脑不支持当前系统")).toBeNull();
    // 正常态的中格照旧是可点补救入口（上面 linux 那条刚钉了它不许出现）
    expect(screen.getByRole("button", { name: /通道未启动 · 点击开启/ })).toBeTruthy();
    // 侧栏照旧渲染（工具横排是它的常驻底栏；搜索框默认折叠，不能当探针）
    expect(screen.getByRole("button", { name: "文件" })).toBeTruthy();
  });

  it("首帧 platform 未读到（undefined）：不降级，Windows 首帧不闪通知卡", () => {
    platformValue.value = undefined;
    render(<RcWorkbench />);

    expect(screen.queryByText("远程电脑暂不支持")).toBeNull();
    expect(screen.getByRole("button", { name: "文件" })).toBeTruthy();
  });

  it("降级态抑制错误条：未交付平台上「命令不存在」不是用户该看的话（Windows / macOS 仍照常显示）", () => {
    platformValue.value = "linux";
    h.error = "command not found: rc_get_status";
    render(<RcWorkbench />);
    // 通知卡照常；错误条不在（RcErrorPanel 对未识别错误把原文印在 hint 里）
    expect(screen.getByText("远程电脑暂不支持当前系统")).toBeTruthy();
    expect(screen.queryByText("command not found: rc_get_status")).toBeNull();

    platformValue.value = "windows";
    render(<RcWorkbench />);
    expect(screen.getByText("command not found: rc_get_status")).toBeTruthy();
  });
});
