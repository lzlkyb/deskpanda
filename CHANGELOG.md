# 变更日志

本文件是**唯一手写源**（Keep a Changelog 格式）。`scripts/gen-changelog.mjs` 在
predev/prebuild 时把它解析成 `desktop/src/lib/changelog.generated.ts`（关于页数据源），
`scripts/update-release-notes.mjs` 在 CI 发版时把用户可见小节抽进 `updater.json` 的
`notes` 字段（更新弹窗内容）。**改这里，不要改生成物。**

版本段结构（CI 与脚本都依赖）：段头为一行内代码 `## [X.Y.Z] - YYYY-MM-DD`，
其下小节如下（注意：本文件内的示例请照此用行内代码书写，段头若独占一行会被
gen-changelog.mjs 当成真实版本段解析——该脚本不跳过代码围栏）：

```markdown
（段头行：## [X.Y.Z] - YYYY-MM-DD）

### 更新摘要
一句话人话，会渲染成更新弹窗顶部高亮卡片（禁止代码符号/根因分析）。

### 新增
- 用户能看懂的点

### 修复
- 用户能看懂的点

### 技术细节
- 不进更新弹窗的小节名之一，仅供本文件与关于页
```

小节映射：新增→feat / 变更→improve / 修复→fix / 安全→sec；`### 技术细节` /
`### 实现说明` / `### 开发者` 小节**不进**更新弹窗。

## [Unreleased]

### 新增
- macOS 上安装 DeskPanda 也能正常用了：托盘常驻、检查更新、自动更新都可用；打开远程电脑会看到「暂不支持 macOS」的明确说明（当前平台、本机还能做什么），而不是点了没反应

### 变更
- 自动更新在国内改走 Gitee 镜像：检查更新与下载安装包不再只能依赖 GitHub 和第三方代理
- 远程电脑功能当前只在 Windows 电脑上提供；macOS 上的采集 / 编码 / 音频 / 键鼠注入依赖一整套尚未迁入的平台桥接，已正式排入后续阶段（拆分方案 S6），期间 mac 构建保留、只出壳子与提示

### 修复
- 修复工作台设置页缺项：画质档、画面范围、会话防休眠此前只有后台能力、没有界面入口（拆分迁移时丢失），现已补进设置页——「画面」分区两行 + 「通用」里的会话防休眠行
- 修复设置页底部「更多远程设置在主窗口」点了没反应：本应用主窗口就是工作台，那个跳转本来就没有接收方，已删除
- 修复远程文件面板「打开所在文件夹」与「打开接收目录」报「命令不存在」：后端命令迁移时整个漏掉了，已补上
- 修复主界面网络诊断卡片一进应用就报「命令不存在」：诊断命令迁移时漏注册，已补上
- 修复关不掉远程电脑工作台窗口：注册关闭拦截后缺窗口销毁权限，点 ✕ 白屏、窗口不动，已补上权限

