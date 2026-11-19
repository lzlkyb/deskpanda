//! 时间戳口径：全库统一走这里，别在各处写 `format!`。
//!
//! # 🔴 为什么带毫秒（`%.3f`）
//!
//! 设备表按 `paired_at` / 最近使用时间排序。秒级时间戳在「连续配对两台」时
//! 会撞在同一秒，于是列表顺序取决于 `rowid`，而 `rowid` 又受 SQLite 复用策略影响
//! ⇒ **同一批数据两次查询顺序不同**，分页会重复/跌页。
//!
//! 毫秒仍需配合 `rowid DESC` 兜底（见 `rc_device_list` 的 `ORDER BY`），
//! 但撞毫秒的概率比撞秒低三个数量级。

pub(super) const NOTE_TIME_FMT: &str = "%Y-%m-%d %H:%M:%S%.3f";

/// 本地时间字串。用 `Local` 而不是 `Utc`：这是给人看的「配对时间」，
/// 跨机比较一律走 epoch 毫秒列，不用这个字串。
pub(super) fn note_now() -> String {
    chrono::Local::now().format(NOTE_TIME_FMT).to_string()
}
