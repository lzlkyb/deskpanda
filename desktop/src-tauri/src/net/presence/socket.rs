//! 组播侧：v4/v6 两个监听套接字，加「往每一块网卡各发一份公告」的发送路径
//! （两个协议族各发各的——P1-b）。
//!
//! 收发用**不同的套接字**：绑同一个端口再往它发包在各平台行为不一致，
//! 而发送方根本不需要固定端口（接收方只看源 IP）。理由见 [`announce_once`]。
//!
//! v6 侧的存活口径是「**多一条路**而不是换路」：bind/join 全失败时 v6 为 `None`，
//! v4 照常工作（纯 v4 机器上这是常态，不是错误）；反之 v4 失败仍然像从前一样
//! 让整套 presence 起不来——v4 是现状大多数环境的唯一路，不能被 v6 的失败连坐。

use super::wire::{build, build_kind, Extras, PresenceApp, WireKind};
use super::{GROUP, GROUP_V6, PORT};
use crate::net::identity::NodeIdentity;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6, UdpSocket};

/// 监听用套接字对：v4 必在，v6 尽力（见模块说明的存活口径）。
pub struct Listener {
    /// v4 侧：`0.0.0.0:port`，阻塞 + 2 秒读超时（线程的节拍器）。
    pub v4: UdpSocket,
    /// v6 侧：`[::]:port`（V6ONLY）、逐网卡 join `ff02::` 组、**非阻塞**——
    /// 由主循环在 v4 醒来时抽干（见 `spawn` 的循环）。
    pub v6: Option<UdpSocket>,
}

/// 监听用套接字：绑 [`PORT`]、加入组播组、带读超时（好让线程能停下来）。
pub fn bind_listener() -> Result<Listener, String> {
    bind_listener_on(PORT)
}

/// 同上，但可以指定端口。**测试用 0（临时端口）**——
/// 固定 5008 会让并行跑的测试互相抢，表现为随机失败（与被测逻辑无关）。
pub fn bind_listener_on(port: u16) -> Result<Listener, String> {
    // 🔴 重试而不是一次就放弃：`stop()` 只置标志，宣告线程要等它那 2 秒读超时
    // 走完才真的退出。用户在这 2 秒内关了再开（或前端连点开关），
    // 新线程就会 bind 失败、只留一条 warning 退出——
    // **地址发现静默死掉，而开关看着是开的**，正是我们想修掉的那个症状。
    let mut last = String::new();
    for i in 0..6 {
        match UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port)) {
            Ok(s) => {
                let v4 = finish_listener(s)?;
                let v6 = bind_listener_v6(port);
                return Ok(Listener { v4, v6 });
            }
            Err(e) => {
                last = e.to_string();
                if i < 5 {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
            }
        }
    }
    Err(format!(
        "绑定地址公告端口 {} 失败（重试 6 次仍被占用）：{}",
        port, last
    ))
}

fn finish_listener(sock: UdpSocket) -> Result<UdpSocket, String> {
    // 🔴 逐块网卡加入组播组。这里原本是 `Ipv4Addr::UNSPECIFIED`，
    //    而它在组播语境下**不是「所有网卡」**，是「让系统按路由表挑一块」——
    //    装了 VMware / VirtualBox / Hyper-V / WSL / Docker / VPN 的机器上
    //    经常挑中虚拟网卡，于是同网段的两台机器互相听不见。
    //
    // ❗ 这不是新发现：`lan_sync` 2026-09-06 就因为同一件事修过一次
    //   （见 `lan_sync::multicast_ifaces` 的注释），只是没搬到这个新模块。
    //   本模块里它的症状更隐蔽：不表现为「同步不了」，而是 `service::target()`
    //   拿不到局域网地址 → 退到只给 node_id 的兜底分支 → **走 n0 公共中继**，
    //   同一局域网的两台机器静静地绕道跨国同步，而界面徽章还写着「外网」。
    let mut joined = 0usize;
    for ifaddr in crate::net::mcast::multicast_ifaces() {
        match sock.join_multicast_v4(&GROUP, &ifaddr) {
            Ok(()) => joined += 1,
            // 虚拟网卡加不进去是常态，逐块 warn 会刷屏
            Err(e) => log::debug!("[Presence] 网卡 {} 加入组播组失败：{}", ifaddr, e),
        }
    }
    if joined == 0 {
        // ❗ 不像 `lan_sync` 那样直接放弃线程：本模块的宣告是**单向也有用**的
        //   （只要对端听得见我们，它就会拨过来，而同步会话本来就是双向的）。
        //   但绝不能静默（规则 #15.3）：这意味着本机永远听不到任何公告。
        log::error!(
            "[Presence] 没能在任何一块网卡上加入组播组，本机听不到对端的地址公告（同步会退到中继）"
        );
    } else {
        log::info!("[Presence] 已在 {} 块网卡上加入组播组", joined);
    }
    sock.set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .map_err(|e| format!("设置读取超时失败：{}", e))?;
    Ok(sock)
}

