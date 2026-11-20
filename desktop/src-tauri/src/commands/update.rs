//! 自动更新相关的 IPC 命令（`start_update` / `check_update`）与其私有辅助。
//!
//! 从 `commands.rs` 拆出来的第一块（D19 方案 C 第 1 批）。选它打头阵的理由：
//! 4 个私有辅助（`retry_with_backoff` / `resolve_update_endpoints` /
//! `candidate_endpoint_groups` / `build_updater`）**全部只被本模块调用**，与其它域零耦合，
//! 因此这一批能干净地验证「`pub use` 重导出 + `app.rs` 的 `invoke_handler!` 零改动」
//! 这套机制到底成不成立——机制若不成立，在这里暴露远比在大搬动里暴露好查。
//!
//! 本文件是**纯搬动**：函数体逐字节未改，可用 `tools/fingerprint.py` 比对哈希验证。
//! 唯一新增的是下面这行 import —— 区块内 13 处 `.emit()` 需要 `Emitter` trait 在作用域，
//! 其余外部符号（`url::Url` / `tauri_plugin_updater::` / `serde_json::` 等）原本就是全限定路径。
//! 🔴 Android 例外（2026-11-20）：那批 emit 全在 `cfg(not(target_os = "android"))` 的
//! 桌面分派函数里（`spawn_desktop_update` / `check_desktop`），Android 侧两个命令
//! 直接转调 `update_android`，一个 emit 都没有 ⇒ 不带门控就是 unused import。

#[cfg(not(target_os = "android"))]
use tauri::Emitter;

// ===== 自动更新（后台线程，不阻塞 UI），採自 PastePanda 实现 =====

/// 单飞闸：同一时刻只允许一个更新 / 检查任务在跑。
///
/// 🔴 为什么需要（审查 P2）：`start_update` / `check_update` 是 **spawn 后立即返回**的，
/// 前端却把 `await invoke(...)` 当成「做完了」的信号。于是连点两次徽章 = 两个任务并发，
/// 一边下载一边又被另一个重试覆盖，`update:*` 事件交叉推送，前端状态机直接乱掉。
/// RAII 式持有：任务（含所有提前 return 分支）结束自动释放。
struct SingleFlight(&'static std::sync::atomic::AtomicBool);

impl SingleFlight {
    fn acquire(flag: &'static std::sync::atomic::AtomicBool) -> Option<Self> {
        flag.compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        )
        .ok()
        .map(|_| Self(flag))
    }
}

impl Drop for SingleFlight {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

/// 指数退避重试辅助函数。
///
/// `should_retry` 决定某个错误是否值得再试一次。**不能无条件重试**：更新源返回
/// "没有可用 manifest" 这类确定性结论时，重试 N 次只会让用户多等 N 次退避
/// （2026-10-22 实测：Gitee 源的 releases 分支失效后 404，旧逻辑每次检查白等
/// 1s+2s 共 3.5 秒才回退到 ghproxy，用户感知就是"点更新要转半天"）。
///
/// `pub(crate)`：Android 的 apk 更新链路（`update_android`）复用同一份退避实现，
/// 调用处传 `|_: &String| true` 保持「任何错误都重试」的原语义。
pub(crate) async fn retry_with_backoff<F, Fut, T, E>(
    max_retries: u32,
    operation_name: &str,
    should_retry: impl Fn(&E) -> bool,
    f: F,
) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut attempt = 0u32;
    loop {
        match f().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                if !should_retry(&e) {
                    log::info!("[Update] {operation_name} 遇到不可重试错误，立即换源: {e}");
                    return Err(e);
                }
                attempt += 1;
                if attempt > max_retries {
                    return Err(e);
                }
                let delay_secs = 1u64 << (attempt - 1);
                // +0–511ms 抖动：无抖动时，一次服务端抖动恢复会让所有客户端在
                // 同一刻齐刷刷回源（惊群），刚缓过来的源又被自己打回去。抖动量
                // 取系统时钟纳秒取模而非 rand crate——更新链路不引新依赖
                // （二进制体积是核心卖点），且这里要的只是"错开"，不是密码学随机。
                let jitter_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| (d.subsec_nanos() % 512) as u64);
                log::warn!(
                    "[Update] {} 失败（第 {}/{} 次），{}s+{}ms 后重试: {}",
                    operation_name,
                    attempt,
                    max_retries,
                    delay_secs,
                    jitter_ms,
                    e
                );
                tokio::time::sleep(std::time::Duration::from_millis(
                    delay_secs * 1000 + jitter_ms,
                ))
                .await;
            }
        }
    }
}

// ===== 慢源看门狗（纯逻辑，便于单测） =====
//
// 背景（2026-09-16 实测）：Gitee 发行版附件走 foruda.gitee.com，稳定约 70KB/s；
// ghproxy 同文件约 1.9MB/s。插件的 endpoints 只对 **manifest** failover，对二进制
// 下载不做——manifest 通了就锁死这条源，慢到天荒地老也不换。看门狗负责在下载阶段
// 主动中止过慢的源，让外层 for 循环轮到下一组。
//
// 🔴 与 pastePanda 的口径差异：**桌面路径消费不了看门狗**——桌面下载走插件的
// `download_and_install`，进度回调没有中止钩子（返回 `()`），打断只能靠插件自带的
// 180s 总超时（见 `build_updater`）。看门狗实际由 **Android 的 `download_apk`**
// 消费（reqwest 流式下载，检查点可 `break`）。所以下面这组符号按
// `cfg(any(target_os = "android", test))` 编译：桌面非测试构建里它们是死代码，
// 硬留着只会养一批永远没人调的 warning（clippy 带 `-D warnings`）。

/// 下载均速下限（字节/秒）。低于此值视为慢源。
/// 取 100KB/s：实测 Gitee 附件 47–73KB/s 会触发切换；ghproxy ~1.9MB/s 远超；
/// 家用宽带只要不是极慢线路，正常下载都会远高于此。
#[cfg(any(target_os = "android", test))]
pub const MIN_DOWNLOAD_SPEED_BPS: u64 = 100 * 1024;

/// 宽限期（秒）：连接建立 + TLS + 首包延迟都算在这段里，期间不判慢。
#[cfg(any(target_os = "android", test))]
pub const SPEED_GRACE_SECS: f64 = 8.0;

/// 是否判定当前下载过慢。
///
/// - 宽限期内一律 `false`（避免把「还没开始吐数据」误杀成慢源）。
/// - 过了宽限期后用 `downloaded / max(elapsed, grace)` 算均速，
///   分母至少按宽限期计，避免刚过线时分母过小导致误杀。
#[cfg(any(target_os = "android", test))]
pub fn is_download_too_slow(elapsed_secs: f64, downloaded_bytes: u64) -> bool {
    if elapsed_secs < SPEED_GRACE_SECS {
        return false;
    }
    let elapsed = elapsed_secs.max(SPEED_GRACE_SECS);
    let bps = downloaded_bytes as f64 / elapsed;
    bps < MIN_DOWNLOAD_SPEED_BPS as f64
}

