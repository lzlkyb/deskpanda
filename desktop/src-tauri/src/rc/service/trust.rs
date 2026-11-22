//! 设备信任表（免确认/禁止/自动接受）与同步配对 elevate。
//!
//! 2026-09-22 从 `service.rs` 平移（体量合规）。`use crate::rc::*` 沿用
//! `service/mod.rs` 的全部名字；方法体未改动，仅 `pub(super)` 方法
//! 升级为 `pub(in crate::rc)`（原语义就是「rc 子树可见」）。

use super::*;

const CFG_PEER_ADDRS: &str = "rc_peer_addrs";
/// 手动直连地址表：node_id -> 规范化字符串（`ip:port`；裸 IP 存成无端口的 ip）。
const CFG_MANUAL_ADDRS: &str = "rc_manual_addrs";

/// 解析用户手填的直连地址。
///
/// 接受三种形态：`1.2.3.4:端口`、`[fd00::9]:端口`、裸 IP（v4 或 v6 字面量，
/// **不带**方括号/端口）——裸 IP 存为端口 0，拨号时再取对端最新已知端点端口
/// （端点端口是易失的，存死只会过期）。
///
/// 拒绝的：通配地址（0.0.0.0 / `::`——永远拨不通）；带网卡 zone 的 v6
/// （`fe80::1%en0`——std 不认，且 zone 只在本机有意义）。**回环允许**：
/// 直连网线/本机调试是合法场景，身份反正由 QUIC 握手校验。
fn parse_manual_addr(raw: &str) -> Result<std::net::SocketAddr, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Err("直连地址不能为空".into());
    }
    // 先按标准 SocketAddr 试（v4:port 与 [v6]:port 都在这条路上）
    if let Ok(a) = s.parse::<std::net::SocketAddr>() {
        if a.ip().is_unspecified() {
            return Err("0.0.0.0 / :: 不是可拨的地址".into());
        }
        return Ok(a);
    }
    // 再试裸 IP（v4 或 v6 字面量）
    if let Ok(ip) = s.parse::<std::net::IpAddr>() {
        if ip.is_unspecified() {
            return Err("0.0.0.0 / :: 不是可拨的地址".into());
        }
        return Ok(std::net::SocketAddr::new(ip, 0));
    }
    Err(format!(
        "「{s}」不是合法的 IP 直连地址（支持 192.168.1.5:4919、[fd00::9]:4919 或裸 IP）"
    ))
}

/// 规范化存储格式：裸 IP（端口 0）存 `ip`，否则存 `ip:port`。
fn manual_addr_to_str(a: &std::net::SocketAddr) -> String {
    if a.port() == 0 {
        a.ip().to_string()
    } else {
        a.to_string()
    }
}

/// 读某台设备的手动直连地址。存的时候校验过格式；这里读坏值**宽容为无**
/// （配置可能被手改）——别让一条脏配置把拨号整个炸掉。
fn manual_addr_of(config: &serde_json::Value, peer: &str) -> Option<std::net::SocketAddr> {
    let s = config.get(CFG_MANUAL_ADDRS)?.get(peer)?.as_str()?;
    parse_manual_addr(s).ok()
}

/// 配对时收到的已签名地址供后续重连使用；无缓存时仍走 iroh 自带的地址发现。
fn saved_peer_addr(config: &serde_json::Value, peer: &str) -> Option<EndpointAddr> {
    let id = iroh::EndpointId::from_str(peer).ok()?;
    let value = config.get(CFG_PEER_ADDRS)?.get(peer)?.clone();
    let addr: EndpointAddr = serde_json::from_value(value).ok()?;
    (addr.id == id).then_some(addr)
}

