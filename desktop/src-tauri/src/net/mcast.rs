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

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
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