/// manifest 检查的单次请求上限（秒）。
///
/// 取 15s 的依据（2026-10-10 实测）：三条源**健康**时响应 0.5–2.5s，而 ghproxy 被限流时
/// 单次要挂 60s 才吐一个非 2xx。不设上限时「3 条源 ×3 次尝试」最坏要 4 分半，
/// 期间界面只有转圈——用户看到的是「更新失败」，实际是卡在一条死源上重试。
///
/// ❗ 只作用于 **Android** 的 reqwest 客户端（`update_android::http_client`）：
/// 桌面侧插件的 `timeout` 同时管 manifest 检查与二进制下载（tauri-plugin-updater
/// 把 `Update` 的 timeout 从 `Updater` 直传，无法只给一头设），统一值见
/// `build_updater` 的 180s。别把这个值当成「桌面下载超时」去调小——它管不到桌面。
#[cfg(any(target_os = "android", test))]
pub const MANIFEST_CHECK_TIMEOUT_SECS: u64 = 15;

// 编译期钉住取值区间：必须显著小于实测的 60s 挂死（否则徽章转到天荒地老），
// 又要容下 2.5s 的健康响应。放在编译期而不是运行期 assert!——两个操作数都是
// 常量，普通 assert! 会被 clippy::assertions_on_constants 拦（恒真式），且真有
// 人把值改成 0 或 60 时，运行期断言要等测试跑起来才炸；`const _` 让越界值在
// 编译期就炸，所有构建（含 release / 非 test）都过一遍。
// （模块级 `const { }` 块仍是实验特性 E0658，故用 `const _: ()` 这个稳定惯用法。）
#[cfg(any(target_os = "android", test))]
const _: () = assert!(
    MANIFEST_CHECK_TIMEOUT_SECS > 0 && MANIFEST_CHECK_TIMEOUT_SECS < 30,
    "Android manifest 超时要显著小于实测的 60s 挂死，又得容下 2.5s 的健康响应"
);

/// 本次进程内「上一次真正取到 manifest 的源」。
///
/// 存在的理由：**「检查更新」和「下载更新」是两条独立的多源 failover**，后者不记得
/// 前者刚才是从哪条源拿到 manifest 的。2026-10-10 14:24 就是这样出的事——
/// 14:24:05 从 ghproxy 查到了 v7.2.11，用户 14:25 点「更新」，`start_update`
/// 又从第 1 条 Gitee 重头跑，撞上 Gitee 通道 404 + ghproxy 限流 + GitHub 直连被墙，
/// 一个「本来能更新」的会话被重放成了「所有源均失败」。
#[cfg(any(target_os = "android", test))]
static LAST_GOOD_SOURCE: std::sync::OnceLock<std::sync::RwLock<Option<String>>> =
    std::sync::OnceLock::new();

#[cfg(any(target_os = "android", test))]
fn last_good_slot() -> &'static std::sync::RwLock<Option<String>> {
    LAST_GOOD_SOURCE.get_or_init(|| std::sync::RwLock::new(None))
}

/// 记下成功的源。写失败不影响任何逻辑（最坏就是下次仍从配置的第一条试起）。
#[cfg(any(target_os = "android", test))]
pub(crate) fn record_good_source(url: &str) {
    if let Ok(mut slot) = last_good_slot().write() {
        *slot = Some(url.to_string());
    }
}

#[cfg(any(target_os = "android", test))]
pub(crate) fn last_good_source() -> Option<String> {
    last_good_slot().read().ok().and_then(|v| v.clone())
}

/// 把满足 `pred` 的那一项提到最前，返回是否发生了移动（第 0 项命中也算没移动）。
///
/// 桌面侧的 `candidate_endpoint_groups` 不记源（插件路径自己管下载，没有可插入的
/// 轮换点），轮转实际发生在 Android 的 `apk_endpoints`——两条路径共用这一个纯函数，
/// 顺序语义只有一份实现。
#[cfg(any(target_os = "android", test))]
pub fn move_matching_to_front<T>(items: &mut Vec<T>, pred: impl Fn(&T) -> bool) -> bool {
    let Some(idx) = items.iter().position(pred) else {
        return false;
    };
    if idx == 0 {
        return false;
    }
    let item = items.remove(idx);
    items.insert(0, item);
    true
}

/// 源的短名，只用于失败文案——三条完整 URL 塞进 toast 是读不出东西的。
#[cfg(any(target_os = "android", test))]
pub fn source_name(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest.split('/').next().unwrap_or("").to_lowercase();
    if host.contains("gitee.com") {
        "Gitee".to_string()
    } else if host.contains("ghproxy") {
        "ghproxy".to_string()
    } else if host == "github.com" {
        "GitHub".to_string()
    } else {
        host
    }
}

/// 截断到 `max` 个字符（按字符不按字节，避免切进中文中间）。
#[cfg(any(target_os = "android", test))]
fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let kept: String = s.chars().take(max).collect();
    format!("{kept}…")
}

/// 汇总每条源的失败原因。
///
/// 旧写法只带**最后一条**源的错误，而三条源的失败原因往往各不相同
/// （Gitee 是 404、ghproxy 是限流、GitHub 是被墙）。用户报上来的「error sending
/// request for url(...github...)」因此完全指不到真正坏掉的那条通道。
/// 桌面侧的多源文案另走 `update:error` 事件里的「全部更新源均失败（已依次尝试）」，
/// 与本函数是同一种思路的两处实现（桌面的 attempts 带 code，载荷形状不同）。
#[cfg(any(target_os = "android", test))]
pub fn format_source_failures(attempts: &[(String, String)]) -> String {
    if attempts.is_empty() {
        return "没有可用的更新源".to_string();
    }
    let parts: Vec<String> = attempts
        .iter()
        .map(|(url, err)| format!("{}→{}", source_name(url), clip(err, 120)))
        .collect();
    format!("所有更新源均失败: {}", parts.join(" | "))
}

/// 更新错误的机器可读类别。
///
/// `is_retryable_check_error`（要不要重试）与 `classify_update_error`（给前端什么标签）
/// 共用这一套归类——历史上两处各维护一份列表，注释里还得写"注意与本函数保持一致"，
/// 漂移只是时间问题。改成单一真相源：这里加一类，两边同时生效。
enum ErrorClass {
    /// 源内容确定性坏：404/5xx、JSON 反序列化失败、版本号/日期解析失败、缺当前平台条目。
    /// 再请求一次还是一样的坏 manifest，重试唯一的效果是让用户多等一轮退避。
    Manifest,
    /// 端点/协议配置错：URL 不合法、非 https、没有端点、平台不支持、请求头非法。
    Config,
    /// 验签失败：包可能被篡改或下载损坏，必须与「普通下载失败」区分开。
    Signature,
    /// 链路超时。
    Timeout,
    /// 链路抖动：连接重置、TLS 抖动、临时 5xx、代理认证失败。值得再试一次。
    Network,
    /// 包已经下到本地，卡在落盘 / 解开 / 启动安装器。
    Install,
    /// 插件新增的未分类变体：保守视为可重试、标签 unknown（前端回退字符串启发式）。
    Unknown,
}

