# DeskPanda 拆分方案（定稿）

> 2026-11-19 · 从 PastePanda 拆出「远程电脑」为独立桌面 App
>
> **2026-11-20 迁入本仓**：本文档原稿在 PastePanda 仓，现复制到本仓 `docs/` 并成为
> DeskPanda 侧唯一活文档。代码注释里原指向「PastePanda 仓的
> `docs/DeskPanda-拆分方案-2026-11-19.md`」已全部改指本文件（对账清单见 §3 S6.3）。
> 状态：**S1–S5 已完成**（2026-11-19 迁入 / 2026-11-20 首发 v0.1.1）；
> **S6（macOS 交付）已立项**——本轮先落「保留 mac 构建 + 降级提示」，功能迁入排在 S6。
> **S7（手机端远程迁移）已立项**——执行方案见
> `DeskPanda-手机端远程迁移方案-2026-11-20.md`（同目录），本文只留索引。

## 0. 已定决策（用户拍板，不再讨论）

| # | 决策 | 含义 |
|---|---|---|
| D1 | **方案 B：双仓 fork** | 从 Deskbase 壳复制一份新仓，代码各自演进 |
| D2 | **壳子必须复制，不许改原壳** | `D:\AItool\winapp\desktop-app-shell` 保持干净，继续做模板 |
| D3 | **独立 App，不要求存量互通** | ALPN / 设备身份 / 配对数据不必兼容 PastePanda |
| D4 | **pastePanda 里的远程电脑先不动** | 双轨期，pastePanda 的 `rc/` 代码一行不改 |
| D5 | 命名 **DeskPanda** | crate/npm/目录/repo = `deskpanda`，显示名 `DeskPanda` |
| D6 | **macOS：保留构建、功能后置 S6**（2026-11-20 补） | mac 包继续出（壳子 + 托盘 + 更新全可用），但远程桌面功能只交付 Windows；mac 上打开工作台出「暂不支持」提示，不假装有功能。详见 §3 S6 |
| D7 | **手机端远程代码迁入本仓（S7）**（2026-11-20 补） | pastePanda `src-mobile/` 的远程面（192 文件 ≈17.7k 行）+ Kotlin 插件 9 文件 + Android 构建基建迁入；知识库不迁。执行方案与对账见 `DeskPanda-手机端远程迁移方案-2026-11-20.md` |

## 1. 关键架构修正（相对上一版方案）

**B 方案下不做 crate 化。** 理由：crate 化的收益是「两个 App 共享一份代码」，而 D1+D3 决定了不共享。
强行 crate 化只会让两个独立产品继续被同一份 workspace 绑定，与「独立」相悖。

⇒ **解耦工作全部在新仓内完成**，pastePanda 保持原样（D4）。
⇒ 新仓内部仍按 `media` / `rc` 分目录，为将来复用留口子，但不建 workspace、不拆 crate、不用 submodule。

## 2. 目标形态

```
D:\AItool\winapp\
├─ pastePanda\          主产品（粘贴栈 + 知识库 + 录屏 + 现有远程电脑），本轮零改动
├─ desktop-app-shell\   Deskbase 模板仓，本轮零改动
└─ deskpanda\           ★ 新仓（从 deskbase 复制而来）
   ├─ desktop\          壳（deskbase 结构 + 改名）
   │  ├─ src\           React 前端：壳层 + 搬来的 rc 组件树
   │  └─ src-tauri\     Rust
   └─ docs\
```

新仓 Rust 目录：

```
src-tauri/src/
├─ main.rs / lib.rs        壳（托盘/单实例/窗口/更新）
├─ branding.rs             改名后的品牌常量
├─ commands/               壳命令 + rc 命令（rc.rs / rc_file.rs / rc_pair.rs，共约 2.7k 行）
├─ rc/                     ★ 从 pastePanda 复制，45,952 行 / 125 文件，按原模块结构保留
├─ platform/               ★ 新写：替代 pastePanda 的宿主能力（屏幕/显示器/光标/设备名）
├─ net/                    ★ 新写：endpoint + 局域网发现 + 帧写（承载搬来的 shared_ep/presence）
└─ store/                  ★ 新写：SQLite（rusqlite），只建 rc 需要的表
```

（以上目录已按此落地。`net/` 的模块与原 pastePanda `sync::` 的对应关系见
`src-tauri/src/net/mod.rs` 头部注释；`platform/screen.rs` 目前只有 Windows 实现，
mac 对应物排在 S6。）

## 3. 阶段计划

