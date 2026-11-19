//! 键值配置表（`config(key, value)`）。
//!
//! 值的存法是「能解析成 JSON 就按 JSON，否则按字符串」——于是
//! `true` / `12` / `"文本"` 都能原样往返，前端读到的类型与写进去的一致。
//!
//! ❗ 原仓这里还带「配置备份轮换」（每分钟至多一次，写 `config_backups/`）。
//!   本仓未迁：远程桌面的配置面很小（设备表为主），而备份轮换的复杂度
//!   （限频锁 + 文件轮换）与收益不匹配。要加时按原仓 `config.rs` 的做法补。

use super::DataStore;
use rusqlite::params;

impl DataStore {
    /// 读出全部配置，扁平成一个 JSON 对象。
    pub fn get_config(&self) -> Result<serde_json::Value, String> {
        let conn = self.lock_conn();
        let mut stmt = conn
            .prepare("SELECT key, value FROM config")
            .map_err(|e| e.to_string())?;

        let mut map = serde_json::Map::new();
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;

        for (key, value) in rows.flatten() {
            let json_val = serde_json::from_str(&value).unwrap_or(serde_json::Value::String(value));
            map.insert(key, json_val);
        }

        Ok(serde_json::Value::Object(map))
    }

    /// 写入配置。**事务包裹全部键**，保证 all-or-nothing——
    /// 中途失败时不能留下「一半新一半旧」的配置（旧值里有设备授权开关）。
    ///
    /// ❗ 只 UPSERT 传进来的键，**不删**未传的键：调用方可能是局部更新，
    ///   全表覆盖会把别人刚写的设置抹掉。
    pub fn save_config(&self, config: &serde_json::Value) -> Result<(), String> {
        let mut conn = self.lock_conn();
        if let serde_json::Value::Object(map) = config {
            let tx = conn.transaction().map_err(|e| e.to_string())?;
            for (key, value) in map {
                let value_str = match value {
                    serde_json::Value::String(s) => s.clone(),
                    _ => serde_json::to_string(value).unwrap_or_default(),
                };
                tx.execute(
                    "INSERT OR REPLACE INTO config (key, value) VALUES (?1, ?2)",
                    params![key, value_str],
                )
                .map_err(|e| e.to_string())?;
            }
            tx.commit().map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