/// 把插件错误映射到 [`ErrorClass`]。插件 enum 是 `#[non_exhaustive]`，
/// 通配分支保证升级插件版本时编译不破、行为保守。
fn error_class(err: &tauri_plugin_updater::Error) -> ErrorClass {
    use tauri_plugin_updater::Error as UpdaterError;
    match err {
        UpdaterError::ReleaseNotFound
        | UpdaterError::TargetNotFound(_)
        | UpdaterError::TargetsNotFound(_)
        | UpdaterError::Serialization(_)
        | UpdaterError::Semver(_)
        | UpdaterError::FormatDate => ErrorClass::Manifest,
        UpdaterError::EmptyEndpoints
        | UpdaterError::UnsupportedOs
        | UpdaterError::UnsupportedArch
        | UpdaterError::UrlParse(_)
        | UpdaterError::InsecureTransportProtocol
        | UpdaterError::Http(_)
        | UpdaterError::InvalidHeaderValue(_)
        | UpdaterError::InvalidHeaderName(_) => ErrorClass::Config,
        UpdaterError::Minisign(_) | UpdaterError::SignatureUtf8(_) | UpdaterError::Base64(_) => {
            ErrorClass::Signature
        }
        UpdaterError::Reqwest(e) if e.is_timeout() => ErrorClass::Timeout,
        UpdaterError::Reqwest(_) => ErrorClass::Network,
        UpdaterError::Network(_) | UpdaterError::AuthenticationFailed => ErrorClass::Network,
        UpdaterError::Io(_)
        | UpdaterError::PackageInstallFailed
        | UpdaterError::DebInstallFailed
        | UpdaterError::TempDirNotOnSameMountPoint
        | UpdaterError::TempDirNotFound
        | UpdaterError::BinaryNotFoundInArchive
        | UpdaterError::InvalidUpdaterFormat
        | UpdaterError::FailedToDetermineExtractPath
        | UpdaterError::Tauri(_) => ErrorClass::Install,
        // 平台差异：Extract 在插件里是 #[cfg(all(target_os = "windows", feature = "zip"))]，
        // macOS/Linux 上枚举没有这个变体，直接列在共用的 `|` 链里会 E0599 编译不过
        // （2026-11-02 首推 CI 的 macOS job 实测抓到——本地 Windows 全绿照不亮这盏灯）。
        // 本 crate 用插件默认依赖（default features 含 zip），故 Windows 上该变体恒存在，
        // 按 target_os 隔离即可；非 Windows 上落进下方通配臂（Unknown，行为保守）。
        #[cfg(windows)]
        UpdaterError::Extract(_) => ErrorClass::Install,
        _ => ErrorClass::Unknown,
    }
}

/// 判断「检查更新」的错误是否值得重试。
///
/// `Manifest` 与 `Config` 两类都是**确定性**结论 —— 源上压根没有可用 manifest
/// （404 / 空响应 / JSON 坏 / 版本号或日期解析失败 / 缺当前平台条目），或端点配置本身
/// 不合法。再请求一次还是同样的结果，唯一效果是让用户多等一轮退避
/// （2026-10-22 实测：Gitee 源的 releases 分支失效后 404，旧逻辑每次检查白等
/// 1s+2s 共 3.5 秒才回退到 ghproxy，用户感知就是"点更新要转半天"）。
/// 其余错误（连接超时、TLS 抖动、临时 5xx）保留重试。
fn is_retryable_check_error(err: &tauri_plugin_updater::Error) -> bool {
    !matches!(error_class(err), ErrorClass::Manifest | ErrorClass::Config)
}

/// 给更新失败打一个**机器可读**的原因标签，随 `update:error` 一起发给前端。
///
/// 为什么不用字符串判断：前端原先靠 `includes("fetch")` 之类猜原因，而插件的错误文案
/// 是英文常量（`Could not fetch a valid release JSON from the remote`），一句文案改动
/// 就能让整套翻译失效 —— 2026-10-22 实测就是这样：源上 404（根本没有 manifest）被
/// 翻译成「网络连接失败，请检查网络后重试」，而真正的网络错误
/// （`error sending request`）反而落到兜底「更新失败，请重试」。
///
/// 返回的标签由前端 `ERROR_TEXT` 表消费，**不要在日志或 UI 里直接展示**。
/// 标签集合与 `error_class` 一一对应，新增类别只改 `error_class` 一处。
fn classify_update_error(err: &tauri_plugin_updater::Error) -> &'static str {
    match error_class(err) {
        ErrorClass::Manifest => "manifest",
        ErrorClass::Config => "config",
        ErrorClass::Signature => "signature",
        ErrorClass::Timeout => "timeout",
        ErrorClass::Network => "network",
        ErrorClass::Install => "install",
        ErrorClass::Unknown => "unknown",
    }
}

/// 解析更新源端点：优先环境变量 `APP_UPDATE_ENDPOINT`（逗号分隔多 URL，按顺序故障转移），
/// 未设置或解析为空则返回 `None`（退回 `tauri.conf.json` 的 `plugins.updater.endpoints`）。
fn resolve_update_endpoints() -> Result<Option<Vec<url::Url>>, String> {
    let raw = match std::env::var("APP_UPDATE_ENDPOINT") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => return Ok(None),
    };
    let mut eps = Vec::new();
    for part in raw.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        match url::Url::parse(trimmed) {
            Ok(u) => eps.push(u),
            Err(e) => return Err(format!("更新源 \"{trimmed}\" 不是合法 URL: {e}")),
        }
    }
    if eps.is_empty() {
        Ok(None)
    } else {
        Ok(Some(eps))
    }
}

/// 默认 Gitee 镜像仓库（owner/repo）。镜像内容由 CI 每次发版推到该仓库的
/// `releases` 分支 `latest/`（见 .github/workflows/build.yml 的镜像步骤）。
///
/// 🔴 必须是**编译期常量**，不能只靠环境变量：装在用户机器上的应用没有
/// `APP_GITEE_REPOSITORY`。2026-11-20 首次配镜像时发现旧实现只读 `std::env::var`，
/// 结果 CI 把产物推上了 Gitee、客户端永远读不到——"配好了"却完全没生效。
const DEFAULT_GITEE_REPOSITORY: &str = "lzul/deskpanda";

/// Gitee 镜像仓库路径（owner/repo），作为第一候选源。
///
/// 国内访问 Gitee raw 比任何公共 GitHub 代理稳定；这一路检查或下载失败时自动
/// 回退到 tauri.conf.json 配置的端点（见 `candidate_endpoint_groups`）。
///
/// 运行时 `APP_GITEE_REPOSITORY` 仍可覆盖（本地调试、指向私服镜像）；留空串
/// 视为不覆盖，避免 CI 里 secret 缺失时把默认值误删。
fn gitee_repository() -> Option<String> {
    resolve_gitee_repository(std::env::var("APP_GITEE_REPOSITORY").ok())
}

/// 纯判断，抽出来才能在无环境变量的单测里覆盖两条分支（env 是进程级全局态，
/// 测试里改它会让同进程其他用例互相污染）。
fn resolve_gitee_repository(runtime: Option<String>) -> Option<String> {
    match runtime {
        Some(v) if !v.trim().is_empty() => Some(v.trim().to_string()),
        _ => {
            let fallback = DEFAULT_GITEE_REPOSITORY.trim();
            (!fallback.is_empty()).then(|| fallback.to_string())
        }
    }
}

/// 候选更新源分组，按顺序尝试：前一组检查或下载任一步失败，才换下一组。
/// 手动 env 覆盖（APP_UPDATE_ENDPOINT）存在时只用这一组、不做自动换源，保持覆盖语义单一可预期。
fn candidate_endpoint_groups() -> Result<Vec<Option<Vec<url::Url>>>, String> {
    build_candidate_groups(resolve_update_endpoints()?, gitee_repository())
}

