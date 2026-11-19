//! 本机 SQLite 存储：**只存远程桌面自己的东西**（已配对设备、授权开关、配置）。
//!
//! # 与原仓的关系
//!
//! PastePanda 的 `data_store` 是 2326 行、四十多张表的全量库（历史、笔记、知识库、
//! AI 用量……）。本产品只需要三张表，所以这里是**重写的精简版**，
//! 表结构与字段语义照抄原仓（`rc_devices` / `devices` / `config`），
//! 方法实现逐字复制，只有「谁提供 `lock_conn`」这一层是新的。
//!
//! 这样处理的原因是：`rc/` 里 46k 行代码对存储层的调用全是 `store.rc_device_*()`，
//! 语义（包括「先 trim 再截 60 字」这类细节）都在那些方法体里。
//! 重写一遍等于把所有细节重赌一次，不如原样搬。
//!
//! # 🔴 两张设备表为什么分开
//!
//! - `rc_devices`：**远程桌面**的授权（谁能接管我的屏幕、谁能给我推文件）；
//! - `devices`：知识库同步的授权。
//!
//! 原仓里它们是两套独立的信任，一台机器可以只被允许远程、不参与同步。
//! 本产品没有同步，但 `devices` 表仍保留——`rc` 的信任判定会同时查两表
//! （`rc_device_get` OR `device_get`），去掉任一边都会改变「什么算已授权」的语义。
//!
//! # 建表即终态
//!
//! 表结构一次建齐（含原仓后来补的 `last_path` / `os` / `tags` / `remark` 列）。
//! 原仓那几条 `pragma_table_info` 迁移语句对本仓是死代码，故未迁；
//! 若将来要加列，**只能追在末尾**（见 `device.rs` 里 `COLS` 的注释）。

pub mod config;
pub mod device;
mod note;
pub mod rc_device;

pub use device::Device;
pub use rc_device::{DeviceTag, RcDevice};

use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

/// 本机库。**进程内单实例**（用 `Arc<DataStore>` 或 `clone()` 传），连接外面套一把锁。
///
/// ❗ 用同步 `Mutex` 而不是连接池：远程桌面的存储在热路径上只是
/// 「会话开始记一笔、结束记一笔」，并发度极低；连接池会引入
/// 「两个连接同时写 → SQLITE_BUSY」这个新问题，而它比锁竞争难查得多。
///
/// ❗ `Clone` 是**共享同一个连接**（内部 `Arc`），不是复制一份库——
/// 原仓的 `DataStore` 也是这个语义，`rc` 里 `store.clone()` 传进发现循环、
/// 会话任务在多处出现，若语义是「各开一个连接」就会退化成连接池。
#[derive(Clone)]
pub struct DataStore {
    conn: Arc<Mutex<Connection>>,
}

impl DataStore {
    /// 打开（或创建）一个库。`:memory:` 是测试常用的内存库。
    ///
    /// 传文件路径时会先建好父目录——首次启动时 `<appdata>/deskpanda/` 还不存在，
    /// 不建目录 `Connection::open` 会报 "unable to open database file"，
    /// 而这个错误**看不出是目录缺失**。
    pub fn new(db_path: impl AsRef<Path>) -> Result<Self, String> {
        let path = db_path.as_ref();
        if path != Path::new(":memory:") {
            if let Some(dir) = path.parent() {
                if !dir.as_os_str().is_empty() {
                    std::fs::create_dir_all(dir)
                        .map_err(|e| format!("创建数据目录失败 {}：{}", dir.display(), e))?;
                }
            }
        }
        let conn = Connection::open(path).map_err(|e| format!("打开数据库失败：{}", e))?;
        let me = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        me.init_schema()?;
        Ok(me)
    }

    /// 拿连接。锁中毒时用 `into_inner` 继续——一条线程 panic 不该让整个存储永久不可用。
    pub(crate) fn lock_conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn init_schema(&self) -> Result<(), String> {
        let conn = self.lock_conn();
        // 配置：键值对，值按 JSON 往返（见 config.rs）。
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS config (
                 key   TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );",
        )
        .map_err(|e| format!("建 config 表失败：{}", e))?;

        // 同步设备表：本产品不做知识库同步，但信任判定要查它（见模块文档）。
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS devices (
                 node_id        TEXT PRIMARY KEY,
                 name           TEXT NOT NULL,
                 paired_at      TEXT NOT NULL,
                 transport      TEXT NOT NULL DEFAULT '',
                 conn_state     TEXT NOT NULL DEFAULT 'offline',
                 last_seen      INTEGER NOT NULL DEFAULT 0,
                 relay_addr     TEXT NOT NULL DEFAULT '',
                 sync_cursor_ms INTEGER NOT NULL DEFAULT 0,
                 last_ok_ms     INTEGER NOT NULL DEFAULT 0,
                 paused         INTEGER NOT NULL DEFAULT 0
             );",
        )
        .map_err(|e| format!("建 devices 表失败：{}", e))?;

        // 远程设备表。字段语义全在 `rc_device.rs` 的 `RcDevice` 上。
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS rc_devices (
                 node_id     TEXT PRIMARY KEY,
                 name        TEXT NOT NULL,
                 note        TEXT NOT NULL DEFAULT '',
                 paired_at   TEXT NOT NULL,
                 conn_state  TEXT NOT NULL DEFAULT 'offline',
                 last_seen   INTEGER NOT NULL DEFAULT 0,
                 last_path   TEXT NOT NULL DEFAULT '',
                 trusted     INTEGER NOT NULL DEFAULT 0,
                 auto_accept INTEGER NOT NULL DEFAULT 0,
                 os          TEXT NOT NULL DEFAULT '',
                 tags        TEXT NOT NULL DEFAULT '[]',
                 remark      TEXT NOT NULL DEFAULT ''
             );",
        )
        .map_err(|e| format!("建 rc_devices 表失败：{}", e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 内存库建表齐全且可读写() {
        let s = DataStore::new(":memory:").expect("建内存库");
        // 三张表都在（缺任一张，对应的设备/配置操作会在运行期才炸）
        for t in ["config", "devices", "rc_devices"] {
            let n: i64 = s
                .lock_conn()
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [t],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "缺表 {t}");
        }
        s.save_config(&serde_json::json!({"a": 1, "b": "x"}))
            .expect("写配置");
        let c = s.get_config().expect("读配置");
        assert_eq!(c["a"], serde_json::json!(1));
        assert_eq!(c["b"], serde_json::json!("x"));
    }

    #[test]
    fn 内存库不建目录也不报错() {
        // `:memory:` 的 parent 是空串，走 create_dir_all("") 会 ENOENT —— 这条钉住那个分支
        assert!(DataStore::new(":memory:").is_ok());
    }
}
