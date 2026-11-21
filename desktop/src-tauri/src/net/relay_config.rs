//! 自建中继的客户端配置（P0 客户端侧，方案
//! `docs/P0-自建中继-客户端方案-2026-11-21.md`，决策 A1/B1/C1 已拍板）。
//!
//! # 现在合并的安全依据（常量为空 = 零行为变化）
//!
//! [`DEFAULT_RELAY_URL`] 为空时 [`relay_mode`] 原样返回 `RelayMode::Default`（纯 n0），
//! 与改动前的代码路径**逐字节一致**——不建自建节点、不拼 token、连 n0 兜底合并
//! 那一步都不执行。服务器就位后改两个常量重发版即接入（§5 接入清单 30 分钟）。
//!
//! # 为什么不做运行时配置（决策 A1 的取舍）
//!
//! 端点是**进程级单例、永不关闭**（`endpoint.rs` 模块头红线）：运行时换中继
//! 等于销毁重建端点，会把所有在途会话一起杀掉。而 Gitee 镜像的教训
//! （2026-11-20）是装在用户机器上的应用**读不到 env**——默认值必须编译期钉死，
//! env 只留给本地调试 / 灰度。
//!
//! # n0 兜底（决策 B1）
//!
//! 自建排最前 + n0 生产表全量合并（[`RelayMap::extend`]）：自建宕机/被墙的时段
//! 退回现状（n0 美东/美西/欧洲/孟买），而不是退回「连不上」。iroh 按延迟择
//! home relay，香港 BGP 对国内用户恒优于 n0 各点。验证期满想撤兜底：删掉
//! `extend` 那一行即可。
//!
//! # QAD 端口口径（🔴 与交接文档 §5.2 的出入，2026-11-21 实查修订）
//!
//! 方案稿与交接文档早先写的「QAD 7824」是旧口径：iroh 1.1 的
//! `DEFAULT_RELAY_QUIC_PORT` 是 **7842**，n0 生产表四台走的都是这个默认，
//! iroh-relay 服务端文档也写明「Normally you'd choose port 7842」。客户端这里
//! 不写死端口、直接 `RelayConfig::from(url)` 吃默认（7842）；**服务端部署时
//! QUIC 监听与安全组放行都必须是 UDP 7842**，两边同口 QAD 才成立。

use iroh::{defaults, Endpoint, RelayConfig, RelayMap, RelayMode, RelayUrl, Watcher as _};
use std::sync::Arc;

/// 🔴 唯一真源。服务器就位后改这里 + 重发版；**不做运行时配置**（决策 A1）。
const DEFAULT_RELAY_URL: &str = ""; // 形如 https://relay.<域名>/
const DEFAULT_RELAY_TOKEN: &str = ""; // 服务端 access.shared_token

/// env 覆盖键：仅本地调试 / 灰度。空串与缺省都视为「未设置」。
const ENV_URL: &str = "DESKPANDA_RELAY_URL";
const ENV_TOKEN: &str = "DESKPANDA_RELAY_TOKEN";

/// 端点该用哪套中继表。`Endpoint::builder(presets::N0).relay_mode(relay_mode())`。
pub fn relay_mode() -> RelayMode {
    relay_mode_from(
        &env_or(ENV_URL, DEFAULT_RELAY_URL),
        &env_or(ENV_TOKEN, DEFAULT_RELAY_TOKEN),
    )
}

/// 纯函数主体（单测打它；env 只在外层读，测试不碰环境）。
///
/// 🔴 URL 无效（解析失败 / 非 https）时**回落 `Default` 并报 error**，而不是让
/// `bind()` 失败把整个共享端点带崩——配置写错最坏退回 n0，不能退回「没法联网」。
/// 非 https 一律拒：token 走 `Authorization` 头也拦不住明文链路被旁路读取。
fn relay_mode_from(url: &str, token: &str) -> RelayMode {
    let url = url.trim();
    if url.is_empty() {
        return RelayMode::Default;
    }
    let parsed: RelayUrl = match url.parse() {
        Ok(u) => u,
        Err(e) => {
            log::error!("[RelayCfg] 自建中继 URL「{url}」解析失败（{e}），回落纯 n0");
            return RelayMode::Default;
        }
    };
    if parsed.scheme() != "https" {
        log::error!("[RelayCfg] 自建中继 URL 必须是 https（拿到 {parsed}），回落纯 n0");
        return RelayMode::Default;
    }
    // `From<RelayUrl>`：QAD 走默认（7842，与 n0 生产表同口径），不写死端口。
    // 逐中继挂 token（决策 A1 口径）：整表级 with_auth_token 会把 token 发给 n0，不用。
    let mut cfg = RelayConfig::from(parsed.clone());
    if !token.is_empty() {
        cfg = cfg.with_auth_token(token);
    }
    let map = RelayMap::empty();
    map.insert(parsed, Arc::new(cfg));
    // 决策 B1：n0 生产表全量兜底（自建条目已插入，同 URL 不会被覆盖）。
    map.extend(&defaults::prod::default_relay_map());
    RelayMode::Custom(map)
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.to_string())
}