/// 纯函数：把「环境变量里读到了什么」与「分组怎么排」分开，顺序才能被单测钉住。
///
/// 顺序是产品行为：**Gitee 镜像在前、tauri.conf.json 的端点在后**。国内用户靠
/// 第一组直连 Gitee（比任何公共 GitHub 代理稳），境外用户或镜像挂掉时第二组兜底。
fn build_candidate_groups(
    override_endpoints: Option<Vec<url::Url>>,
    gitee_repo: Option<String>,
) -> Result<Vec<Option<Vec<url::Url>>>, String> {
    if let Some(eps) = override_endpoints {
        return Ok(vec![Some(eps)]);
    }
    let mut groups = Vec::new();
    if let Some(repo) = gitee_repo {
        let gitee_url = format!("https://gitee.com/{repo}/raw/releases/latest/updater-gitee.json");
        let gitee =
            url::Url::parse(&gitee_url).map_err(|e| format!("Gitee 更新源 URL 无效: {e}"))?;
        groups.push(Some(vec![gitee]));
    }
    groups.push(None); // 退回 tauri.conf.json 配置的端点
    Ok(groups)
}

/// 构造 `Updater`：注入指定的端点（若有），否则用配置端点。
/// 供 `check_update` / `start_update` 共用，确保更新源解析只有这一处实现（单一真相源）。
/// 泛型 `R: Runtime`：让 mock 运行时的集成测试也能驱动作命令（见文件末尾测试）。
///
/// 超时设 180s：插件的 `timeout` 同时作用于 manifest 检查与安装包下载（`Update`
/// 的 timeout 从 `Updater` 直传，无法只给一头设）。manifest 是几 KB 的小文件，
/// 180 秒只为防"服务器收下连接再不响应"的永久挂死（reqwest 默认无总超时，
/// 挂死时徽章会转到天荒地老）；安装包实测 ≤15MB，180s ≈ 83KB/s 的兜底网速，
/// 更慢还有重试 2 次 + 换源兜底。常见场景不受影响：端点 404 走 ReleaseNotFound
/// 秒回（不可重试），端点通畅也是秒回——只有"端点活着但不响应"的罕见死链
/// 才会真的吃满 180s。
fn build_updater<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    endpoints: Option<Vec<url::Url>>,
) -> Result<tauri_plugin_updater::Updater, String> {
    use std::time::Duration;
    use tauri_plugin_updater::UpdaterExt;
    let app_handle = app.clone();
    let mut builder = app_handle.updater_builder();
    if let Some(eps) = endpoints {
        builder = builder
            .endpoints(eps)
            .map_err(|e| format!("更新源配置无效（需 https）: {e}"))?;
    }
    builder
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|e| format!("更新插件初始化失败: {e}"))
}

/// 后台执行更新检查+下载安装，通过 Tauri event 推送状态到前端。
/// 按 `candidate_endpoint_groups` 的顺序依次尝试候选源（默认 Gitee 优先、
/// tauri.conf.json 配置端点兜底），前一个候选检查或下载任一步失败就换下一个，
/// 全部失败才报错。内置指数退避重试：每个候选内检查最多重试 2 次、下载安装最多重试 2 次。
///
/// 🔴 Android 分派（方案甲，2026-10-03）：tauri-plugin-updater 的桌面路径在移动端
/// 不可用（manifest 端点 + exe 安装都是桌面语义），改走 `update_android` 的
/// apk-update.json 直检 + 系统安装器。**事件契约与桌面完全一致**
/// （update:checking / available / downloading / progress / ready / error），
/// 前端与托盘菜单等调用点不分派、不感知。
#[tauri::command]
pub fn start_update<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    #[cfg(target_os = "android")]
    {
        super::update_android::spawn_apk_update(app);
        return;
    }
    #[cfg(not(target_os = "android"))]
    spawn_desktop_update(app);
}

/// 桌面下载路径（Android 在 `start_update` 里已分派走）：单飞闸 + 多源 failover +
/// 指数退避，全部经 `update:*` 事件推状态。
///
/// 抽成独立函数而不是在 `start_update` 里内联包裹：`#[cfg]` 块内的 `return` 之后
/// 若还跟着代码，Android 构建会报 unreachable statement（桌面构建又看不到那块，
/// 两边都不讨好）。分出函数后分派点保持「顶部 return」的形态，与 pastePanda 一致。
#[cfg(not(target_os = "android"))]
fn spawn_desktop_update<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    // 已有任务在跑就直接忽略本次触发（事件由在跑的那个推送，不重复发状态）。
    static RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let Some(guard) = SingleFlight::acquire(&RUNNING) else {
        log::info!("更新任务已在进行，忽略重复触发");
        return;
    };
    tauri::async_runtime::spawn(async move {
        let _guard = guard; // 持有到任务结束（含所有 early return）
        let _ = app.emit("update:checking", ());

        let groups = match candidate_endpoint_groups() {
            Ok(g) => g,
            Err(e) => {
                let _ = app.emit("update:error", serde_json::json!({ "message": e }));
                return;
            }
        };

        let mut last_message: Option<String> = None;
        // 最近一次失败的机器可读原因，随 update:error 一起发（见 classify_update_error）。
        let mut last_code: &'static str = "unknown";
        // 是否至少有一路候选明确回答「已是最新」。与 last_message 合起来决定收尾：
        // 全部报没有更新 ⇒ stale；有真错误 ⇒ 按错误报。
        let mut saw_uptodate = false;
        for endpoints in groups {
            let updater = match build_updater(&app, endpoints) {
                Ok(u) => u,
                Err(e) => {
                    last_message = Some(e);
                    last_code = "config";
                    continue;
                }
            };

            let check_result =
                match retry_with_backoff(2, "检查更新", is_retryable_check_error, || {
                    updater.check()
                })
                .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        last_code = classify_update_error(&e);
                        last_message = Some(format!("检查更新失败: {e}"));
                        continue;
                    }
                };

            let update = match check_result {
                Some(u) => u,
                None => {
                    // 这一路候选报告「没有可用更新」，但**不代表全部候选的结论**：
                    // Gitee 镜像比 GitHub 晚一拍（镜像步骤失败时 GitHub 已发布、Gitee
                    // 还留着上一版），直接 return 会让「托盘点更新」在国内源滞后时
                    // 凭空报错。继续问下一路，全部问完仍没有才在循环外收尾。
                    log::info!("[Update] 候选源报告已是最新，继续问后续源");
                    saw_uptodate = true;
                    continue;
                }
            };

            let date_str = update.date.map(|d| d.to_string());
            let current_ver = update.current_version.clone();
            let _ = app.emit(
                "update:available",
                serde_json::json!({
                    "version": update.version,
                    "body": update.body,
                    "date": date_str,
                    "currentVersion": current_ver,
                }),
            );

            let _ = app.emit("update:downloading", ());

            let app_progress = app.clone();
            // 下载阶段保持「任何错误都重试」的原有语义：这里失败多半是链路抖动，
            // 且重试会从头下载，代价可控；换源由外层 `continue` 负责。
            let result = retry_with_backoff(
                2,
                "下载安装",
                |_: &tauri_plugin_updater::Error| true,
                || {
                    let u = update.clone();
                    let ap = app_progress.clone();
                    async move {
                        let mut downloaded_total: u64 = 0;
                        // 下载速度：窗口限流计算（~250ms 重算一次），不是每个 chunk 都重算——
                        // 快网速下 chunk 回调可能每秒触发好几十次，逐次算瞬时速率会跳得难看。
                        // downloaded/total 仍每 chunk 都发（百分比保持平滑），只有 bytesPerSec 在窗口
                        // 间隔内复用上一次算出的值。
                        let mut window_start = std::time::Instant::now();
                        let mut window_bytes: u64 = 0;
                        let mut last_bytes_per_sec: f64 = 0.0;
                        u.download_and_install(
                            move |chunk_len, total| {
                                downloaded_total += chunk_len as u64;
                                window_bytes += chunk_len as u64;
                                let elapsed = window_start.elapsed();
                                if elapsed.as_millis() >= 250 {
                                    last_bytes_per_sec =
                                        window_bytes as f64 / elapsed.as_secs_f64();
                                    window_start = std::time::Instant::now();
                                    window_bytes = 0;
                                }
                                let _ = ap.emit(
                                    "update:progress",
                                    serde_json::json!({
                                        "downloaded": downloaded_total,
                                        "total": total,
                                        "bytesPerSec": last_bytes_per_sec,
                                    }),
                                );
                            },
                            || {
                                // 🔴 插件这个回调是 on_download_finish（updater.rs:712），触发点在
                                // verify_signature **之前**。绝不能在这里发 update:ready——验签失败
                                // 时前端会误进自动重启倒计时并 relaunch，而更新根本没装上
                                // （2026-10-18 假签名实测：失败后 +9.2s app 被自行重启）。
                                // update:ready 改到 download_and_install 成功返回之后发（见 match）。
                            },
                        )
                        .await
                    }
                },
            )
            .await;

            match result {
                Ok(()) => {
                    // 下载 + 验签 + 安装全部成功才宣告 ready，前端据此起自动重启倒计时。
                    // Windows NSIS 路径走不到这里：install_inner 以 process::exit(0) 收尾，
                    // 重启由安装器负责；macOS/Linux 走到这里，事件才有意义。
                    let _ = app.emit("update:ready", ());
                    return; // 这个源成功，结束
                }
                Err(e) => {
                    last_code = classify_update_error(&e);
                    last_message = Some(format!("下载安装失败: {e}"));
                    continue; // 换下一个候选源重试
                }
            }
        }

        // 全部候选问完仍没拿到可下载的更新。两种收尾必须分开：没有任何真错误 ⇒
        // 每一路都说「已是最新」，沿用原语义发 stale（用户是从「有可用更新」点进来
        // 的，静默清空徽章比报错更糟）；有错误 ⇒ 如实报错，别让「没有更新」掩盖失败。
        if last_message.is_none() && saw_uptodate {
            let _ = app.emit(
                "update:error",
                serde_json::json!({
                    "message": "下载前复查未找到可用更新，可能已发布新版本，请重新检查",
                    "code": "stale"
                }),
            );
            return;
        }

        let _ = app.emit(
            "update:error",
            serde_json::json!({
                "message": format!(
                    "全部更新源均失败（已依次尝试）: {}",
                    last_message.unwrap_or_else(|| "未知错误".into())
                ),
                "code": last_code
            }),
        );
    });
}