### S1｜复制壳子并改名 ✅ 已完成（2026-11-19）

```bash
cd /d/AItool/winapp
robocopy desktop-app-shell deskpanda /E /XD .git node_modules dist target /XF *.log
cd deskpanda/desktop
node scripts/rename-app.mjs deskpanda --repo lzlkyb/deskpanda \
  --identifier com.pastepanda.deskpanda --display "DeskPanda"
```

手动补齐（脚本不管）：图标、更新签名密钥（`npx tauri signer generate`，公钥进
`tauri.conf.json`、私钥进 GitHub secret）、updater endpoints 指向 `lzlkyb/deskpanda`、
新仓 `git init` + 首次提交——**四项均已完成**。

**验收**：已过（`npm run dev` 起得来；标题/托盘 tooltip 均为 DeskPanda；
identifier 为 `com.pastepanda.deskpanda`）。

### S2｜复制 rc 代码并编译通过 ✅ 已完成（2026-11-19）

- 复制 `src-tauri/src/rc/` → 新仓同名目录（保留子模块结构，不重组）
- 复制 `src-tauri/src/commands/{rc.rs,rc_file.rs,rc_pair.rs}`
- 复制前端：`src/components/rc/**`、`src/lib/api/rc*`、`src/stores/rc*`、`src/hooks/useRc*`、`src/lib/rc*.ts`
- 补 `Cargo.toml` 依赖：`iroh` + `noq-proto/protocol`（拥塞控制）+ `windows`（DXGI/MediaFoundation/Input/Graphics/Gdi/Com）+ `rusqlite`(bundled) + `ring` + `argon2` + `sha2` + `image` + `tokio` + `jpeg` 相关
- 补 `tauri.conf.json` 的 `resources/ffmpeg` 打包配置

**实际偏差**：pastePanda 的 macOS 三件套（`rc/mod.rs` 的 `#[path="../macos/remote_*.rs"]`
三条声明与 `rc/audio.rs` 的 `mac_capture`）连同 `src/macos/` 桥接整体未迁，声明已删
（提交 `2b9d962`）。后果是 `rc` 整模块按 `cfg(windows)` 门控，mac 构建只出壳子——
补课范围与做法见 S6。

**验收**：已过（`cargo check` 通过；rc 单测随迁，数量与 pastePanda 侧对得上）。

### S3｜解耦 12 处宿主依赖 ✅ 已完成（2026-11-19，2026-11-20 补漏）

pastePanda 的 rc 直接引用了主 App 的 12 类基础设施，这是唯一的硬耦合，全部在新仓内自建
（下表为迁移当时的对账基准，落点列补记实际结果）：

| # | 现依赖 | 引用量 | 新仓改法（落点） |
|---|---|---|---|
| 1 | `data_store::DataStore` | 8 | 新写 `store::RcStore`（rusqlite），只建 `rc_devices` / `rc_settings` 两表，schema 沿用原定义 |
| 2 | `sync::transport::write_frame` | 12 | 纯函数，复制进 `net/transport.rs` |
| 3 | `sync::path_kind::{PathKind, register_underlay, of_conn}` | 8 | 复制进 `net/path_kind.rs`（纯逻辑 + iroh 连接属性） |
| 4 | `sync::presence::{Nearby, ...}` | 2 + `discovery.rs` 整体 | 复制 `presence` 模块，组播组/ALPN 改成 deskpanda 自己的（避免与主 App 互刷） |
| 5 | `sync::identity::NodeIdentity` | 2 | 复制，密钥落盘路径改到新 App 数据目录 |
| 6 | `shared_ep::{get_or_bind, register, unregister, existing}` | 10 | 复制 `shared_ep.rs`（216 行）→ `net/endpoint.rs`，新 App 自持端点，ALPN 用 `deskpanda/rc/1` |
| 7 | `screenshot::{MonitorInfo, monitor_region, list_monitors, get_cursor_pos, capture_*}` | 13 | 新写 `platform/screen.rs`（Win32 `EnumDisplayMonitors` / DXGI / macOS 分支） |
| 8 | `macos::{screen, av1, av1_receive}` | 6 | 跟随搬（`cfg(target_os="macos")`），是否同期支持见 §6 → **已决断：S6** |
| 9 | `tray_manager::get_monitor_work_area` | 1 | `platform/screen.rs` 内用 `MonitorFromPoint` + `GetMonitorInfoW` |
| 10 | `paste_engine::TOOL_WINDOW_LABELS` | 1 | 改成 `RcConfig::own_window_labels: Vec<String>`，由 App 初始化时注入 |
| 11 | `user_paths::home_dir` | 1 | `dirs::home_dir()` |
| 12 | `device_identity::local_device_name` | 1 | 新 App 自己生成（主机名 + 短后缀），不读主 App 配置 |