impl RcService {
    pub(in crate::rc) fn peer_addr(
        &self,
        peer: &str,
        presence: &PresenceTable,
    ) -> Result<EndpointAddr, String> {
        let id = iroh::EndpointId::from_str(peer).map_err(|e| format!("设备号无效：{e}"))?;
        let config = self.store.get_config()?;
        let mut addr = self
            .joins
            .exchange_for(peer, now_ms())
            .and_then(|intent| intent.addr)
            .or_else(|| saved_peer_addr(&config, peer))
            .unwrap_or_else(|| EndpointAddr::new(id));
        for socket in presence.addrs_of(peer, now_ms()) {
            addr = addr.with_ip_addr(socket);
        }
        // 手动直连地址（IP 直连入口）：只是**多给一条候选**——与 presence 学到的、
        // 中继兜底一起交给 QUIC 竞速（P1 决策 C 同哲学），不覆盖任何已有候选。
        // 填错的最坏结果是这一条不通，其他路径原样可用。
        //
        // 裸 IP（端口 0）：端点端口对端每次重启都可能换，拨号时取**最新**已知端口
        // （presence 优先于存盘的旧地址）；一个都拿不到就跳过这条候选并留日志，
        // 绝不往 0 端口拨。
        if let Some(m) = manual_addr_of(&config, peer) {
            let resolved = match m.port() {
                0 => presence
                    .addrs_of(peer, now_ms())
                    .first()
                    .map(|a| a.port())
                    .or_else(|| addr.ip_addrs().next().map(|a| a.port()))
                    .map(|p| {
                        let mut m = m;
                        m.set_port(p);
                        m
                    }),
                _ => Some(m),
            };
            match resolved {
                Some(m) => addr = addr.with_ip_addr(m),
                None => log::info!(
                    "[ManualAddr] {} 存了裸 IP 但拿不到对端已知端口，本次跳过该候选",
                    &peer[..8.min(peer.len())]
                ),
            }
        }
        Ok(addr)
    }

    /// 设置/清除某台设备的手动直连地址（IP 直连入口）。
    ///
    /// 🔴 前置与 `set_device_trust` 同一条：必须已是本机认可的设备——手动地址
    ///    是「给已配对设备补一条路」，不是「连陌生设备」的口子（身份由 QUIC
    ///    握手校验，但 UI 入口也只对已配对设备存在）。
    pub fn set_manual_addr(&self, peer: &str, addr: Option<&str>) -> Result<(), String> {
        if !self.has_remote_trust(peer) {
            return Err("该设备尚未与本机配对，无法设置直连地址".into());
        }
        let addr = addr.map(str::trim).filter(|s| !s.is_empty());
        let mut config = self.store.get_config()?;
        let obj = config.as_object_mut().ok_or("配置文件不是一个对象")?;
        match addr.map(parse_manual_addr).transpose()? {
            None => {
                if let Some(map) = obj
                    .get_mut(CFG_MANUAL_ADDRS)
                    .and_then(|v| v.as_object_mut())
                {
                    map.remove(peer);
                }
            }
            Some(a) => {
                let entry = obj
                    .entry(CFG_MANUAL_ADDRS)
                    .or_insert_with(|| serde_json::json!({}));
                let map = entry.as_object_mut().ok_or("rc_manual_addrs 格式无效")?;
                map.insert(
                    peer.to_string(),
                    serde_json::Value::String(manual_addr_to_str(&a)),
                );
            }
        }
        self.store.save_config(&config)
    }

    /// 设备详情用：该设备当前的手动直连地址（存盘原样字符串，空 = 未设置）。
    pub fn manual_addr_str(&self, peer: &str) -> Option<String> {
        let config = self.store.get_config().ok()?;
        manual_addr_of(&config, peer).map(|a| manual_addr_to_str(&a))
    }

    pub(in crate::rc) fn save_peer_addr(
        &self,
        peer: &str,
        addr: &EndpointAddr,
    ) -> Result<(), String> {
        if addr.id.to_string() != peer {
            return Err("设备地址与身份不一致".into());
        }
        let mut config = self.store.get_config()?;
        let obj = config.as_object_mut().ok_or("配置文件不是一个对象")?;
        let addrs = obj
            .entry(CFG_PEER_ADDRS)
            .or_insert_with(|| serde_json::json!({}));
        let map = addrs.as_object_mut().ok_or("已保存的设备地址格式无效")?;
        map.insert(
            peer.to_string(),
            serde_json::to_value(addr).map_err(|e| e.to_string())?,
        );
        self.store.save_config(&config)
    }

    pub fn forget_peer_addr(&self, peer: &str) -> Result<(), String> {
        let mut config = self.store.get_config()?;
        if let Some(map) = config
            .get_mut(CFG_PEER_ADDRS)
            .and_then(|v| v.as_object_mut())
        {
            map.remove(peer);
            self.store.save_config(&config)?;
        }
        Ok(())
    }
    pub fn joins(&self) -> Arc<RcJoins> {
        self.joins.clone()
    }

    pub(in crate::rc) fn cfg(&self) -> serde_json::Value {
        self.store.get_config().unwrap_or_default()
    }

    pub fn enabled(&self) -> bool {
        // 唯一读取点在 `super::cfg_enabled`（含缺省值），别在这里再写一份默认判断。
        super::cfg_enabled(&self.store)
    }