/// 只检查更新、不下载，通过 Tauri event 把结果推给前端用于展示徽章。
/// 与 `start_update` 共用 `updater.check()`、指数退避重试与候选源回退顺序，确保检查逻辑
/// 只有这一处实现（单一真相源）。
///
/// 🔴 Android 分派（方案甲，2026-10-03）：tauri-plugin-updater 的桌面路径在移动端
/// 不可用（manifest 端点 + exe 安装都是桌面语义），改走 `update_android` 的
/// apk-update.json 直检。**返回 shape 与桌面事件载荷一致**（version / body /
/// date / currentVersion），前端不分派、不感知。
///
/// 命令签名从「spawn 后立即返回 unit」改成 async 直查并返回值，同时供两端前端
/// `await invoke("check_update")` 消费（桌面 UpdateContext 与手机端 MobileUpdate
/// 都读返回值）；事件推送保持不变，下载流程仍由 start_update 驱动。改之前桌面端
/// invoke 只能拿到 null，前端据此走「已是最新」分支——用户会先看到一句假的
/// 「已是最新版本」，等 available 事件到达才被纠正成「发现新版本」。
#[tauri::command]
pub async fn check_update<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<Option<serde_json::Value>, String> {
    #[cfg(target_os = "android")]
    {
        return super::update_android::check_apk_update(&app).await;
    }
    #[cfg(not(target_os = "android"))]
    {
        check_desktop(app).await
    }
}

/// 桌面检查路径（Android 在 `check_update` 里已分派走）：单飞闸 + 多源 failover +
/// 指数退避。状态经 `update:*` 事件推送（与改动前逐字节一致），另把「有没有新版本」
/// 作为返回值交给调用方——事件管下载流程，返回值管即时问答，两条通道互不替代。
#[cfg(not(target_os = "android"))]
async fn check_desktop<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<Option<serde_json::Value>, String> {
    // 与下载任务**分开**的闸：检查和下载可以并存，但各自不能重入。
    static RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let Some(guard) = SingleFlight::acquire(&RUNNING) else {
        log::info!("更新检查已在进行，忽略重复触发");
        return Ok(None);
    };
    let _guard = guard;
    let _ = app.emit("update:checking", ());

    let groups = match candidate_endpoint_groups() {
        Ok(g) => g,
        Err(e) => {
            let _ = app.emit("update:error", serde_json::json!({ "message": e }));
            return Ok(None);
        }
    };

    let mut last_message: Option<String> = None;
    let mut last_code: &'static str = "unknown";
    let total_groups = groups.len();
    for (idx, endpoints) in groups.into_iter().enumerate() {
        let updater = match build_updater(&app, endpoints) {
            Ok(u) => u,
            Err(e) => {
                last_message = Some(e);
                last_code = "config";
                continue;
            }
        };

        match retry_with_backoff(2, "检查更新", is_retryable_check_error, || {
            updater.check()
        })
        .await
        {
            Ok(Some(u)) => {
                let date_str = u.date.map(|d| d.to_string());
                let current_ver = u.current_version.clone();
                let payload = serde_json::json!({
                    "version": u.version,
                    "body": u.body,
                    "date": date_str,
                    "currentVersion": current_ver,
                });
                let _ = app.emit("update:available", payload.clone());
                return Ok(Some(payload));
            }
            Ok(None) => {
                // None = 这个源明确报告没有更新。**不在这里 return**：
                // 多源场景下（如 Gitee 镜像同步延迟或路径指错），第一个源的
                // None 会把后续源上真正可用的更新静默吞掉——用户看到"已是最新"，
                // 日志里却毫无痕迹。改为记录并继续问后续源；全部问完仍无更新
                // 才在循环外发 uptodate。单源场景（默认）行为不变。
                log::info!("[Update] 候选源 {}/{} 报告已是最新", idx + 1, total_groups);
                continue;
            }
            Err(e) => {
                last_code = classify_update_error(&e);
                last_message = Some(format!("检查更新失败: {e}"));
                continue;
            }
        }
    }

    // 全部候选问完：无任何错误 ⇒ 确实没有更新；有错误 ⇒ 如实报错
    // （None 不是失败，不能拿 None 掩盖真的失败）。
    match last_message {
        None => {
            let _ = app.emit("update:uptodate", ());
        }
        Some(msg) => {
            let _ = app.emit(
                "update:error",
                serde_json::json!({
                    "message": format!("全部更新源均检查失败（已依次尝试）: {msg}"),
                    "code": last_code
                }),
            );
        }
    }
    // 失败也返回 Ok(None)：错误经 update:error 事件告知前端（带 code），命令返回值
    // 只回答「有没有新版本」这一个问题。让 invoke reject 会让前端 catch 分支与事件
    // 监听双双触发，同一个错误提示两次。
    Ok(None)
}

#[cfg(test)]
mod tests {
    // 测试名有意用中文（守卫/回归钉的业务语义直接写在名字里）。
    #![allow(non_snake_case)]
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tauri_plugin_updater::Error as UpdaterError;