**2026-11-20 补漏**（提交 `d586885` + CHANGELOG Unreleased）：三处命令迁移时整个漏掉
（`open_file_location` / `rc_net_diag` / 设置页三行），前端 invoke 报「命令不存在」；
注册穷举守卫因此补了**反向**断言（非 Windows 段有的壳子命令，Windows 段也必须注册）。

**验收**：grep 零命中已过、`cargo test --lib` 全绿。⚠️ 该 grep 目前只在 Windows
口径下成立——`rc` 被 `cfg(windows)` 门控，mac 路径根本没编。S6 解门控后必须重跑
（见 S6.3 对账表第 2 项）。

### S4｜前端换壳 ✅ 已完成（2026-11-19/20）

跨域依赖实测只有 4 个，迁移成本比预想小：

| 依赖 | 引用量 | 处理 |
|---|---|---|
| `@/components/Toast` | 20 | 换成 deskbase 的 toast |
| `@/lib/fingerprint` | 18 | 复制过来（纯前端指纹） |
| `@/lib/utils` | 12 | 复制用到的几个函数 |
| `@/lib/confirm` | 5 | 换成 deskbase 的确认弹窗 |

其余 `@/hooks/useRc*`、`@/components/rc/*` 自属，整棵迁。

壳层工作（入口改根级页面 + 侧栏 / 套 deskbase layout 壳 / 托盘菜单接壳子的 /
HUD 与胶囊窗改新仓窗口体系 / 第一版保留 rc 自己的 CSS Modules）**均已完成**；
两条判据随关窗权限修复落到代码：

- ⚠️ 判据 30：非主线程 `WebviewWindowBuilder::build()` 返回 ≠ 建好，`recv_timeout` 超时只能算「未确认」
- ⚠️ 判据 19：注册 `onCloseRequested` 后需 `core:window:allow-destroy`，缺了是静默白屏

**验收**：已过（`tsc --noEmit` + eslint + 搬来的前端 rc 测试全绿；`npm run lint:ui` 按新仓规则执行）。

### S5｜端到端 + 打包 ✅ 已完成（2026-11-20 实测）

- 两台机器装同一构建，跑完：发现 → 配对 → 远程控制 → 文件传输 → 剪贴板 → 系统声音
- 无 `ffmpeg` 资源时降级路径可用（软编）
- 记录安装包实测体积，据此定新体积红线（壳子原红线 15MB 必然要改）

**验收**：已过。清单见 `docs/S5-双机验收清单.md`（Windows 双机全过；手机热点中继
一轮可后补）。实测记录：安装包 8.7 MB / 安装后约 33 MB / 冷启动约 1.0 s ⇒
体积红线定为**安装包 ≤ 15 MB、安装后 ≤ 45 MB**（清单 §7）。

### S6｜macOS 交付（2026-11-20 立项；壳已落，功能待迁）

#### 6.1 本轮已决断、已落地（2026-11-20）

按 D6：**保留 mac 构建**（CI `check-macos` / `build-macos` 继续出包），远程功能
只交付 Windows，mac 上出诚实降级提示而不是假装有功能：

- `lib.rs`：`rc` / `net` / `platform` 三模块按 `cfg(windows)` 门控，mac 构建只含
  壳子（托盘 / 单实例 / 窗口 / 更新 / `get_app_info`——后者两段均已注册，是前端
  判断平台的唯一数据源）。
- 前端：`usePlatform`（模块级单飞读 `get_app_info`，进程内只发一次）→
  `rcPlatformDegraded` 判据（`platform !== undefined && platform !== "windows"`；
  **undefined 不降级**，防 Windows 首帧闪提示）→ 降级态四处接线（主区通知卡 /
  侧栏整体退场 / 错误槽抑制 / 标题栏中格静态徽章），另停 `useRcAutoCheck` 轮询与
  自动开通道（该 pref 缺省 true，否则 mac 白弹失败 toast）。
- 入口守卫：rc 命令只在 `main.rs` 的 Windows 段 `invoke_handler` 注册，mac 上
  invoke 一律失败——降级态下不发这些命令；`useRc` 本身无 toast（失败走 `rc.error`，
  而错误槽在降级态被抑制），所以 mac 上不会有「命令不存在」的弹窗。