    /// 本机是否开了「会话期间防休眠」。唯一读取点在 `super::cfg_keep_awake`。
    pub fn keep_awake(&self) -> bool {
        super::cfg_keep_awake(&self.store)
    }

    pub fn max_capability(&self) -> Capability {
        self.cfg()
            .get(CFG_CAPABILITY)
            .and_then(|v| v.as_str())
            .and_then(Capability::parse)
            .unwrap_or(Capability::View)
    }

    pub fn device_deny(&self) -> HashMap<String, bool> {
        self.cfg()
            .get(CFG_DEVICE_DENY)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }

    /// 是否在**远程**配对表里。
    /// 是否在**远程**配对表里（只认 `rc_devices`，不含笔记同步表）。
    /// B-b：文件通道 / 免确认 / 自动收文件的准入判据。
    pub(in crate::rc) fn is_rc_paired(&self, node_id: &str) -> bool {
        matches!(self.store.rc_device_get(node_id), Ok(Some(_)))
    }

    /// 远程信任：远程配对 **或** 同步配对（方案 A 单向继承）。
    ///
    /// 🔴 B-b（2026-09-20 拍板）：这条只用于「能不能敲门 / 列表可见」；
    ///    **文件通道与免确认/自动接收只认 `rc_devices`**（`is_rc_paired`）。
    ///    同步配对设备首次要通过会话批准 elevate 写入 rc 表后，才获得文件准入。
    pub fn has_remote_trust(&self, node_id: &str) -> bool {
        if self.is_rc_paired(node_id) {
            return true;
        }
        matches!(self.store.device_get(node_id), Ok(Some(_)))
    }

    /// 同步设备首次被**人工批准**远程会话时写入 rc_devices（幂等）。
    ///
    /// B-b：同意一次会话 = 用户确认「这台同步设备可以远程我」，此后它就是
    /// 正式远程设备（可开免确认 / 自动收文件）。不在敲门时自动 elevate。
    pub fn elevate_from_sync(&self, node_id: &str) -> Result<(), String> {
        if self.is_rc_paired(node_id) {
            return Ok(());
        }
        let name = self
            .store
            .device_get(node_id)
            .ok()
            .flatten()
            .map(|d| d.name)
            .unwrap_or_else(|| "同步设备".into());
        self.store.rc_device_pair(node_id, &name)
    }

    /// 方案 D：这台设备是否开了「免确认直连」。
    ///
    /// 只认 rc 配对表——仅同步配对、还没被远程用过的设备没有行可查，
    /// 视为未开启（合理：免确认的前提是这台设备已经用过至少一次远程）。
    pub fn device_trusted(&self, node_id: &str) -> bool {
        matches!(self.store.rc_device_get(node_id), Ok(Some(d)) if d.trusted)
    }

    /// 决策 10：这台设备是否开了「自动接收文件」。
    ///
    /// 与 `device_trusted` 同一口径：只认 rc 配对表，仅同步配对、还没被远程用过
    /// 的设备没有行可查 → 视为未开启（合理：自动接收的前提是它已经用过至少一次）。
    pub fn device_auto_accept(&self, node_id: &str) -> bool {
        matches!(self.store.rc_device_get(node_id), Ok(Some(d)) if d.auto_accept)
    }

    /// 方案 D：设置「免确认直连」。仅同步配对的设备先幂等提升进 rc 表再设。
    ///
    /// 🔴 必须先确认**已配对**（rc 表或 sync 表有行）：对未知 id 不能顺手
    ///    `elevate_from_sync` ——那会凭空造出一行 rc 配对（名字「同步设备」）、
    ///    `has_remote_trust` 立刻变 true，等于绕过 SAS 配对把陌生设备放进白名单。
    pub fn set_device_trust(&self, node_id: &str, trusted: bool) -> Result<(), String> {
        if !self.has_remote_trust(node_id) {
            return Err("该设备尚未与本机配对，无法设置免确认".into());
        }
        self.elevate_from_sync(node_id)?;
        self.store.rc_device_trust_set(node_id, trusted)
    }

    /// 决策 10：设置「自动接收此设备推送的文件」。
    ///
    /// 前置条件与 `set_device_trust` 完全一致（要先是本机认可的设备），
    /// 少任何一个都会让开关在界面上点得动、却写不进去。
    ///
    /// 🔴 它只影响**要不要弹确认条**，不影响门禁：`gate_inbound` 一律先跑。
    /// 🔴 只对推送方向生效（对方发给我）。
    pub fn set_device_auto_accept(&self, node_id: &str, on: bool) -> Result<(), String> {
        if !self.has_remote_trust(node_id) {
            return Err("该设备尚未与本机配对，无法设置自动接收".into());
        }
        self.elevate_from_sync(node_id)?;
        self.store.rc_device_auto_accept_set(node_id, on)
    }