/// v6 侧的监听套接字。**任何失败都返回 `None` 而不报错给上层**——
/// v6 是「多一条路」（见模块说明的存活口径），纯 v4 机器上这块整个缺席是常态。
///
/// 三件事与 v4 侧刻意不同，都有理由：
/// - `IPV6_V6ONLY`：不设的话（Linux 默认双栈）`[::]` 会把 v4-mapped 也收进来，
///   与 v4 套接字抢同一个 `0.0.0.0:port`，bind 直接失败；
/// - join 用**接口索引**（v6 组播 API 的口径），组是 [`GROUP_V6`]（ff02:: 域）；
/// - 非阻塞：v4 套接字是这套线程的节拍器（2 秒读超时），v6 靠主循环顺带抽干，
///   不自己阻塞——两个阻塞套接字就得两个线程或 select，为一条「多余的路」
///   不值当。
fn bind_listener_v6(port: u16) -> Option<UdpSocket> {
    use socket2::{Domain, Protocol, SockAddr, Socket, Type};

    let make = || -> std::io::Result<UdpSocket> {
        let sock = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))?;
        sock.set_only_v6(true)?;
        sock.bind(&SockAddr::from(SocketAddr::from(SocketAddrV6::new(
            Ipv6Addr::UNSPECIFIED,
            port,
            0,
            0,
        ))))?;
        Ok(sock.into())
    };
    let sock = match make() {
        Ok(s) => s,
        // 没有 IPv6 栈的机器（少数虚拟机/容器）——v4 照常工作，不值得 warn
        Err(e) => {
            log::debug!(
                "[Presence] 绑 v6 组播端口 {} 失败（纯 v4 环境？）：{}",
                port,
                e
            );
            return None;
        }
    };
    let mut joined = 0usize;
    for iface in crate::net::mcast::multicast_ifaces_v6() {
        match sock.join_multicast_v6(&GROUP_V6, iface.index) {
            Ok(()) => joined += 1,
            // 虚拟网卡加不进去是常态（同 v4 侧），逐块 warn 会刷屏
            Err(e) => log::debug!(
                "[Presence] 网卡 #{}（{}）加入 v6 组播组失败：{}",
                iface.index,
                iface.addr,
                e
            ),
        }
    }
    if joined == 0 {
        // ❗ info 而不是 error：v4 侧「一块都 join 不进」意味着整套发现废了，
        //   而 v6 侧一块都进不去的最大可能是「这台机器根本没有 v6」——
        //   现状大多数环境如此，按 error 报会在每台 v4 机器上刷假警报。
        //   但也不能静默（规则 #15.3）：真有 v6 却 join 不进去时，这条是唯一线索。
        log::info!("[Presence] 没能在任何网卡上加入 v6 组播组（纯 v4 环境？本机听不到 v6 公告）");
    } else {
        log::info!("[Presence] 已在 {} 块网卡上加入 v6 组播组", joined);
    }
    match sock.set_nonblocking(true) {
        Ok(()) => Some(sock),
        Err(e) => {
            log::warn!("[Presence] v6 监听设非阻塞失败，弃用 v6 侧：{}", e);
            None
        }
    }
}

/// 宣告一次要的全部参数。
///
/// 收成结构体是因为里面有**两个 `u16`**（`endpoint_port` / `group_port`）——
/// 位置参数下把它们写反，编译器不会报错，只会「对端照着错端口拨号」，
/// 而且从外面**完全看不出来**（2026-09-17 已经在 `commands/kb_sync.rs`
/// 踩过一次同款：把 presence 的端口当成端点端口写进了公告）。
pub struct Announce {
    /// 本套 presence 的用途，写进公告供接收侧认串台。
    pub app: PresenceApp,
    /// 写进公告的端口：本机 **iroh 端点**端口，供对端拨号。
    pub endpoint_port: u16,
    /// 包发往的组播端口：**本套 presence** 的端口，也是本机在听的那个。
    pub group_port: u16,
    pub now_ms: i64,
}