#### 6.2 迁入范围（Rust 侧，从 pastePanda `src-tauri/src/macos/` 41 个文件里取子集）

rc 真正依赖的（以 `#[path]` 声明与 `crate::macos::` 引用实查，2026-11-20）：

| 迁入物 | 被谁用 | 说明 |
|---|---|---|
| `remote_capture.rs` + `remote_capture.m` | `rc::mac_capture` | 屏幕采集（ScreenCaptureKit），依赖 `macos::screen` / `screen_layout` |
| `remote_video.rs` + `remote_video.m`(+`.h`) | `rc::mac_video` | AV1 编码，依赖 `macos::av1` |
| `remote_input.rs` + `remote_input.m` / `remote_input_lock.m` | `rc::mac_input` | 键鼠注入，依赖 `macos::screen` |
| `remote_audio.rs` + `remote_audio.m` / `remote_audio_encoder.m`(+`.h`) | `rc::audio::mac_capture` | 系统声音采集 |
| `av1.rs` + `av1_bridge.c/.h` + `av1_decode_bridge.c` | `remote_video` / `rc::service` 收端 | AV1 编解码桥接 |
| `screen.rs` + `screen.m`、`screen_layout.rs` | 上面全部 + `rc::inbound_tasks` / `rc::input` | 显示器枚举与坐标 |
| `av1_receive.rs` | `rc::service::mod` 的 `av1_receiver` 字段 | 收端解码（**消费侧代码已在本仓原位**） |
| `keep_awake.m` | `rc::keep_awake` 的 `pp_mac_awake_start/stop` extern | **Rust 侧已在本仓原位**（`#[cfg(target_os="macos")]` 段），只缺 `.m` 符号 |

**不需要迁**（同目录其余文件）：录屏 / GIF / 裁剪 / 贴边 / 文件关联 / 启动项 /
剪贴板 / 滚动等是 pastePanda 主产品能力，与 rc 无关（与 §7 同旨）。

本仓侧要动的——全部是「解门控 + 补声明」，**不是重写**：消费侧 mac 分支在迁移时
原样保留（`rc/inbound_tasks.rs`、`rc/input.rs`、`rc/service/mod.rs` 里的
`crate::macos::*` / `mac_video` 引用，`rc/keep_awake.rs` 的 extern），只因 `rc`
整模块门控而不可达：

1. `lib.rs`：`rc` / `net` / `platform` 门控改 `cfg(any(windows, target_os = "macos"))`。
   `net/` 内部无 Windows-only 代码（仅 `identity.rs` 一条 mac 分支）预计可直接解；
   `platform/screen.rs` 是纯 Win32，需先补 mac 实现。
2. `platform/screen.rs`：补 mac 实现（接口对齐 Win32 版：枚举 / 矩形 / 光标 /
   工作区，可从 pastePanda `macos/screen.rs` 移植）。
3. `rc/mod.rs`：补三条 `#[cfg(target_os="macos")] #[path="../macos/remote_*.rs"]`
   声明；`rc/audio.rs` 补 `mac_capture` 声明与 `pub use`。
4. `commands/rc.rs`：`rc_encode_caps` 补 mac 分支（pastePanda 原样：
   `mac_video::hardware_available` / `scope_support` + `macos::screen::monitors`）。
5. `build.rs`：补 mac 段（cc 编译 `.m` / `.c` + 链 frameworks，以 pastePanda
   `build.rs` 的 mac 段为基准）。
6. `commands/system.rs`：`open_file_location` 的 mac reveal 桥接（pastePanda
   `macos/open.rs`）——当前是诚实报错，可同批补齐。

#### 6.3 迁移对账（防「改一处漏一处」）

沿用 S3 的 grep 纪律，另加四条 mac 专项。`commands/system.rs` 的
「前端 invoke 名必须与 `generate_handler!` 双向对齐」判据即出自本表第 1 项。