    /// 源上找不到 manifest（Gitee releases 分支失效时的真实形态）不该重试：
    /// 重试只会让用户多等一轮 1s+2s 退避。
    #[test]
    fn manifest_missing_is_not_retryable() {
        assert!(!is_retryable_check_error(&UpdaterError::ReleaseNotFound));
        assert!(!is_retryable_check_error(&UpdaterError::TargetNotFound(
            "windows-x86_64".into()
        )));
        assert!(!is_retryable_check_error(&UpdaterError::TargetsNotFound(
            vec!["windows-x86_64".into()]
        )));
    }

    /// manifest 内容确定性坏（JSON 反序列化失败、版本号/日期解析失败）同样不该重试：
    /// 换来的还是同一份坏 manifest。这类曾被误判为"其余错误保留重试"。
    #[test]
    fn deterministic_content_errors_are_not_retryable() {
        assert!(!is_retryable_check_error(&UpdaterError::FormatDate));
        assert!(!is_retryable_check_error(&UpdaterError::Serialization(
            serde_json::from_str::<i32>("not json").unwrap_err()
        )));
        assert!(!is_retryable_check_error(&UpdaterError::Semver(
            semver::Version::parse("not-a-version").unwrap_err()
        )));
    }

    /// 端点配置错（URL 不合法、非 https、没有端点、平台不支持）也不该重试：
    /// 配置不修，重试一万次还是同样的错。
    #[test]
    fn config_errors_are_not_retryable() {
        assert!(!is_retryable_check_error(&UpdaterError::UrlParse(
            url::ParseError::RelativeUrlWithoutBase
        )));
        assert!(!is_retryable_check_error(
            &UpdaterError::InsecureTransportProtocol
        ));
        assert!(!is_retryable_check_error(&UpdaterError::EmptyEndpoints));
        assert!(!is_retryable_check_error(&UpdaterError::UnsupportedOs));
    }

    /// 链路抖动值得再试一次：换源前多试一轮是划算的。
    #[test]
    fn transient_errors_stay_retryable() {
        assert!(is_retryable_check_error(&UpdaterError::Network(
            "connection reset".into()
        )));
        assert!(is_retryable_check_error(
            &UpdaterError::AuthenticationFailed
        ));
        assert!(is_retryable_check_error(&UpdaterError::Io(
            std::io::Error::new(std::io::ErrorKind::ConnectionReset, "reset")
        )));
    }

