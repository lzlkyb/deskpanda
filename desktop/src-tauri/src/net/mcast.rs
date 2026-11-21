//! 组播收发：**逐块网卡**，不用 `INADDR_ANY`。
//!
//! # 🔴 为什么必须逐块列出，而不能图省事用 `INADDR_ANY`
//!
//! `Ipv4Addr::UNSPECIFIED` 在组播语境下**不是「所有网卡」**，
//! 而是「让系统按路由表挑一块」。装了 VMware / VirtualBox / Hyper-V /
//! WSL / Docker / VPN 的机器上（现在相当常见），挑中的经常是**虚拟网卡**——于是：
//!   · 发出去的组播包压根没上真实局域网
//!   · 加入组播组也加在了虚拟网卡上，收不到真实局域网的包
//! 两台同网段的机器就此互相看不见，**而且不报任何错**。
//! 这正是 2026-09-06「两台电脑互相发现不了」的根因。
//!
//! ❗ 组播 TTL 用 `set_multicast_ttl_v4` 而不是 `set_ttl`：后者设的是 `IP_TTL`，
//!   对组播从来没生效过（旧代码写了 `set_ttl(2)`，组播 TTL 一直是默认的 1，
//!   同网段仍能通，所以一直没被发现）。
//!
//! IPv6 侧（P1-b）与 v4 完全同构：逐网卡（用**接口索引**而不是网卡上的地址，
//! 这是 v6 组播 API 的口径），收发都在。v6 与 v4 是「多一条路」而不是「换路」——
//! 任何一侧枚举不到/发不出去，另一侧照旧。

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::sync::Mutex;

/// 本机所有可用于组播收发的 IPv4 网卡地址。
fn enumerate_ifaces() -> Vec<Ipv4Addr> {
    let addrs: Vec<Ipv4Addr> = netdev::get_interfaces()
        .into_iter()
        .filter(|i| i.is_up() && !i.is_loopback())
        .flat_map(|i| i.ipv4.into_iter().map(|n| n.addr()))
        .filter(|a| !a.is_loopback() && !a.is_unspecified())
        .collect();
    if addrs.is_empty() {
        // 一块都枚举不到时退回旧行为：宁可只走系统默认那块，也好过一个包都不发
        log::warn!("[Mcast] 没枚举到可用的 IPv4 网卡，退回系统默认网卡");
        return vec![Ipv4Addr::UNSPECIFIED];
    }
    addrs
}

/// 网卡列表缓存：`(枚举时刻, 地址表)`。
static IFACE_CACHE: Mutex<Option<(i64, Vec<Ipv4Addr>)>> = Mutex::new(None);

/// 缓存存活秒数。
const IFACE_CACHE_SECS: i64 = 30;

/// 本机可用于组播收发的 IPv4 网卡地址（带缓存）。
///
/// # 🔴 为什么要缓存
///
/// `netdev::get_interfaces()` **不便宜**：它默认带 `gateway` feature，
/// 除了 `GetAdaptersAddresses` 还要解析每块网卡的网关。而组播发送在
/// 招呼包心跳上是**每 5 秒一次**，再加上每条消息一次——把枚举硬件
/// 放在这条路径上是纯浪费。
///
/// 网卡变动是低频事件（插拔网线、切 Wi-Fi、连 VPN），缓存 30 秒足够新鲜：
/// 最坏情况是切网络后 30 秒内发不出去，而对端的招呼包本来就是 5 秒一发，
/// 发现延迟本就在秒级。
///
/// ❗ 监听侧的 join 只在启动时跑一次，不受缓存影响。
pub(crate) fn multicast_ifaces() -> Vec<Ipv4Addr> {
    let now = chrono::Utc::now().timestamp();
    if let Ok(g) = IFACE_CACHE.lock() {
        if let Some((at, list)) = g.as_ref() {
            if now - *at < IFACE_CACHE_SECS {
                return list.clone();
            }
        }
    }
    let addrs = enumerate_ifaces();
    if let Ok(mut g) = IFACE_CACHE.lock() {
        *g = Some((now, addrs.clone()));
    }
    addrs
}