/// 往**每一块**网卡、**每一个协议族**各发一份地址公告。
///
/// 发送与监听用不同的套接字：绑同一个端口再往它发包在各平台行为不一致，
/// 而发送方根本不需要固定端口（接收方只看源 IP）。
///
/// 🔴 这里原本是一个 `bind(0.0.0.0:0)` 的 socket 直接 `send_to`，
/// 没有 `set_multicast_if_v4`——包从系统按路由表挑的那一块出去，
/// 在有虚拟网卡的机器上经常根本没上真实局域网。理由同 `finish_listener`。
///
/// ❗ 复用 `lan_sync` 那套（带 30 秒网卡缓存），不重写一份：
/// `netdev::get_interfaces()` 不便宜，而它已经在招呼包路径上被缓存过了。
///
/// 🔴 `group_port` 必须由调用方给：本进程里跑着**两套** presence（5008 知识库同步 /
/// 5009 远程电脑），写死就串台——2026-09-17 的事故见 [`super::PresenceStart::port`]。
/// `app` 同理，它是**事后**认出串台的唯一手段（[`super::Heard::WrongApp`]）。
pub fn announce_once(me: &NodeIdentity, a: Announce) -> Result<(), String> {
    let Announce {
        app,
        endpoint_port,
        group_port,
        now_ms,
    } = a;
    let mut sent = 0usize;
    let mut last = String::new();
    // ts 的递增序号**跨协议族连续**：v4 与 v6 的每份包都各是唯一 ts——
    // 重放检查按节点全局严格递增，同族不同网卡会撞，跨族同理。
    let mut seq = 0usize;
    for ifaddr in crate::net::mcast::multicast_ifaces() {
        // 🔴 每块网卡**各建一份包、ts 不同**（2026-09-07 修）。
        //
        //    旧写法是上层 `build()` 一次、这里把**同一份字节**往每块网卡各发一份，
        //    于是 N 份包带着**同一个 `ts`**。而接收侧的严格递增重放检查
        //    （`wire.ts <= entry.last_ts` ⇒ `return Heard::Replay`）就在学习源地址
        //    那几行**之前**：第一份被接受并写 `last_ts`，其余每一份都被当重放
        //    丢掉、**永远走不到学地址**。后果：`service::target()` 只拿到一个候选地址，
        //    如果那一个是不可路由的网卡（Hyper-V / VMware 虚拟网卡很常见），
        //    打洞就失败、回落公共 relay——而旧的 `transport_of()` 仍然报 "lan"。
        //    （现在不会了：传输方式改成从活连接实测，这种情况徽章会写「绕中继」——
        //    但上面那个只学得到一个候选地址的 bug 本身仍然得防。）
        //
        //    `now_ms + seq` 而不是改接收侧的检查顺序：把重放检查挪到学地址之后，
        //    等于允许一个第三方重放旧包把**自己的 IP** 登记到那个 node_id 名下；
        //    而每包不同 ts 既保住了严格递增的重放防护，也让 N 个源地址都能被学到。
        //    代价：每次宣告多签 N-1 次名（ed25519，微秒级）与最多几毫秒的 ts 偏差，
        //    而 `CLOCK_WINDOW_MS` 是分钟级的，不会因此被卡。
        let packet = build(me, app, endpoint_port, now_ms + seq as i64)?;
        seq += 1;
        match crate::net::mcast::send_via_iface(&ifaddr, GROUP, group_port, &packet) {
            Ok(()) => sent += 1,
            Err(e) => {
                // 虚拟网卡发不出去是常态，逐块 warn 会刷屏
                log::debug!("[Presence] 经网卡 {} 发送地址公告失败：{}", ifaddr, e);
                last = e.to_string();
            }
        }
    }
    for iface in crate::net::mcast::multicast_ifaces_v6() {
        let packet = build(me, app, endpoint_port, now_ms + seq as i64)?;
        seq += 1;
        match crate::net::mcast::send_via_iface_v6(iface.index, GROUP_V6, group_port, &packet) {
            Ok(()) => sent += 1,
            Err(e) => {
                log::debug!(
                    "[Presence] 经网卡 #{}（{}）发送 v6 地址公告失败：{}",
                    iface.index,
                    iface.addr,
                    e
                );
                last = e.to_string();
            }
        }
    }
    if sent == 0 {
        // 一块都没发出去 = 对端永远不知道本机地址，必须报（规则 #15.3）
        return Err(format!("地址公告没能从任何一块网卡发出去：{}", last));
    }
    Ok(())
}