### 技术细节
- 修复 d586885 引入的 macOS 编译失败：`commands/net_diag.rs` 无条件 `use crate::net`，而 `net` 模块整体 `#[cfg(windows)]` 门控 ⇒ mac 侧 clippy/test 编不过（CI「Rust check + test (macOS)」红灯）。修法是命令保持双段注册（mac 诊断卡片要调，漏注册就是「命令不存在」），实现按平台分派：非 Windows 返回 `snapshot: null` +「远程电脑功能仅支持 Windows」，返回结构的 snapshot 字段类型用 `#[cfg]` 各自声明，前端 `NetDiagSnapshot | null` 契约不变。
- Gitee 镜像仓库路径从「运行时环境变量」改为**编译期常量** `DEFAULT_GITEE_REPOSITORY`：装在用户机器上的应用没有 `APP_GITEE_REPOSITORY`，旧实现只读 `std::env::var`，结果是 CI 把产物推上了 Gitee、客户端永远读不到（2026-11-20 配镜像时发现）。运行时 env 仍可覆盖（本地调试 / 私服）。
- `start_update` 里「本候选报告已是最新」从 `return` 改成 `continue`：Gitee 镜像比 GitHub 晚一拍同步时，托盘点更新会让第一路判成「没有更新」并直接报错；现在继续问下一路，全部问完仍没有才发 `stale`（原有语义不变，只是不再被第一路提前终止）。
- 设置项补覆盖的前端接线落成源码守卫 `desktop/src/__tests__/RcSettingsWiring.test.ts`：三个 `rc_set_*` 从 API 封装 → store action → `useRc` 投影 → 组件消费 → 设置页挂载**五环逐环**钉住，另钉死链接事件名 `pp:open-settings-rc` 的字符串字面量不复辟、`onOpenSettings` prop 不回来。四环注入破坏均精确变红后还原。
- 设置页被控行的禁用判据收口 `desktop/src/lib/rcRowGate.ts`：三档真话——加载中（「正在读取远程状态…」）≠ 已关闭（「远程协助已关闭」）≠ 切换在飞（不给理由，不拿「已关闭」糊弄）。防的是「界面说已开而保活锁根本不会拿」这类假话。
- 注册穷举守卫补**反向**断言：非 Windows 段里出现的壳子命令，Windows 段也必须注册。`rc_net_diag` 与 `open_file_location` 这次是两段全漏，原有守卫只查「Windows 段有的非 Windows 段必须有」这一个方向，查不到。
- `open_file_location` 自带 UNC / 网络共享路径拦截（`is_unsafe_network_path`）：远程文件面板展示的路径来自对端、可能是被投毒的内容，Windows 上对这类路径做 `exists()` / `explorer` 会**自动发起 SMB 认证**、泄漏用户 NTLMv2 凭据哈希。macOS 的 reveal 桥接未迁，诚实报错不静默。
- 设计稿 `design/远程电脑-设置页补画面分区-2026-11-20.html`（`design/` 按仓规不入库）：四张图含画质双列下拉展开、范围单列下拉 + 禁用态、能力读取失败错误条。错误条实现用产品真实的 danger 红，稿中近似画成橙色。
- 平台判据收口 `desktop/src/lib/rcPlatform.ts`（rc 域唯一判断处，不复用壳子 `lib/platform.ts`——本仓无消费方，且 rc 域文件统一 `rc` 前缀）+ `desktop/src/hooks/usePlatform.ts`（模块级单飞读 `get_app_info`，进程内只发一次，失败回落 undefined 且不重试）。**undefined 不降级**：首帧与读取失败都不亮提示，防 Windows 首帧闪一下「暂不支持」。
- 降级态四处接线：主区换通知卡（`RcUnsupportedPlatform`，吃 theme.css warning 令牌）/ 侧栏整体不渲染 / 错误槽抑制（mac 上 invoke 失败也无红条）/ 标题栏中格静态徽章（「远程电脑仅支持 Windows」，非按钮）；另停 `useRcAutoCheck` 轮询与自动开通道（该 pref 缺省 true，否则 mac 白弹失败 toast）。判据「出口不存在时就不要画一个出口」。
- 拆分方案文档迁入本仓 `docs/DeskPanda-拆分方案-2026-11-19.md`（自 PastePanda，现为 DeskPanda 侧活文档）：S1–S5 标记完成态并补记实际偏差（S2 未迁 macOS 三件套、S3 的 12 处依赖落点、S5 体积红线实测值），新增 **S6 正式阶段**（macOS 交付：迁入范围清单 / 迁移对账 / 验收判据；§6 的 macOS 待定项改为 2026-11-20 已决断 D6）。`CLAUDE.md`、`README.md` 与 5 处代码注释（`lib.rs` / `commands/rc.rs` / `commands/system.rs` / `rc/mod.rs` / `rc/audio.rs`）原指向 PastePanda 仓的悬空引用全部改指本文件。
- 新增 19 条测试（全量 1009 → 1028）：`rcPlatform` 5（含 🔴 undefined 不降级、大小写敏感）/ `usePlatform` 3（含 🔴 多处挂载只发一次 invoke）/ `RcUnsupportedPlatform` 4 / `RcA2TitleBar` +2（降级态中格是静态徽章、无死按钮）/ `RcWorkbench` +5（接线：mac 主区只剩通知卡、侧栏退场、🔴 Windows 零影响、首帧 undefined 不降级、错误条抑制）。7 处注入破坏均精确变红后还原。

## [0.1.1] - 2026-11-20

### 更新摘要
远程电脑独立成应用：在两台电脑之间远程控制、传文件、互传剪贴板，安装包只有 8.7 MB。