/// 经指定网卡发一份组播包。
pub(crate) fn send_via_iface(
    ifaddr: &Ipv4Addr,
    group: Ipv4Addr,
    port: u16,
    payload: &[u8],
) -> std::io::Result<()> {
    use socket2::{Domain, Protocol, SockAddr, Socket, Type};

    let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    // 🔴 这一行是关键：不设它，包就从系统按路由表挑的那块网卡出去。
    sock.set_multicast_if_v4(ifaddr)?;
    // ❗ 组播 TTL 不是 `set_ttl`，见模块文档。
    sock.set_multicast_ttl_v4(2)?;
    sock.bind(&SockAddr::from(SocketAddr::from(SocketAddrV4::new(
        *ifaddr, 0,
    ))))?;
    sock.send_to(
        payload,
        &SockAddr::from(SocketAddr::from(SocketAddrV4::new(group, port))),
    )?;
    Ok(())
}

// ===== IPv6 侧（P1-b）：与上面逐条同构，差别只有组播 API 的口径 =====

/// 一块可做 IPv6 组播收发的网卡。
///
/// v6 的组播接口用**接口索引**指定（v4 用网卡上的地址）：join 与
/// `set_multicast_if_v6` 收的都是 `u32` 索引。留一个地址纯粹是为了日志可读——
/// 拨号候选不用它，候选的源地址由接收方从包的源地址取（铁律见 presence 模块说明②）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ipv6Iface {
    pub(crate) index: u32,
    /// 这块网卡上的一个 v6 地址（取枚举到的第一个可用项，可能就是 fe80）。
    /// 只用于日志——真实拨号候选由接收方从包的源地址取，不走这个字段。
    pub(crate) addr: Ipv6Addr,
}

/// 本机所有可用于组播收发的 IPv6 网卡。
///
/// 与 v4 侧的关键差别：**link-local（fe80::）地址也算数**。访客 Wi-Fi 常常
/// 只给 fe80 不给 ULA/全局地址——那正是 P1 要覆盖的「纯 v6 网段」，把 fe80
/// 排除掉就等于把这类网段整个放弃。fe80 作候选的降级规则在 presence 表那边
/// （末位兜底），这里只回答「这块网卡能不能收发组播」。
fn enumerate_ifaces_v6() -> Vec<Ipv6Iface> {
    netdev::get_interfaces()
        .into_iter()
        .filter(|i| i.is_up() && !i.is_loopback() && i.index != 0)
        .filter_map(|i| {
            let addr = i
                .ipv6
                .iter()
                .map(|n| n.addr())
                .find(|a| !a.is_loopback() && !a.is_unspecified())?;
            Some(Ipv6Iface {
                index: i.index,
                addr,
            })
        })
        .collect()
}

/// v6 网卡列表缓存，与 v4 侧同一个理由（`netdev` 枚举不便宜，心跳路径别反复付）。
static IFACE_CACHE_V6: Mutex<Option<(i64, Vec<Ipv6Iface>)>> = Mutex::new(None);

/// 本机可用于组播收发的 IPv6 网卡（带缓存）。
///
/// ❗ 枚举不到时返回**空**而不是像 v4 那样退回「系统默认」：v4 的兜底是
/// `UNSPECIFIED` 这个有明确语义的值，v6 侧对应的「接口索引 0」在多数平台上
/// 不是合法接口——硬塞只会把错误推后到 join/send 时再报一遍。空表由调用方
/// 按「这一族没有路」处理（v4 侧照常工作）。
pub(crate) fn multicast_ifaces_v6() -> Vec<Ipv6Iface> {
    let now = chrono::Utc::now().timestamp();
    if let Ok(g) = IFACE_CACHE_V6.lock() {
        if let Some((at, list)) = g.as_ref() {
            if now - *at < IFACE_CACHE_SECS {
                return list.clone();
            }
        }
    }
    let addrs = enumerate_ifaces_v6();
    if let Ok(mut g) = IFACE_CACHE_V6.lock() {
        *g = Some((now, addrs.clone()));
    }
    addrs
}

