import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { rcNetDiag } from "@/lib/api/rcNetDiag";
import { RcNetDiagCard } from "./RcNetDiagCard";

// 只替掉取数，判定/文案仍用真实实现（punchSummary / mappingLabel 的口径不该被测试重写）
vi.mock("@/lib/api/rcNetDiag", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/api/rcNetDiag")>()),
  rcNetDiag: vi.fn(),
}));

afterEach(() => cleanup());

it("a failed probe is reported in the card instead of silently keeping the old state", async () => {
  vi.mocked(rcNetDiag).mockRejectedValue(new Error("探测服务未就绪"));
  render(<RcNetDiagCard />);
  fireEvent.click(screen.getByRole("button", { name: /网络诊断/ }));
  const alert = await screen.findByRole("alert");
  expect(alert.textContent).toContain("网络诊断读取失败");
  expect(alert.textContent).toContain("探测服务未就绪");
  // 失败时不许再冒充「报告还没生成」（原来的静态占位文案）
  expect(screen.queryByText(/报告还没生成/)).toBeNull();
});

/** P0 C1：完整快照夹具——「当前中继」行的出/不出与文案。 */
const snapshot = (
  over: Partial<import("@/lib/api/rcNetDiag").NetDiagSnapshot> = {},
): import("@/lib/api/rcNetDiag").NetDiagSnapshot => ({
  udp_v4: true,
  udp_v6: false,
  mapping_varies_by_dest_ipv4: false,
  mapping_varies_by_dest_ipv6: null,
  global_v4: "1.2.3.4:5678",
  global_v6: null,
  captive_portal: false,
  preferred_relay: null,
  relay_latencies_ms: [],
  verdict: "direct_likely",
  family_v4: { udp: true, mapping_varies: false, observed: "1.2.3.4:5678" },
  family_v6: { udp: false, mapping_varies: null, observed: null },
  home_relay: null,
  home_relay_connected: false,
  ...over,
});

async function openCard() {
  render(<RcNetDiagCard />);
  fireEvent.click(screen.getByRole("button", { name: /网络诊断/ }));
}

it("有 home 中继时显示「当前中继」一行：域名 · 已连接", async () => {
  vi.mocked(rcNetDiag).mockResolvedValue({
    snapshot: snapshot({
      home_relay: "https://relay.example.com/",
      home_relay_connected: true,
    }),
    unavailable_reason: "",
  });
  await openCard();
  const row = await screen.findByText("当前中继");
  expect(row.nextElementSibling?.textContent).toBe("relay.example.com · 已连接");
});

it("未连接的中继照实显示「未连接」——不给用户假的安心", async () => {
  vi.mocked(rcNetDiag).mockResolvedValue({
    snapshot: snapshot({
      home_relay: "https://relay.example.com/",
      home_relay_connected: false,
    }),
    unavailable_reason: "",
  });
  await openCard();
  expect(await screen.findByText("relay.example.com · 未连接")).toBeTruthy();
});

it("没有 home 中继时整行不出（不摆「未知」占位）", async () => {
  vi.mocked(rcNetDiag).mockResolvedValue({
    snapshot: snapshot(),
    unavailable_reason: "",
  });
  await openCard();
  await screen.findByText("直连判定");
  expect(screen.queryByText("当前中继")).toBeNull();
});