/// 把一个**已经做好的**包往每一块网卡、**每一个协议族**各发一份。
///
/// 与 [`announce_once`] 的分工是「发」与「做包」：那个负责做包（每块网卡一份、
/// `ts` 递增），这个只负责发——**同一份字节往每块网卡各发一遍**（v4 组、v6 组
/// 发的字节一模一样，P1-b 不动线上格式一个字节）。
///
/// ❗ 所以两个调用方的要求不一样，别把其中一个的特性当成这个函数的特性：
/// - 配对握手包（`rc::discovery::send`、`tick`）：一次性动作包，一份字节发全部网卡正合适。
/// - 招呼包（`presence::spawn` 的 5 秒心跳）：**周期**包，每份靠自己的 `ts`
///   往下走，同样不需要按网卡错开（接收侧的严格递增重放检查只对地址公告生效，
///   见 `table::hear_plain`）。
///
/// 需要「每块网卡一份、`ts` 递增」的只有地址公告，走 [`announce_once`]。
///
/// `group_port` 与 `announce_once` 同一个理由必须由调用方给：本进程里跑着两套
/// presence（5008 知识库同步 / 5009 远程电脑），写死就串台。
pub fn send_all(group_port: u16, packet: &[u8]) -> Result<(), String> {
    let mut sent = 0usize;
    let mut last = String::new();
    for ifaddr in crate::net::mcast::multicast_ifaces() {
        match crate::net::mcast::send_via_iface(&ifaddr, GROUP, group_port, packet) {
            Ok(()) => sent += 1,
            Err(e) => {
                // 虚拟网卡发不出去是常态，逐块 warn 会刷屏
                log::debug!("[Presence] 经网卡 {} 发送失败：{}", ifaddr, e);
                last = e.to_string();
            }
        }
    }
    for iface in crate::net::mcast::multicast_ifaces_v6() {
        match crate::net::mcast::send_via_iface_v6(iface.index, GROUP_V6, group_port, packet) {
            Ok(()) => sent += 1,
            Err(e) => {
                log::debug!(
                    "[Presence] 经网卡 #{}（{}）v6 发送失败：{}",
                    iface.index,
                    iface.addr,
                    e
                );
                last = e.to_string();
            }
        }
    }
    if sent == 0 {
        // 与 `announce_once` 同一条判据：一块都没发出去就等于这个动作没发生，
        // 而调用方（配对、招呼）都在等结果，静默失败会让人以为是对方没响应。
        return Err(format!("包没能从任何一块网卡发出去：{}", last));
    }
    Ok(())
}

/// 做一份**招呼包**（[`WireKind::Hello`]）：告诉同网段「我是谁、我叫什么」。
///
/// 只做包、不发——发送由调用方走 [`send_all`]。
///
/// 🔴 **单独抽出来是为了让它能被测试碰到**。2026-09-17 那版的教训：
/// 收包侧（`table::hear_plain` → `Nearby`）写得挺全、`sync/tests.rs` 里
/// 九条用例全过，而**发的那一侧压根没接上**——「附近的设备」永远是空的。
/// 原因是测试里的招呼包全是**自己用 `build_kind` 现造的**，生产路径一次没跑。
///
/// 间隔由调用方定（[`super::HELLO_INTERVAL_SECS`]）：招呼包是**周期心跳**，
/// 不是一次性动作——收包侧按「最近 [`super::NEARBY_TTL_MS`] 内听到过」判在不在附近。
pub fn hello_packet(
    me: &NodeIdentity,
    app: PresenceApp,
    endpoint_port: u16,
    name: &str,
    now_ms: i64,
) -> Result<Vec<u8>, String> {
    build_kind(
        me,
        app,
        WireKind::Hello,
        endpoint_port,
        now_ms,
        Extras::hello(name),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v6 组与 v4 组「同编号」：ff02:: 低 32 位必须逐字节等于 224.1.1.1。
    /// 两边各喊各的组，靠这个镜像关系记住「它们是同一份公告的两个族」。
    #[test]
    fn v6组与v4组同编号() {
        let seg = GROUP_V6.segments();
        let low32 = ((seg[6] as u32) << 16) | seg[7] as u32;
        assert_eq!(
            low32,
            u32::from_be_bytes(GROUP.octets()),
            "ff02:: 组的低 32 位应与 v4 组一致（现在两侧是 {GROUP_V6} vs {GROUP}）"
        );
        // link-local 组播域：不跨路由器，与 v4 组播的可达域对齐（P1 方案 §3 决策 A1）
        assert!(
            GROUP_V6.is_multicast() && GROUP_V6.segments()[0] == 0xff02,
            "v6 组必须在 ff02:: link-local 域"
        );
    }

    /// 监听器冒烟：v4 必须能绑临时端口且带读超时（线程节拍器）；
    /// v6 允许缺席（纯 v4 环境），在场时必须是 v6 地址。
    #[test]
    fn 监听器冒烟_v4必在v6尽力() {
        let l = bind_listener_on(0).expect("绑临时端口不该失败");
        let la = l.v4.local_addr().expect("local_addr");
        assert!(la.is_ipv4());
        assert!(
            l.v4.read_timeout().expect("read_timeout").is_some(),
            "v4 侧必须带读超时——它是整个循环的节拍器"
        );
        if let Some(v6) = &l.v6 {
            assert!(v6.local_addr().expect("local_addr").is_ipv6());
        }
    }
}