/// 当前 home 中继（决策 C1 的取数口，诊断卡与会话状态**共用这一个**，别再抄一份）。
///
/// 返回 `(URL 字符串, 是否已连接)`；`None` = 还没选中 home relay（刚启动 /
/// 未启用中继 / 表为空）。iroh 1.1 的 `RelayStatus` 只暴露 `is_connected()`
/// （没有 connecting 态）——「已连接 / 未连接」两态是 API 的诚实上限。
pub fn home_relay(ep: &Endpoint) -> Option<(String, bool)> {
    let mut w = ep.home_relay_status();
    let entries = w.get();
    let s = entries
        .iter()
        .find(|s| s.is_connected())
        .or_else(|| entries.first())?;
    Some((s.url().to_string(), s.is_connected()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// n0 生产表里的一台（美东），取自 iroh defaults 源码常量同值。
    const N0_HOST: &str = "https://use1-1.relay.n0.iroh.link./";

    #[test]
    fn 空url_回落纯n0() {
        assert!(matches!(relay_mode_from("", "tok"), RelayMode::Default));
        // 空白串同样视为未配置
        assert!(matches!(relay_mode_from("   ", ""), RelayMode::Default));
    }

    #[test]
    fn 非https或坏url_回落纯n0_不崩() {
        assert!(matches!(
            relay_mode_from("http://relay.example.com/", "tok"),
            RelayMode::Default
        ));
        assert!(matches!(
            relay_mode_from("not a url at all", "tok"),
            RelayMode::Default
        ));
    }

    #[test]
    fn 自建就位_表内必含自建与n0兜底() {
        let RelayMode::Custom(map) = relay_mode_from("https://relay.example.com/", "tok") else {
            panic!("配置了 URL 就该是 Custom");
        };
        let urls: Vec<String> = map.urls::<Vec<_>>().iter().map(|u| u.to_string()).collect();
        assert!(
            urls.contains(&"https://relay.example.com/".to_string()),
            "自建必在表内：{urls:?}"
        );
        assert!(
            urls.contains(&N0_HOST.to_string()),
            "n0 兜底必须还在（决策 B1）：{urls:?}"
        );
    }

    #[test]
    fn token只挂在自建节点上() {
        let RelayMode::Custom(map) = relay_mode_from("https://relay.example.com/", "s3cret") else {
            panic!();
        };
        let self_url: RelayUrl = "https://relay.example.com/".parse().unwrap();
        let mine = map.get(&self_url).expect("自建条目");
        assert_eq!(mine.auth_token.as_deref(), Some("s3cret"));
        // QAD 走默认端口（7842 = iroh 1.1 DEFAULT_RELAY_QUIC_PORT，n0 生产同口）。
        // 服务端部署文档必须对齐这个口，否则 QAD 静默失效、只剩中继转发。
        assert_eq!(
            mine.quic.as_ref().map(|q| q.port),
            Some(7842),
            "自建节点的 QAD 端口必须是生态默认 7842"
        );
        // n0 各台都不能带我们的 token（会白送防滥用凭据给第三方）
        for url in map.urls::<Vec<_>>() {
            if url == self_url {
                continue;
            }
            let cfg = map.get(&url).expect("表里的 URL 必有配置");
            assert!(cfg.auth_token.is_none(), "n0 节点 {url} 不该带自建 token");
        }
    }

    #[test]
    fn token为空_自建照常进表_只是不带token() {
        let RelayMode::Custom(map) = relay_mode_from("https://relay.example.com/", "") else {
            panic!();
        };
        let url: RelayUrl = "https://relay.example.com/".parse().unwrap();
        assert!(map.get(&url).is_some_and(|c| c.auth_token.is_none()));
    }
}
