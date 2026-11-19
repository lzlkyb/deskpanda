//! 控制帧与流的编解码。**只搬字节，不懂业务。**
//!
//! # 线格式
//!
//! 一个控制帧 = `u32 大端长度 | 内容`。长度上下限都夹住：
//! 空帧直接拒（对端 bug 不该被当成一次合法请求），超过 [`MAX_FRAME_LEN`] 也拒
//! （防「对端发一个巨大帧把本机内存吃满」）。
//!
//! # 🔴 为什么是「停滞超时」而不是「会话总超时」
//!
//! 会话只在**整轮成功之后**才推游标，中途断掉等于零进度、下一轮从头再来。
//! 一旦总超时短于实际需要的时间，那次传输就**永远做不完**——不是变慢，是永久失败。
//! 而按进度算就没有这个矛盾：每块各自计时，
//! [`IO_CHUNK`] / 30 秒 ≈ 最低 17 kbps，任何真实链路都高出几个数量级，
//! 而「保活着每分钟挤一个字节」的慢速攻击会在 30 秒内被砍掉。**对「慢但在动」零误伤。**
//!
//! ❗ 它顺带把「完全死掉的连接」也兜住了，所以不需要依赖 QUIC 自己的 idle timeout。
//!
//! # 🔴 写侧同样会挂死
//!
//! 对端不读的话，流控窗口填满后 `write_all` 会永远阻塞。只包读不包写会漏掉一半的
//! 挂死路径——所以 [`wr`] 与两个读函数都套同一层 [`stalled`]。

use iroh::endpoint::{Connection, RecvStream, SendStream};
use iroh::{Endpoint, EndpointAddr};
use std::time::Duration;

/// 一次读或写**停滞**多久就放弃。
pub const STALL_TIMEOUT: Duration = Duration::from_secs(30);

/// 大文件按块收发，每块各自计时。**读写两侧共用这一个块大小。**
///
/// ❗ 不分块的话，一个 256MiB 的文件就是一次 `read_exact` / `write_all`，
/// 包上超时又变回了「总超时」——慢链路上必超。
const IO_CHUNK: usize = 64 * 1024;

/// 单个控制帧的长度上限。会话开场那个 hello 只有几十字节，
/// 64 KiB 是给未来加字段留的余量，不是给数据用的。
pub const MAX_FRAME_LEN: usize = 64 * 1024;

/// 给一次读/写加停滞超时。超时就中断整个会话（游标不推，下一轮重来）。
async fn stalled<T>(
    what: &str,
    stall: Duration,
    f: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    match tokio::time::timeout(stall, f).await {
        Ok(r) => r,
        Err(_) => Err(format!(
            "{}：{} 秒内一个字节都没动，已中断本次传输",
            what,
            stall.as_secs()
        )),
    }
}

/// 一次会话手里的东西：连接 + 一对流。
///
/// 分成 `dial`/`accept` 两头拿，是因为**会话是双向的**：
/// 一次往返里两边都要发自己的帧、也都要收对端的。
pub struct Wire {
    pub conn: Connection,
    pub send: SendStream,
    pub recv: RecvStream,
}

/// 主动连一个对端，拿到一对流。
///
/// `alpn` 由调用方给：本产品有会话通道与文件通道两条 ALPN，
/// 端点层（[`super::endpoint`]）按 ALPN 分派，这里不写死。
pub async fn dial(ep: &Endpoint, to: EndpointAddr, alpn: &[u8]) -> Result<Wire, String> {
    let conn = ep
        .connect(to, alpn)
        .await
        .map_err(|e| format!("连接对端失败：{}", e))?;
    let (send, recv) = conn
        .open_bi()
        .await
        .map_err(|e| format!("开流失败：{}", e))?;
    Ok(Wire { conn, send, recv })
}

/// 等一个对端连进来（**只到握手为止，不等它开流**）。
///
/// 🔴 开流那一步要单独调 [`accept_streams`]，不能并进来：
/// ALPN 是公开的，**没配对的人也连得上**。只要它连上之后不开双向流，
/// `accept_bi().await` 就会一直挂着——而这个函数是在 accept 循环里调的，
/// 于是**所有其它设备的入连接全被堵死**。拆开之后调用方可以把开流
/// 丢进独立任务并加超时。
pub async fn accept_conn(ep: &Endpoint) -> Result<Connection, String> {
    let incoming = ep.accept().await.ok_or("没有连接进来")?;
    incoming.await.map_err(|e| format!("握手失败：{}", e))
}

/// 在一个已握手的连接上等对端开双向流。**调用方应当给它加超时。**
pub async fn accept_streams(conn: Connection) -> Result<Wire, String> {
    let (send, recv) = conn
        .accept_bi()
        .await
        .map_err(|e| format!("开流失败：{}", e))?;
    Ok(Wire { conn, send, recv })
}

/// 发一个控制帧：`u32 长度 | 内容`。
pub async fn write_frame(s: &mut SendStream, bytes: &[u8]) -> Result<(), String> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME_LEN {
        return Err(format!("控制帧长度不合法（{} 字节）", bytes.len()));
    }
    wr(s, &(bytes.len() as u32).to_be_bytes()).await?;
    wr(s, bytes).await
}

/// 收一个控制帧。
pub async fn read_frame(r: &mut RecvStream) -> Result<Vec<u8>, String> {
    let mut n = [0u8; 4];
    stalled("读帧长度", STALL_TIMEOUT, async {
        r.read_exact(&mut n)
            .await
            .map_err(|e| format!("读帧长度失败：{}", e))
    })
    .await?;
    let n = u32::from_be_bytes(n) as usize;
    if n == 0 || n > MAX_FRAME_LEN {
        return Err(format!("对端报的帧长度不合法（{} 字节）", n));
    }
    let mut buf = vec![0u8; n];
    // 控制帧封顶 `MAX_FRAME_LEN`（= [`IO_CHUNK`]），不用再分块
    stalled("读帧内容", STALL_TIMEOUT, async {
        r.read_exact(&mut buf)
            .await
            .map_err(|e| format!("读帧内容失败：{}", e))
    })
    .await?;
    Ok(buf)
}

/// 写一段字节。**分块 + 每块停滞超时**。错误统一成字符串，省得每处写一遍 `map_err`。
async fn wr(s: &mut SendStream, b: &[u8]) -> Result<(), String> {
    for part in b.chunks(IO_CHUNK) {
        stalled("写流", STALL_TIMEOUT, async {
            s.write_all(part)
                .await
                .map_err(|e| format!("写流失败：{}", e))
        })
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 帧长边界是**纯规则**，不依赖真实流即可钉住上下限。
    ///
    /// ❗ 用 `const { assert!() }` 而不是普通 `assert!`：两边都是常量，
    /// `clippy::assertions_on_constants` 会把「运行期断言常量」判为无意义。
    /// 挪进 const 块后语义更强——**编译不过**，而不是跑测试才发现。
    ///
    /// ❗ 只测规则，不测 I/O：真正的往返由 S5 的双机端到端覆盖。
    #[test]
    fn 帧长上限与常量一致() {
        const { assert!(MAX_FRAME_LEN == 64 * 1024) };
        const {
            assert!(
                MAX_FRAME_LEN >= IO_CHUNK,
                "控制帧必须能一次写完，否则 read_frame 的「不用再分块」前提不成立"
            )
        };
    }
}