    /// 源上没有 manifest（404/5xx/缺当前平台条目/内容确定性坏）必须与「网络故障」区分开：
    /// 前者该告诉用户「服务暂时不可用」，后者才是「检查网络」。
    /// 前端按这个标签出文案，所以标签本身不能漂。
    #[test]
    fn classify_separates_manifest_from_network() {
        assert_eq!(
            classify_update_error(&UpdaterError::ReleaseNotFound),
            "manifest"
        );
        assert_eq!(
            classify_update_error(&UpdaterError::TargetNotFound("windows-x86_64".into())),
            "manifest"
        );
        // manifest 内容坏（JSON/版本号/日期）同样归 manifest：源的问题，不是用户的网络问题。
        assert_eq!(classify_update_error(&UpdaterError::FormatDate), "manifest");
        assert_eq!(
            classify_update_error(&UpdaterError::Semver(
                semver::Version::parse("not-a-version").unwrap_err()
            )),
            "manifest"
        );
        assert_eq!(
            classify_update_error(&UpdaterError::Network("connection reset".into())),
            "network"
        );
        assert_eq!(
            classify_update_error(&UpdaterError::EmptyEndpoints),
            "config"
        );
        assert_eq!(
            classify_update_error(&UpdaterError::UnsupportedOs),
            "config"
        );
        // 端点 URL 不合法归 config：修配置，不是重试。
        assert_eq!(
            classify_update_error(&UpdaterError::UrlParse(
                url::ParseError::RelativeUrlWithoutBase
            )),
            "config"
        );
        // 落盘/安装失败与下载失败分开：前者多半是磁盘权限或磁盘空间。
        assert_eq!(
            classify_update_error(&UpdaterError::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "denied"
            ))),
            "install"
        );
    }

    /// 不可重试错误必须**只调用一次**（改动前是 3 次，本测试即失败）。
    #[tokio::test]
    async fn non_retryable_error_is_attempted_once() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let result: Result<(), UpdaterError> =
            retry_with_backoff(2, "检查更新", is_retryable_check_error, || {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Err(UpdaterError::ReleaseNotFound)
                }
            })
            .await;

        assert!(result.is_err());
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "404 类确定性错误不应重试（每次重试都要白等一轮退避）"
        );
    }

    /// 可重试错误保留原有语义：`max_retries=1` ⇒ 首次 + 重试 1 次 = 2 次调用。
    #[tokio::test]
    async fn retryable_error_is_attempted_again() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let result: Result<(), UpdaterError> =
            retry_with_backoff(1, "检查更新", is_retryable_check_error, || {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Err(UpdaterError::Network("connection reset".into()))
                }
            })
            .await;

        assert!(result.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 2, "可重试错误应再试一次");
    }

    /// 🔴 装好的应用没有环境变量：默认镜像必须来自编译期常量，否则「配了镜像
    /// 但客户端读不到」（2026-11-20 首次配 Gitee 时的真实形态）。
    #[test]
    fn default_mirror_applies_without_env() {
        assert_eq!(
            resolve_gitee_repository(None).as_deref(),
            Some("lzul/deskpanda")
        );
        assert_eq!(
            resolve_gitee_repository(None).as_deref(),
            Some(DEFAULT_GITEE_REPOSITORY)
        );
    }

    /// 运行时覆盖优先于默认值：本地调试 / 指向私服镜像时改 env 即可，不用重编。
    #[test]
    fn runtime_env_overrides_default_mirror() {
        assert_eq!(
            resolve_gitee_repository(Some("myorg/mirror".into())).as_deref(),
            Some("myorg/mirror")
        );
        // 前后空格要吃掉，拼 URL 时多一个空格就是另一个（404 的）地址。
        assert_eq!(
            resolve_gitee_repository(Some("  myorg/mirror  ".into())).as_deref(),
            Some("myorg/mirror")
        );
    }

    /// 空串 / 纯空格视为「没设」，回落到默认镜像：CI 里 secret 缺失时会传空串，
    /// 不能因此把默认源删掉（那会让国内用户直接退化成直连 GitHub）。
    #[test]
    fn blank_runtime_env_falls_back_to_default() {
        assert_eq!(
            resolve_gitee_repository(Some(String::new())).as_deref(),
            Some(DEFAULT_GITEE_REPOSITORY)
        );
        assert_eq!(
            resolve_gitee_repository(Some("   ".into())).as_deref(),
            Some(DEFAULT_GITEE_REPOSITORY)
        );
    }

    /// 常量必须是 `owner/repo` 形状：多一段或少了斜杠拼出来的 raw 地址必然 404。
    #[test]
    fn default_mirror_has_owner_repo_shape() {
        let parts: Vec<&str> = DEFAULT_GITEE_REPOSITORY.split('/').collect();
        assert_eq!(
            parts.len(),
            2,
            "应为 owner/repo：{DEFAULT_GITEE_REPOSITORY}"
        );
        assert!(parts.iter().all(|p| !p.trim().is_empty()), "两段都不能为空");
    }

    /// 顺序本身就是产品行为：Gitee 组必须排第一，配置端点兜底排第二。
    /// 写反了境外/镜像故障时先撞 GitHub —— 国内用户等于又回到「直连超时」。
    #[test]
    fn gitee_group_is_first_then_config_fallback() {
        let groups = build_candidate_groups(None, Some("lzul/deskpanda".into())).unwrap();
        assert_eq!(groups.len(), 2);
        let first = groups[0].as_ref().expect("第一组应显式给 Gitee 端点");
        assert_eq!(first.len(), 1);
        assert_eq!(
            first[0].as_str(),
            "https://gitee.com/lzul/deskpanda/raw/releases/latest/updater-gitee.json"
        );
        assert!(
            groups[1].is_none(),
            "第二组必须是 None（= 用 tauri.conf.json 里的端点兜底）"
        );
    }

    /// 没有 Gitee 仓库时只剩兜底组；不能塞一个空组，那会白发一次必然失败的请求。
    #[test]
    fn no_gitee_repo_leaves_only_config_fallback() {
        let groups = build_candidate_groups(None, None).unwrap();
        assert_eq!(groups.len(), 1);
        assert!(groups[0].is_none());
    }

    /// `APP_UPDATE_ENDPOINT` 的覆盖语义不能变：只用这一组、不做自动换源
    /// （本地假源测试 `fake-update-server.mjs` 就靠这条）。
    #[test]
    fn explicit_override_ignores_gitee() {
        let eps = vec![url::Url::parse("http://127.0.0.1:8899/updater.json").unwrap()];
        let groups = build_candidate_groups(Some(eps), Some("lzul/deskpanda".into())).unwrap();
        assert_eq!(groups.len(), 1, "覆盖模式下不应再追加 Gitee 组");
        assert_eq!(
            groups[0].as_ref().unwrap()[0].as_str(),
            "http://127.0.0.1:8899/updater.json"
        );
    }

    // ===== 慢源看门狗（纯逻辑，2026-09-16 实测口径） =====

    #[test]
    fn 宽限期内不判慢_即使零字节() {
        assert!(!is_download_too_slow(0.0, 0));
        assert!(!is_download_too_slow(7.9, 0));
        assert!(!is_download_too_slow(SPEED_GRACE_SECS - 0.01, 0));
    }

    #[test]
    fn 宽限期后零字节算慢() {
        assert!(is_download_too_slow(8.0, 0));
        assert!(is_download_too_slow(30.0, 0));
    }

    #[test]
    fn 实测_Gitee档约70KBps_应判慢() {
        // 12 秒下了约 70KB/s → 840KB；均速 70KB/s < 100KB/s
        let bytes = 70 * 1024 * 12;
        assert!(is_download_too_slow(12.0, bytes));
    }

    #[test]
    fn 实测_ghproxy档约1点9MBps_不应判慢() {
        // 12 秒约 1.9MB/s → 远超阈值
        let bytes = (1.9 * 1024.0 * 1024.0 * 12.0) as u64;
        assert!(!is_download_too_slow(12.0, bytes));
    }

    #[test]
    fn 恰好阈值不算慢() {
        let bytes = MIN_DOWNLOAD_SPEED_BPS * 8;
        assert!(!is_download_too_slow(8.0, bytes));
    }

    #[test]
    fn 刚过宽限期时分母至少按宽限期计_避免误杀() {
        // 8.1s 下了 800KB ≈ 98KB/s，接近阈值；分母 max(8.1, 8)=8.1
        // 若误用极短 elapsed（如把 0.1s 当分母）会把 800KB/0.1s 算成超快而漏判，
        // 这里验证正确语义：仍按真实 elapsed 判慢
        let bytes = 800 * 1024;
        assert!(is_download_too_slow(8.1, bytes));
        // 同样字节若 elapsed 更长更慢，必然判慢
        assert!(is_download_too_slow(10.0, bytes));
    }

    // ===== 源位记忆 / 失败汇总（2026-10-10「三条源全灭」事故的回归用例） =====

    /// 与桌面实际配置同构的源：`tauri.conf.json` 的 endpoints（GitHub 一条）加
    /// 默认 Gitee 镜像，Gitee 组第一、配置端点兜底第二（顺序由
    /// `build_candidate_groups` 钉住）。
    ///
    /// 只用两条而不是 pastePanda 的三条：deskpanda 的 endpoints 只有 GitHub
    /// （ghproxy 在「清掉永不存在的更新端点」那次提交里删了），夹具不虚构
    /// 不存在的源。`source_name` 对 ghproxy 的映射另有一条纯映射测试覆盖。
    fn two_groups() -> Vec<Vec<String>> {
        [
            "https://gitee.com/lzul/deskpanda/raw/releases/latest/updater-gitee.json",
            "https://github.com/lzlkyb/deskpanda/releases/latest/download/updater.json",
        ]
        .iter()
        .map(|u| vec![u.to_string()])
        .collect()
    }

    /// 取某个源码文件里 `fn` 的函数体文本（按行切，LF/CRLF 检出都能用）。
    /// 只测纯函数的话，把**调用点**删掉照样全绿——下面几条守卫钉的就是调用点。
    fn fn_body_of(src: &str, sig_prefix: &str) -> String {
        let lines: Vec<&str> = src.lines().collect();
        let start = lines
            .iter()
            .position(|l| l.trim_start().starts_with(sig_prefix))
            .unwrap_or_else(|| panic!("找不到 {sig_prefix}"));
        let rest = &lines[start + 1..];
        let end = rest
            .iter()
            .position(|l| l.trim_end() == "}")
            .expect("函数结尾的顶格花括号");
        lines[start..start + 1 + end].join("\n")
    }

    /// 本文件内的函数体（守卫用）。
    fn fn_body(sig_prefix: &str) -> String {
        fn_body_of(include_str!("update.rs"), sig_prefix)
    }

    /// `update_android.rs` 内的函数体（Android 分派目标的守卫用）。
    /// 该文件的符号按 `cfg(target_os = "android")` 编译，但 `include_str!` 读的是
    /// 源文本——守卫在桌面测试里照样能钉住 Android 路径的调用点。
    fn fn_body_android(sig_prefix: &str) -> String {
        fn_body_of(include_str!("update_android.rs"), sig_prefix)
    }

    #[test]
    fn 上次成功的源会被提到首位且不多不少() {
        let github = "https://github.com/lzlkyb/deskpanda/releases/latest/download/updater.json";
        let mut groups = two_groups();
        let moved = move_matching_to_front(&mut groups, |g| {
            g.first().map(String::as_str) == Some(github)
        });
        assert!(moved, "第 2 条源命中时必须真的换位");
        assert_eq!(groups.len(), 2, "轮转不许丢源");
        assert!(groups[0][0].starts_with("https://github.com/"));
        // 其余源保持原有相对顺序
        assert!(groups[1][0].contains("gitee.com"));
    }

    #[test]
    fn 命中的源本来就在首位则不算移动() {
        let mut groups = two_groups();
        let moved = move_matching_to_front(&mut groups, |g| g[0].contains("gitee.com"));
        assert!(!moved);
        assert!(groups[0][0].contains("gitee.com"));
    }

    #[test]
    fn 没记到成功源时顺序原样不动() {
        let mut groups = two_groups();
        let snapshot = groups.clone();
        let moved = move_matching_to_front(&mut groups, |_| false);
        assert!(!moved, "没有任何命中却动了顺序 = 配置顺序被静默改写");
        assert_eq!(groups, snapshot);
    }

    /// 上次成功的源要被进程级记住，且后一次覆盖前一次。
    ///
    /// 这条覆盖的不只是「函数能跑」：`record_good_source` / `last_good_source`
    /// 的唯一消费方是 Android 的 `apk_endpoints`（按 target_os 门控），桌面
    /// 无头环境原本**零覆盖**——2026-10-10「三条源全灭」事故的根因就落在这里。
    ///
    /// 🔴 这是**唯一**允许直接碰 `LAST_GOOD_SOURCE` 的用例：那是个进程级
    /// OnceLock，并行测试同时写会互相踩（读到对方写入的值 ⇒ 随机红且无法归因）。
    /// 新增用例请走 `move_matching_to_front` 这类纯函数，别直接摸这个 slot。
    #[test]
    fn 上次成功的源会被记住且后写覆盖先写() {
        let first = "https://gitee.com/lzul/deskpanda/raw/releases/latest/updater-gitee.json";
        let second = "https://github.com/lzlkyb/deskpanda/releases/latest/download/updater.json";
        record_good_source(first);
        assert_eq!(last_good_source().as_deref(), Some(first));
        record_good_source(second);
        assert_eq!(
            last_good_source().as_deref(),
            Some(second),
            "换了源之后必须记住新的那条，否则下载阶段仍按旧源重放"
        );
    }

    #[test]
    fn 失败文案要带上每条源各自的原因() {
        let attempts = vec![
            (
                "https://gitee.com/lzul/deskpanda/raw/releases/latest/updater-gitee.json"
                    .to_string(),
                "Could not fetch a valid release JSON from the remote".to_string(),
            ),
            (
                "https://github.com/lzlkyb/deskpanda/releases/latest/download/updater.json"
                    .to_string(),
                "timeout".to_string(),
            ),
        ];
        let msg = format_source_failures(&attempts);
        // 每条都要出现，且各带自己的原因——只报最后一条时，用户报上来的错误
        // 指不到真正坏掉的那条通道。
        assert!(msg.contains("Gitee→"), "第 1 条源的短名被吞了: {msg}");
        assert!(msg.contains("GitHub→"), "第 2 条源的短名被吞了: {msg}");
        assert!(msg.contains("release JSON"), "第 1 条源的原因被吞了: {msg}");
        assert!(msg.contains("timeout"), "最后一条源的原因丢了: {msg}");
    }

    #[test]
    fn 一条源都没试过不许说所有源失败() {
        assert_eq!(format_source_failures(&[]), "没有可用的更新源");
    }

    #[test]
    fn 源短名按主机映射_未知主机退回域名() {
        assert_eq!(source_name("https://gitee.com/x/raw/a.json"), "Gitee");
        assert_eq!(
            source_name("https://ghproxy.net/https://github.com/x"),
            "ghproxy"
        );
        assert_eq!(source_name("https://github.com/x/y"), "GitHub");
        assert_eq!(
            source_name("https://mirror.example.com/a.json"),
            "mirror.example.com"
        );
    }

    #[test]
    fn 超长原因会截断且不会切进中文中间() {
        let long = "错".repeat(200);
        let clipped = clip(&long, 10);
        assert_eq!(clipped.chars().count(), 11, "10 个字符 + 省略号");
        assert!(clipped.ends_with('…'));
        assert_eq!(clip("短", 10), "短");
    }

    // ===== 静态守卫（调用点删掉不会有人工测试变红，只能钉源码） =====

    /// 更新请求必须带总超时：删掉这行不会有别的测试变红——reqwest 默认无总
    /// 超时，「端点活着但不响应」的死链会让徽章转到天荒地老。
    ///
    /// 与 pastePanda 的口径差异：插件的 timeout 同时作用于 manifest 检查与
    /// 二进制下载（tauri-plugin-updater 把 `Update` 的 timeout 从 `Updater`
    /// 直传，无法只给一头设），桌面侧只有一个 180s；Android 的 manifest 检查
    /// 另有 `MANIFEST_CHECK_TIMEOUT_SECS` 管 reqwest 客户端
    /// （见 `update_android::http_client`）。
    #[test]
    fn build_updater必须带总超时() {
        let body = fn_body("fn build_updater<");
        assert!(
            body.contains(".timeout(Duration::from_secs("),
            "更新请求回到了「无超时」: {body}"
        );
        // MANIFEST_CHECK_TIMEOUT_SECS 的取值区间改由定义处旁的 const 块在
        // **编译期**钉（见该常量上方注释），这里不再重复运行期断言。
    }

    /// 端点顺序要按上次成功的源轮转（2026-10-10「三条源全灭」事故的回归守卫）：
    /// 检查与下载是两条独立 failover，后者必须复用前者刚成功的源，否则一个
    /// 「本来能更新」的会话会被重放成「所有源均失败」。
    ///
    /// 桌面侧 `candidate_endpoint_groups` 不记源（插件路径无可插入的轮换点），
    /// 轮转实际发生在 Android 的 `apk_endpoints`——guard 钉的是那边。
    #[test]
    fn 端点顺序要按上次成功的源轮转() {
        let body = fn_body_android("fn apk_endpoints");
        assert!(
            body.contains("last_good_source()") && body.contains("move_matching_to_front(&mut eps"),
            "apk_endpoints 不再复用上次成功的源：下载阶段会重放整条 failover（2026-10-10 事故）"
        );
    }

    /// 两条检查路径都必须记下成功的源（否则下一轮换源重放）：
    /// Android 的 `check_apk_update` / `spawn_apk_update` 是桌面
    /// `check_update` / `start_update` 的对应物（见本文件顶部的 Android 分派）。
    #[test]
    fn 两条检查路径都必须记下成功的源() {
        for sig in ["pub async fn check_apk_update<", "pub fn spawn_apk_update<"] {
            let body = fn_body_android(sig);
            assert!(
                body.contains("record_good_source("),
                "{sig} 没记下成功的源，下一轮又会从配置第 1 条重放"
            );
        }
    }

    /// apk 路径的失败汇总必须共用 `format_source_failures`，别各写一份——
    /// 多条源的失败原因各不相同（Gitee 404 / ghproxy 限流 / GitHub 被墙），
    /// 只报最后一条源时用户报错信息指不到真正坏的通道。
    ///
    /// 桌面侧的 `update:error` 文案（「全部更新源均失败（已依次尝试）」）是另一种
    /// 载荷形状（带 code），与本函数并存；这里钉两件事：update.rs 里那句只出现
    /// 在汇总函数返回值中一次，且 apk 侧零重复实现。
    #[test]
    fn apk失败文案必须走共享汇总() {
        let production = || {
            include_str!("update.rs")
                .split("#[cfg(test)]")
                .next()
                .unwrap_or("")
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
        };
        assert_eq!(
            production()
                .filter(|l| l.contains("所有更新源均失败"))
                .count(),
            1,
            "这句只允许出现在 format_source_failures 的返回值里一次"
        );
        for sig in ["pub async fn check_apk_update<", "pub fn spawn_apk_update<"] {
            let body = fn_body_android(sig);
            assert!(
                body.contains("format_source_failures(&attempts)"),
                "{sig} 没走汇总，失败原因又会只剩最后一条源"
            );
        }
        let apk_lines = || {
            include_str!("update_android.rs")
                .split("#[cfg(test)]")
                .next()
                .unwrap_or("")
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
        };
        assert_eq!(
            apk_lines()
                .filter(|l| l.contains("所有更新源均失败"))
                .count(),
            0,
            "apk 路径要共用桌面的汇总，别各写一份"
        );
    }
}