/// 经指定网卡发一份 **IPv6** 组播包。
///
/// 与 v4 版的差别只有两处口径：接口用索引指定；组播跳数用
/// `set_multicast_hops_v6`（v6 版的「组播 TTL 不是 set_ttl」，同模块文档那条）。
/// `ff02::` 是 link-local 域，本就不出本地链路，跳数设 2 只求与 v4 侧对称。
pub(crate) fn send_via_iface_v6(
    iface: u32,
    group: Ipv6Addr,
    port: u16,
    payload: &[u8],
) -> std::io::Result<()> {
    use socket2::{Domain, Protocol, SockAddr, Socket, Type};

    let sock = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))?;
    // 🔴 v6 版的「必须逐网卡」：不设它，包就走系统按路由表挑的那块（同 v4 注释）。
    sock.set_multicast_if_v6(iface)?;
    sock.set_multicast_hops_v6(2)?;
    // 绑 `[::]:0` 只挑源端口；出接口已由上面那行钉死，内核在这块网卡上选源地址
    // （有 ULA/全局时优先于 fe80——正好是候选分级的口径，无需这里再排序）。
    sock.bind(&SockAddr::from(SocketAddr::from(SocketAddrV6::new(
        Ipv6Addr::UNSPECIFIED,
        0,
        0,
        0,
    ))))?;
    // 目的地址带上 scope_id（接口索引）：ff02 是 link-local 域，目的地必须指明
    // 哪条链路；与 set_multicast_if_v6 二选一即可成立，两个都给是显式胜过隐式。
    sock.send_to(
        payload,
        &SockAddr::from(SocketAddr::from(SocketAddrV6::new(group, port, 0, iface))),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 枚举出来的每块网卡都必须可指名（索引非 0）、地址可用。
    ///
    /// ❗ 断言写成**条件式**而不是「必须非空」：CI 容器里可能一块 v6 网卡都没有，
    ///    那是合法环境（v4 侧照常工作），不是本函数的 bug。
    #[test]
    fn v6网卡枚举只出可用项() {
        for i in enumerate_ifaces_v6() {
            assert_ne!(i.index, 0, "接口索引 0 不是合法网卡");
            assert!(!i.addr.is_loopback());
            assert!(!i.addr.is_unspecified());
        }
    }

    /// fe80-only 网卡（访客 Wi-Fi 场景）不该在枚举层被丢掉——
    /// 分级是 presence 表的事，这里只管「能不能收发」。
    #[test]
    fn v6枚举不过滤link_local() {
        // 直接构造不了 netdev 的 Interface（字段私有构造不公开），退而钉住
        // 过滤条件本身：过滤谓词里没有对 fe80 的排除。
        let src = include_str!("mcast.rs");
        let body = src
            .split("fn enumerate_ifaces_v6")
            .nth(1)
            .expect("enumerate_ifaces_v6 还在");
        assert!(
            !body.contains("is_unicast_link_local"),
            "枚举层不得按 link-local 过滤——纯 fe80 网段会被整段放弃（P1-b 的目标场景）"
        );
    }

    /// v4 侧的兜底（枚举为空退 UNSPECIFIED）不许被照搬进 v6——
    /// v6 没有「索引 0 = 默认」的可移植语义。
    #[test]
    fn v6枚举为空时不造兜底项() {
        let ifaces = multicast_ifaces_v6();
        // 缓存 30 秒，本测试只验证「要么空、要么全是真接口」，不依赖具体机器
        for i in &ifaces {
            assert_ne!(i.index, 0);
        }
    }
}