### 新增
- 远程电脑成为独立应用（DeskPanda），不再依附原桌面软件；托盘常驻，关掉主窗口仍在后台待命
- 「有人请求远程这台电脑」的确认浮层：主窗口没开、或被全屏应用盖住时也能看到，并可一键允许 / 拒绝
- 安装包随附 FFmpeg 运行库，AV1 与硬件编码在装好的机器上直接可用（缺少时自动退回软件编码）

### 修复
- 修复主窗口关闭后收不到远程请求确认：确认卡片此前只迁了界面、没有入口，实际永远不出现
- 修复 macOS 发版作业打包步骤的工作目录缺陷：路径改为仓库根相对并加产物齐全校验，避免 Release 缺 mac 安装包

### 技术细节
- 入站浮层（`rcask.html`）此前只迁了组件、没迁入口页与命令注册：`ask_pop.rs` 指向的页面不存在（白窗），`rc_ask_state/hide/reopen` 未进 `invoke_handler`（卡片空白），`on_change` 未挂到 `boot.rs` 的会话回调（浮层不随状态换档），`capabilities` 的 windows 名单也缺 `rc-ask`（`listen()` 全静默失败）。四处一起补，并加了源码守卫钉住。
- 删除 `ui-guards` feature：它把「命令注册 / capabilities / 前端 invoke」三侧成对的守卫挡在默认测试之外，而 pre-push 跑的是 `--no-default-features` ⇒ 守卫永远不会执行。改为常开后按本仓结构校正路径（注册表在 `main.rs`、`on_change` 在 `rc/boot.rs`、capabilities 只有 `default.json`）。
- 注册穷举守卫两处假绿：① 只认 `commands::` 前缀，`rc::ask_pop::` 的注册等于不存在；② 不剥行注释，注释掉一条注册仍会被解析成「已注册」（实测注释掉 `rc_ask_state` 后 787 条测试全绿）。已修，并用「注释掉注册 → 红」验证过。
- FF DLL 覆盖环境变量改名 `DESKPANDA_FF_DLL_DIR`（旧名 `PASTEPANDA_FF_DLL_DIR` 仍认）。
- CI 的 Windows 作业摘掉 sccache：cargo 1.100 起新 build-dir 布局默认开启，每个依赖一条 `-L`，本 crate 近 450 个依赖把 rustc 命令行撑到 56 KB，超过 Windows 32 KB 上限；cargo 本会用 argfile 兜底，而 sccache 不认 argfile ⇒ 编译期报 `failed to spawn ... os error 206`（报错文本里没有「命令行过长」字样，极难归因）。跨运行缓存改由 rust-cache 承担，macOS 侧 ARG_MAX 足够大，继续用 sccache。
- 帧龄 EMA 的原子更新改成 CAS 循环：rustc 1.100 把 `AtomicI64::fetch_update` 标为 deprecated（改名 `try_update`），CI 的 clippy 带 `-D warnings` 会判红，而本机 1.96 还没有新名字。同时把单步推进抽成纯函数 `ema_step` 并补 4 条单测（首样本落值 / 升 α=1/2 / 降 α=1/8 / 相等幂等），防日后 α 被悄悄改动。
- 工具链钉版本：新增 `rust-toolchain.toml`（1.96.0）作为唯一源，CI 四个作业显式钉同一版本，并加一道 3 秒的自检作业（toml 与 workflow 漂了、或 `@stable` 回潮就红）。此前 CI 跟 latest stable、本机落后若干版，新 lint 只红 CI 不红本机，红灯只能靠推送后看 Actions 才知道。

## [0.1.0] - 2026-10-26

### 更新摘要
deskbase 壳子基线：复制即可开始新的桌面应用。

### 新增
- 三层架构资产：技术壳（托盘/单实例/窗口重建）、更新闭环（检查→下载→重启）、设计系统（token + 30+ 组件）
- rename-app.mjs 一键改名（11 处产品标识一次改齐）
- 示例页 HomeTab / SettingsTab 演示业务页写法

### 技术细节
- 版本真源为 src-tauri/Cargo.toml，另两处由 sync-version.mjs 同步

## [0.0.9] - 2026-10-25

### 更新摘要
壳子预览版：三层资产初版拼齐，复制流程跑通。

### 新增
- 技术架构层：托盘/单实例/关窗销毁 webview/DWM 圆角
- 更新架构层：单飞闸 + 多源回退 + 可撤销自动重启

### 修复
- 修复设计预览页残留业务引用导致类型检查失败