| # | 对账项 | 判据 |
|---|---|---|
| 1 | 前端 invoke 名 ↔ `generate_handler!` 双向对齐 | 现有注册穷举守卫在 mac 口径重跑；`open_file_location` mac 分支不得再报「命令不存在」 |
| 2 | `crate::(sync\|data_store\|screenshot\|macos\|lan_pair\|tray_manager\|paste_engine\|user_paths\|device_identity\|shared_ep)` 在 `rc/` 与 `commands/` 下零命中 | S3 原判据在 mac 路径重跑（此前 mac 路径没编，等于没查） |
| 3 | 消费侧原位 mac 分支全部有着落 | 6.2 列出的现成引用（`crate::macos::*` / `mac_video` / `pp_mac_awake_*`）解门控后必须编译通过——它们是「迁入完成度」的天然探针 |
| 4 | 平台分支那一半靠什么验 | CLAUDE.md §8.1 第 6 条：`check-macos` / `build-macos` 从此真实编译 rc（此前 rc 被 cfg 挡在 mac 编译外，mac CI 全绿但没照到 rc 一行）；tag 时 mac 产物带远程功能 |

#### 6.4 验收判据

1. `cargo check --target aarch64-apple-darwin`（CI `check-macos`）下 `rc` 模块真实
   编入：`lib.rs` 门控已解、三条 `#[path]` 有着落、无 `crate::macos` 悬空引用。
2. mac 上打开工作台**不再出现**降级提示（`usePlatform` 读到 `"macos"` ⇒
   `rcPlatformDegraded` 为 false）。通知卡组件与其测试**不删**——判据是「平台不
   支持」而非「永不出现」，S6 之后它守护将来新增平台（如 Linux）的默认诚实。
3. `docs/S5-双机验收清单.md` 补 macOS 行（至少：mac 被控 + Win 主控的发现 /
   配对 / 看屏 / 声音 / 键鼠 / 文件；纯 mac 双机可选）。
4. 安装包容积重测并记入清单 §7（AV1 + ScreenCaptureKit 的增量）。
5. S3 grep 零命中 + 注册穷举守卫在 mac 口径全过。
6. 降级提示的 19 条前端测试保持全绿（`rcPlatform` 5 + `usePlatform` 3 +
   `RcUnsupportedPlatform` 4 + `RcA2TitleBar` 2 + `RcWorkbench` 5；Windows 与
   未知平台仍靠它们）。

## 4. 门禁与规范

- 新仓沿用 deskbase 的 7 项 push 前门禁（组件体量/fmt/eslint/tsc/vitest/clippy/cargo test）
- 版本号真源 = `desktop/src-tauri/Cargo.toml`（deskbase 规则），与 pastePanda 无关
- 组件体量红线 300 行沿用；搬来的文件里已知贴顶的：`RcA2PairExchange`(300)、
  `RcWorkbench`(298，S6 降级接线后仍合规)、`RcSessionCapsule`(284)
- 改 UI 前出 `design/` HTML 稿（deskbase 规则 4）

## 5. 风险与对策

| 风险 | 对策 |
|---|---|
| 同机双 App 共存：局域网 hello 互刷「附近的设备」 | `presence` 用独立组播地址 + 独立 ALPN |
| 同机双 App：单实例锁、托盘图标、剪贴板竞争 | 新 App 用不同 identifier；不注册全局剪贴板监控，只按需读写 |
| 体积暴涨（iroh + ffmpeg + Win32 图形） | S5 实测后定红线，不套壳子 15MB（已定：安装包 ≤ 15MB / 安装后 ≤ 45MB） |
| 前端测试基建不同（判据 27：桩别装进共享 test-setup） | 桩与驱动 API 收口在消费方测试文件 |
| 搬代码时「改一处漏一处」 | S3 用 grep 零命中做验收，不靠自证 |
| `clippy` 结果缓存骗人（判据 1） | 真值先 `touch src/lib.rs`，再 `-- -D warnings` |

## 6. 待定项

- **macOS**：~~rc 有 `mac_capture` / AV1 的 macOS 分支。方案按「S2 一起搬、S3 补平台实现」处理；是否本轮交付 macOS 包，等 S2 结束评估工作量再定。~~
  **已决断（2026-11-20，D6）**：保留 mac 构建、远程功能后置 S6、降级提示先行落地
  （S6.1）。迁入范围与验收见 S6.2–S6.4。
- **中文名**：显示名统一用 `DeskPanda`，界面文案是否需要中文别名（如「熊猫桌面」）未定。

## 7. 明确不做

- 不改 pastePanda 任何代码（含不改其 `rc/`、不删其入口）
- 不做跨 App 互通、不做配对/身份数据迁移
- 不建 workspace、不拆 crate、不用 submodule
- 第一版不做 UI token 统一、不重写 rc 的业务逻辑
- S6 不迁录屏类 `.m`（`recording` / `rec_trim` / `rec_gif` / `screenshot_legacy` 等）：
  那些是 pastePanda 主产品能力，deskpanda 只做远程桌面