    pub(in crate::rc) fn peer_name(&self, node_id: &str) -> String {
        if let Ok(Some(d)) = self.store.rc_device_get(node_id) {
            return d.name;
        }
        if let Ok(Some(d)) = self.store.device_get(node_id) {
            return d.name;
        }
        String::new()
    }
}

/// 配对缓存地址的守卫。
///
/// ❗ 位置在文件末尾而不是紧跟 `saved_peer_addr`：`clippy::items_after_test_module`
/// 要求 test 模块之后不能再有生产项，而这些测试要覆盖整个 `impl RcService` 块
/// （`peer_addr` 在下面几屏）。放到末尾是那条 lint 指定的解法。
#[cfg(test)]
mod short_pair_addr_tests {
    use super::*;

    #[test]
    fn saved_address_must_match_authenticated_device_id() {
        let peer = iroh::SecretKey::from_bytes(&[7; 32]).public().to_string();
        let other = iroh::SecretKey::from_bytes(&[8; 32]).public();
        let mut config = serde_json::json!({ "rc_peer_addrs": {} });
        config[CFG_PEER_ADDRS][peer.as_str()] =
            serde_json::to_value(EndpointAddr::new(other)).unwrap();
        assert!(saved_peer_addr(&config, &peer).is_none());
        let id = iroh::EndpointId::from_str(&peer).unwrap();
        config[CFG_PEER_ADDRS][peer.as_str()] =
            serde_json::to_value(EndpointAddr::new(id)).unwrap();
        assert_eq!(saved_peer_addr(&config, &peer).unwrap().id, id);
    }

    #[test]
    fn 手动地址_三种合法形态都解析得动() {
        use std::net::SocketAddr;
        let v4 = parse_manual_addr(" 192.168.1.5:4919 ").unwrap();
        assert_eq!(v4, SocketAddr::from(([192, 168, 1, 5], 4919)));
        let v6 = parse_manual_addr("[fd00::9]:4919").unwrap();
        assert_eq!(v6.port(), 4919);
        assert_eq!(v6.ip().to_string(), "fd00::9");
        // 裸 IP（v4 / v6）：端口 0 = 拨号时再补
        assert_eq!(parse_manual_addr("10.0.0.7").unwrap().port(), 0);
        assert_eq!(parse_manual_addr("fe80::1").unwrap().port(), 0);
    }

    #[test]
    fn 手动地址_通配与垃圾串都拒() {
        assert!(parse_manual_addr("0.0.0.0:4919").is_err(), "通配 v4 不可拨");
        assert!(parse_manual_addr("[::]:4919").is_err(), "通配 v6 不可拨");
        assert!(parse_manual_addr("").is_err());
        assert!(parse_manual_addr("   ").is_err());
        assert!(parse_manual_addr("not-an-ip").is_err());
        assert!(parse_manual_addr("192.168.1.5:").is_err());
        // 回环**允许**：直连网线 / 本机调试是合法场景，身份由 QUIC 握手校验
        assert!(parse_manual_addr("127.0.0.1:4919").is_ok());
    }

    #[test]
    fn 手动地址_存储格式往返一致() {
        use std::net::SocketAddr;
        let full = SocketAddr::from(([192, 168, 1, 5], 4919));
        assert_eq!(manual_addr_to_str(&full), "192.168.1.5:4919");
        let bare = SocketAddr::from(([10, 0, 0, 7], 0));
        assert_eq!(manual_addr_to_str(&bare), "10.0.0.7");
        // 序列化 → 反解析无损（裸 IP 回来还是端口 0）
        assert_eq!(parse_manual_addr(&manual_addr_to_str(&bare)).unwrap(), bare);
    }

    #[test]
    fn 手动地址_坏配置宽容为无_不炸拨号() {
        let config = serde_json::json!({
            CFG_MANUAL_ADDRS: {
                "peer-ok": "192.168.1.5:4919",
                "peer-bad": "被手改坏的值",
                "peer-empty": "",
            }
        });
        assert!(manual_addr_of(&config, "peer-ok").is_some());
        assert!(manual_addr_of(&config, "peer-bad").is_none(), "坏值当没有");
        assert!(manual_addr_of(&config, "peer-empty").is_none());
        assert!(manual_addr_of(&config, "peer-missing").is_none());
    }
}
