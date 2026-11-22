/**
 * RcManualAddrRow — 三态与交互：设置→保存、清除、Enter/Esc、失败不退出编辑态。
 * 目标行只对 rc 配对设备挂载的约束由父层（RcA2DeviceDetail）持有，这里测行内为。
 */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { RcTargetDevice } from "@/lib/api/rc";
import { RcManualAddrRow } from "./RcManualAddrRow";

afterEach(() => cleanup());

function target(over: Partial<RcTargetDevice> = {}): RcTargetDevice {
  return {
    node_id: "a".repeat(64),
    name: "办公室台式机",
    conn_state: "offline",
    last_seen: 0,
    denied: false,
    source: "rc",
    presence: "never",
    last_path: "",
    note: "",
    display_name: "办公室台式机",
    trusted: false,
    auto_accept: false,
    ...over,
  } as RcTargetDevice;
}

describe("RcManualAddrRow 直连地址行", () => {
  it("未设置态：提示文案 + 设置入口，不显示值", () => {
    render(<RcManualAddrRow target={target()} busy={false} onSet={vi.fn()} />);
    expect(screen.getByText(/未设置/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "设置" })).toBeTruthy();
  });

  it("已设置态显示当前值，清除走 null", async () => {
    const onSet = vi.fn(async () => true);
    render(
      <RcManualAddrRow
        target={target({ manual_addr: "192.168.1.5:4919" })}
        busy={false}
        onSet={onSet}
      />,
    );
    expect(screen.getByText("192.168.1.5:4919")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "清除" }));
    expect(onSet).toHaveBeenCalledWith("a".repeat(64), null);
  });

  it("编辑→Enter 保存带值；Esc 取消不调后端", async () => {
    const onSet = vi.fn(async () => true);
    render(<RcManualAddrRow target={target()} busy={false} onSet={onSet} />);
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    const input = screen.getByLabelText("直连地址");
    fireEvent.change(input, { target: { value: "[fd00::9]:4919" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onSet).toHaveBeenCalledWith("a".repeat(64), "[fd00::9]:4919");
    // Esc 路径：重新进编辑再按 Esc，不产生新的后端调用
    //（Enter 的保存是异步的，先等回到未设置态再进第二轮）
    const calls = onSet.mock.calls.length;
    fireEvent.click(await screen.findByRole("button", { name: "设置" }));
    fireEvent.keyDown(screen.getByLabelText("直连地址"), { key: "Escape" });
    expect(onSet.mock.calls.length).toBe(calls);
  });

  it("保存失败不退出编辑态（草稿还在，用户可改了再存）", async () => {
    const onSet = vi.fn(async () => false);
    render(<RcManualAddrRow target={target()} busy={false} onSet={onSet} />);
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    const input = screen.getByLabelText("直连地址");
    fireEvent.change(input, { target: { value: "10.0.0.7" } });
    await fireEvent.click(screen.getByRole("button", { name: "保存" }));
    expect(onSet).toHaveBeenCalledWith("a".repeat(64), "10.0.0.7");
    expect(screen.getByLabelText("直连地址")).toBeTruthy();
    expect((screen.getByLabelText("直连地址") as HTMLInputElement).value).toBe("10.0.0.7");
  });

  it("空草稿禁用保存（清空用「清除」按钮，不是存空串）", () => {
    render(<RcManualAddrRow target={target()} busy={false} onSet={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    expect(
      (screen.getByRole("button", { name: "保存" }) as HTMLButtonElement).disabled,
    ).toBe(true);
  });
});
