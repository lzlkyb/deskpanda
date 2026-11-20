# DeskPanda 手机端远程迁移方案（2026-11-20）

> 2026-11-20 撰写。目标：把 PastePanda 手机端（`src-mobile/`）的**远程电脑**代码
> 迁入本仓，使 DeskPanda 具备 Android 被控端/主控端能力。本文是执行前的方案，
> 所有规模数字均为 2026-11-20 实测。
>
> 与 `DeskPanda-拆分方案-2026-11-19.md` 的关系：拆分方案的 S1–S5 已完成
> （共享逻辑层已迁入本仓），S6 是 macOS 交付阶段；**手机端迁移是并列的新阶段
> （S7）**，本文即 S7 的执行方案。拆分方案文档只留索引，细节以本文为准。

## 0. 决策表

| # | 决策点 | 结论 | 依据 |
|---|---|---|---|
| D1 | 手机端放哪 | `desktop/src-mobile/`（镜像 pastePanda 布局） | `@/` → `src/` 别名已就绪，30 个共享 import 零改动；tsconfig include 加一项即可 |
| D2 | 知识库去留 | **不迁**（60 文件 5,041 行） | deskpanda 无知识库；手机端远程面对 `kb_sync_*` 零调用（实测） |
| D3 | 构建配置 | 独立 `vite.config.mobile.ts`（照搬 pastePanda 模式），桌面构建不受影响 | pastePanda 同款；单配置会把移动端入口拖进桌面 build |
| D4 | 移动端 dev 端口 | **1432**（桌面 1430 错位 +2，对齐 pastePanda 1420/1422 习惯） | 端口撞车表现是白屏，必须显式错开；执行时先 `netstat` 核对 |
| D5 | 包名 | `com.pastepanda.app` → `com.pastepanda.deskpanda` | identifier 已是 `com.pastepanda.deskpanda`；Kotlin 9 文件 + Rust 2 处装载点全部硬编码旧包名，漏改表现是插件静默装载失败 |
| D6 | TouchSandbox | **迁**（`dev/` 1 文件 71 行） | 无会话手势联调面，对远程输入调试有直接价值 |

## 1. 摸排结论（现状）

### 1.1 pastePanda 手机端是什么

Tauri Android 应用，`src-mobile/` 252 文件 / 22,696 行：

| 目录 | 文件 | 行数 | 处置 |
|---|---|---|---|
| `session/` | 77 | 6,757 | 迁（远程会话全栈：输入/视频/指针/剪贴板/保活） |
| `devices/` | 43 | 4,609 | 迁（设备列表/配对/文件收发） |
| `ui/` | 55 | 4,540 | 迁（移动端 UI 组件库，零知识库耦合，实测） |
| `knowledge/` | 60 | 5,041 | **不迁** |
| `settings/` | 5 | 673 | 迁（RcSettingsView/RcMobileHistory/MobileUpdateSection） |
| `video/` | 5 | 247 | 迁（PinchViewport/VideoSurface） |
| `dev/` | 1 | 71 | 迁（TouchSandbox） |
| `styles/` | 1 | 140 | 迁（mobile-base.css） |
| 顶层 | 3 | 482 | 迁并改造（App.tsx 215 / App.test.tsx 252 / main.tsx 15） |
| **合计迁入** | **192** | **≈17,655** | |

入口 `src-mobile/index.html`（`/mobile-boot.css` 外链防 WebView 冷启动白屏，
内联 `<style>` 会卡死 vite build——pastePanda 实测教训，搬运时保持外链）。

### 1.2 deskpanda 已有什么（拆分 S1–S5 的成果，实测全部存活）

- **共享逻辑层 30 个模块全部在位**：`@/lib/utils`、`@/hooks/useRc`、
  `@/lib/api/rc`、`useRcFile`、`rcQuality`、`rcDevice`、`rcShortCode`、`rcStore`、
  `rcFileStore`、`rcSessionStats`、`rcPointer`、`rcFile`、`useRcFrames` 等——
  手机端 192 个文件的 `@/` import **不需要任何适配**。
- **Rust 侧已就绪**：`rc::keepalive.rs`（`rc_keepalive_set` 命令已注册、
  `set_session_display` 已就绪）；`rc::file_transfer` 的 Android 段
  （`init_android_receive_dir`，函数在位但 main.rs **无调用点**）；
  `rc_file_send_blob(_abort)` 已注册（见 §2 与 #12 的交叉）。
- **测试基建**：`desktop/vitest.config.ts` 的 include 是全仓通配
  （`**/*.test.tsx`），src-mobile 测试搬入即自动收编；但 `DOM_NEEDED_TS`
  需补 17 个 src-mobile 条目（pastePanda 同款清单，见 M1）。

### 1.3 命令缺口（手机端 18 个 invoke 对 deskpanda 101 个注册命令求差）

| 命令 | 判定 | 说明 |
|---|---|---|
| `apk_install_status` / `apk_open_install_settings` | **缺，迁** | 应用内自更新（APK 安装权限引导），配 `update_android.rs` + ApkInstallerPlugin |
| `mobile_interaction_haptic` / `mobile_interaction_set` | **缺，迁** | 触感/交互反馈，`mobile_interaction.rs` 57 行 |
| `rc_file_send_blob` / `_abort` | 已注册 | deskpanda 现有「死命令」，手机端迁入后**激活**（§2） |
| `check_update` / `start_update` | 已注册 | 桌面 updater 版；Android 需在 `update.rs` 加 cfg 分派到 `update_android.rs` |
| `mobile_received_file_action` | 半缺 | deskpanda 已有前端包装器（`rcFile.ts:140`）但未注册；随 ReceivedFileActions.kt 激活 |
| `rc_sync_offers`、`kb_sync_*` 9 个 | **不迁** | 知识库同步链；手机端远程面零调用（实测 grep 无命中）；进 #12 清理 |

### 1.4 Android 基建缺口

- Kotlin 插件：pastePanda `src-tauri/android/` 10 文件 901 行；
  **迁 9 个 660 行**（KnowledgeSharePlugin 93 + KnowledgeShareStore 148 属知识库，不迁）；
  其中 `ReceivedFileActions.kt` 依赖 `KnowledgeSharePlugin.chooseExport`，
  迁时抽一个 ≤30 行的 ExportChooser 帮助类解耦。
- Rust 装载点 2 处硬编码 `com.pastepanda.app`：`rc/keepalive.rs:97`
  （register_android_plugin）、`shared_ep/network.rs:22`（IrohNetworkPlugin）。
- **Cargo.toml 无桌面专属排除段**——`tauri-plugin-single-instance`（第 33 行）
  与 `arboard`（第 95 行）会让 Android 构建直接失败（pastePanda 同款依赖
  已按 `[target.'cfg(not(any(target_os="android",target_os="ios")))'...]`
  排除；deskpanda 无 window-state/global-shortcut/clipboard-manager，
  段内只需这两条）。
- 脚本/配置全缺：`tauri.android.conf.json`、`vite.config.mobile.ts`、
  `scripts/android-build.mjs`（238 行）、`prepare-android.mjs`（136 行，
  含 AndroidManifest 权限补丁与 `com.pastepanda.app` 硬编码）、
  `publish-apk.mjs`（342 行）、package.json 脚本族。
- main.rs 无 `download_dir` → `init_android_receive_dir` 接线（pastePanda
  lib.rs:420 段有，deskpanda main.rs 是拆分时重写的 698 行直构造版）。

## 2. 与 #12 清理的交叉修正（先记下来，防双向返工）

#12 已决策的 A 批清理中，两项因手机端迁入**反转**：

| 原 #12 判定 | 新判定 | 原因 |
|---|---|---|
| `rc_file_send_blob` / `_abort` 是死命令，删 | **保留**，从清理名单移除 | 手机端发文件通道在用（迁入后 Rust 侧注册即激活） |
| `mobile_received_file_action` 是死包装器，删 | **保留**，从清理名单移除 | ReceivedFileActions.kt 的调用出口 |

其余维持 #12 原判：`rc_sync_offers` + RcSyncOffers 组件 + `kbSyncAllowFromRc`/
`kbSyncDenyFromRc` 包装器仍属清理项（手机端零调用）；RcFileOverlay 补接线独立。

## 3. 阶段计划（M1–M5）

### M1 构建基建（无产品代码改动）

1. `desktop/src-tauri/Cargo.toml`：新增 `[target.'cfg(not(any(target_os = "android", target_os = "ios")))'.dependencies]`，纳入 single-instance + arboard（注释写明：移动端构建排除，桌面不动）。
2. `desktop/src-tauri/tauri.android.conf.json`：beforeDevCommand `npm run dev:mobile`、devUrl `http://localhost:1432`、frontendDist `../dist-mobile`（deskpanda 无 easytier，不需要 resources null 化）。
3. `desktop/vite.config.mobile.ts`：照搬 pastePanda（端口 1432、`.vite-mobile` 缓存、入口改写中间件、index.html emit 到包根）；**去掉 rcask.html**（Android 单 webview，pastePanda 注释已记）。
4. `desktop/scripts/`：迁 `android-build.mjs` / `prepare-android.mjs` / `publish-apk.mjs` + `prepare-android.test.ts`，改其中的 pastePanda 路径/包名/环境配方（LIBCLANG_PATH 等按本机实际）。
5. `desktop/package.json`：加 `dev:mobile` / `build:mobile` / `android:*` 脚本族。
6. `desktop/tsconfig.json`：include 加 `"src-mobile"`。
7. `desktop/vitest.config.ts`：`DOM_NEEDED_TS` 补 17 个 src-mobile 条目（devices 3 / session 13 / ui 1）。
8. `desktop/public/mobile-boot.css`：从 pastePanda `public/` 迁入（含 icon.png 核对）。
9. **验收**：`npm run build:mobile` 产出 `dist-mobile/`（此时 src-mobile 尚未迁入，用一个临时最小入口或先迁 M4 的入口文件——若顺序调整，在 M1 内说明）；`cargo check` 桌面不受影响。

### M2 Rust 命令补齐

1. 新增 `desktop/src-tauri/src/commands/update_android.rs`（660 行，含三个双端编译纯函数）；`update.rs` 加 `#[cfg(target_os="android")]` 分派（check_update/start_update 转交）；注册 `apk_install_status`/`apk_open_install_settings`。
2. 新增 `commands/mobile_interaction.rs`（57 行）；注册 `mobile_interaction_haptic`/`mobile_interaction_set`。
3. main.rs 补 `#[cfg(target_os = "android")]` 的 `download_dir` → `init_android_receive_dir` 调用。
4. `net/endpoint.rs`（SharedEp 初始化处）补 Android 段：register IrohNetworkPlugin + 切网 Channel 刷新（pastePanda `shared_ep/network.rs` 79 行为蓝本，逐行对照落点）。
5. 注册 `mobile_received_file_action`。
6. Kotlin 装载点包名 `com.pastepanda.app` → `com.pastepanda.deskpanda`（keepalive.rs:97、network.rs:22）。
7. **验收**：桌面 `cargo test` 全绿（纯函数测试随迁）；clippy `-D warnings` 绿。

### M3 Kotlin 插件迁入

1. 迁 9 文件 660 行（MainActivity / RcSessionDisplay / RcSessionForegroundService / RcKeepalivePlugin / IrohNetworkPlugin / ApkInstallerPlugin / MobileInteractionPlugin / ReceivedFileActions），包名全量替换。
2. `ReceivedFileActions.kt` 解耦：抽 ≤30 行 ExportChooser（持有 ActivityResultLauncher + chooseExport），替代 KnowledgeSharePlugin 依赖。
3. **验收**：Android 目标 `cargo check --target aarch64-linux-android` 通过（依赖本机 NDK 环境；若本机不可用，记录为 M5 前置）。

### M4 前端迁入与改造

1. 192 文件整体迁入 `desktop/src-mobile/`（保留 git mv 语义外的纯拷贝，LF 行尾）。
2. `App.tsx` 去知识库：删 KnowledgeView/useKnowledgeInbox import、knowledgeTask state、seenShareRequest effect、knowledge badge、knowledgePane 渲染共约 7 处；`mobileDestinations.ts` 删 knowledge 项（1 行）。
3. `src-mobile/index.html`：标题「PastePanda 远程」→「DeskPanda 远程」。
4. `useRcSessionKeepalive.ts` 随 session/ 迁入（deskpanda 缺此 hook，Rust 命令已就绪）。
5. **验收**：`tsc` / `eslint` / `vitest`（含随迁测试）全绿；`npm run build:mobile` 产出含远程工作台的 dist-mobile。

### M5 真机验收（需用户真机）

debug 包烤的是 devUrl，手机不在同网会白屏——一律 `--release` 或同网 dev。
验收项：连接远程电脑（被控/主控双向）、手势输入准度（TouchSandbox 可先自检）、
文件收发（blob 通道 + 接收目录）、后台保活（前台服务常驻通知）、应用内更新
（安装权限引导 → 系统安装器）。

## 4. 风险与未决项

1. **端口 1432 占用**：执行 M1 时 `netstat` 核对，撞车则按 pastePanda 惯例再错位（+2），同步改 tauri.android.conf.json。
2. **本机 NDK/JDK 环境**：pastePanda 的 android-build.mjs 环境配方是按本机路径写的（`D:\AItool\jdk-17...` 等），迁入后按 deskpanda 实际核对；若本机 NDK 缺失，M3 的 Android 编译验收降级为「代码就绪、真机联调时补验」。
3. **capabilities 权限**：deskpanda 只有 `default.json`；Android 侧需核对 fs 权限是否覆盖 received-file-out 缓存目录（M2 时对照 pastePanda default.json）。
4. **`icon.png` 落点**：deskpanda 资源在 `desktop/` 根而非 `public/`，vite publicDir 行为迁移时实测（mobile-boot.css 与 index.html 的引用路径必须与产出一致）。
5. **vite build 体积**：移动端 bundle 与桌面分开（dist-mobile），互不影响。

## 5. 验收判据（总表）

| # | 判据 | 阶段 |
|---|---|---|
| 1 | 桌面七项门禁（体量/fmt/eslint/tsc/vitest/clippy/cargo test）全程不回归 | M1–M4 每阶段 |
| 2 | `npm run build:mobile` 产出 dist-mobile 且入口 HTML 在包根 | M1/M4 |
| 3 | src-mobile 随迁测试在 deskpanda vitest 下全绿（含 17 个 DOM_NEEDED 条目） | M4 |
| 4 | Android 目标 cargo check 通过（或记录环境缺失降级） | M3 |
| 5 | 包名全仓无 `com.pastepanda.app` 残留（grep 判据） | M3 |
| 6 | #12 交叉修正落地：rc_file_send_blob 与 mobile_received_file_action 移出清理名单 | M2 |
| 7 | 真机五项验收（连接/手势/文件/保活/更新） | M5 |

## 6. 执行记录（2026-11-20 ~ 2026-11-21）

> 本节是「做完了什么、卡在哪、证据在哪」的流水。方案章节（§0–§5）保持撰写时的
> 判断不变；执行中发现的偏差在此记录，不回改上文。

### 6.1 阶段状态

| 阶段 | 状态 | 说明 |
|---|---|---|
| M1 构建基建 | ✅ 完成 | Cargo.toml 移动端排除段（single-instance + arboard）、`tauri.android.conf.json`、`vite.config.mobile.ts`、`scripts/{android-build,prepare-android,publish-apk}.mjs` + 测试、package.json 脚本族、tsconfig include、`public/mobile-boot.css` |
| M2 Rust 命令补齐 | ✅ 完成 | `update_android.rs` / `mobile_interaction.rs` / `mobile_export.rs`（新建的解耦层）/ `mobile_received_file.rs`；`download_dir` 接线；IrohNetworkPlugin 注册点；两处包名替换 |
| M3 Kotlin 插件迁入 | ✅ 完成（2026-11-20） | 9 文件 660 行落 `desktop/src-tauri/android/`；详见 §6.2 |
| M4 前端迁入与改造 | ✅ 完成 | 192 文件迁入 `desktop/src-mobile/`；App.tsx 去知识库；tsc/eslint/vitest 全绿 |
| M5 真机验收 | ⏳ 待真机 | `gen/android` 尚未 `tauri android init`；需用户真机联调 |

### 6.2 M3 明细（2026-11-20）

- 落盘 9 文件（`desktop/src-tauri/android/`）：MainActivity / RcSessionDisplay /
  RcSessionForegroundService / RcKeepalivePlugin / IrohNetworkPlugin /
  ApkInstallerPlugin / MobileInteractionPlugin / ReceivedFileActions /
  **ExportChooserPlugin（新建）**。包名全量 `com.pastepanda.deskpanda`。
- **ExportChooserPlugin 是本次唯一的新代码**（§1.4 计划的解耦落地）：Rust 侧
  `mobile_export.rs` 按类名反射装载它，契约只有 `receivedFile({path, action})`；
  `ReceivedFileActions.start()` 的第二参数由 `KnowledgeSharePlugin` 改为
  `ExportChooserPlugin`（只借 `chooseExport` 递 ActivityResultLauncher）。
- 产品字样随包名改：前台服务通知标题 `PastePanda 远程` → `DeskPanda 远程`；
  WifiLock tag `pastepanda:rc-session` → `deskpanda:rc-session`。
- 执行中抓到一处**方案漏项**：`prepare-android.mjs` 的复制清单只有 8 个文件名，
  没有 `ExportChooserPlugin.kt`——照原清单跑，新插件进不了 APK 且
  `prepare-android.test.ts` 的「每个 .kt 都从唯一源码恢复」断言会 ENOENT。
  已补 9 项（该测试同时是复制清单的守卫）。
- 另修两处测试环境/路径问题（都不是产品代码缺陷）：
  `vitest.config.ts` 的 `DOM_NEEDED_TS` 补 `scripts/prepare-android.test.ts`
  （该测试用 DOMParser 校验补丁后的清单是合法 XML，node 环境下 ReferenceError）；
  `src/lib/rcAskPop.test.ts` 的 invoke_handler 守卫从 `main.rs` 改读 `app.rs`
  （应用体已迁到 app.rs，main.rs 是薄壳）。
- 提交前 parity 抽查抓到 **M4 漏迁两个守卫**：`changelogDisplay.ts` 的注释
  声称「配 changelogDisplay.test.ts 守卫钉死口径」，但该测试与
  `src/__tests__/changelogParser.test.ts` 都没随迁（消费方是 S7 的
  `src-mobile/ui/MobileUpdateSheet.tsx`）。两文件与 pastePanda 版逐字节相同
  （changelogParser/changelogDisplay 两仓 identical），原样补迁，15 条全绿。
  src-mobile 测试 parity 其余干净：92−21=71，缺的 21 个全在 `knowledge/`
  （D2 决策不迁）。

### 6.3 Android 编译验收（替代 §3-M3 第 3 条的本机 gradle 验收）

- `cargo check --target aarch64-linux-android`：**0 error / 0 warning**。
- 🔴 **在仓内目录跑这条命令会 E0463**（`can't find crate for core`）：仓根
  `rust-toolchain.toml` 钉 1.96.0，而该工具链没装 `aarch64-linux-android`
  target（装在 default stable 上）。配方：`RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-msvc`
  + `scripts/android-build.mjs` 的 `ENV_DEFAULTS`（NDK 27.2.12479018 的 CC/CXX/AR/
  linker）+ `CARGO_TARGET_DIR=target-android`。本机验证过 0/0；顺手装
  `rustup target add aarch64-linux-android --toolchain 1.96.0` 可让仓内直跑（未装，
  属本机环境选择）。
- 对照组：pastePanda 上游同样命令 **2 个 E0384 编不过**
  （`commands/rc.rs` / `screenshot.rs` 缺 `mut`）——上游 Android 路径从未真正
  编译过；deskpanda 迁入后的 rc 树反而是干净的。这解释了两仓「Android 代码看着
  一样、其实一边从没过编译器」的风险。

### 6.4 途中事故：comctl32 / LNK1123（Windows app bin 链接失败）

- 现象：`tauri::Builder` 链加上 `muda`（托盘）后，Windows release 的 app bin
  `LNK1123 failed ... CVT1100: 资源重复。类型: MANIFEST，名称: 1`。
- 误判留档：曾假设根因是 `app.rs → muda → TaskDialogIndirect` 需要 comctl32 v6，
  按这个假设改了两处注释——**已被证伪**（import 与门控都不是原因）。
- 真相：两份完全相同的 RT_MANIFEST（ID 1）冲突——tauri-build 自己生成一份
  （VERSIONINFO + ICON + manifest），我们又用 `embed_resource` 补了一份
  Common-Controls v6 依赖。`compile_for_tests()` 只发 `link-arg-tests`，到不了
  app bin；`compile_for_everything()` 发裸 `cargo:rustc-link-arg` 是唯一同时覆盖
  app bin / lib 单测 / `tests/` 的口径。
- 修复（`build.rs` 定稿）：`WindowsAttributes::new_without_app_manifest()` +
  `compile_for_everything()`。默认 manifest 与补的 Common-Controls 依赖内容一致
  （均无 supportedOS 条目），是等价替换而非功能裁剪。
- 验证：app exe / bin-unittests / lib 单测 / smoke / update_flow 五个产物的
  MANIFEST 均恰好 1 份（392B），VERSION + GROUP_ICON + 6 ICON 健在。
- 桌面四道门禁全绿：`cargo test` 默认 / no-default 各 840 passed；
  `clippy --all-targets -D warnings` 默认 / no-default 各 0 条。

### 6.5 CI 与 macOS/Linux 口径

- CI 四作业（前端 / Windows test+clippy / macOS clippy+test / 工具链自检）实测
  全绿（run 44649693499）。macOS 在钉 `rust-toolchain.toml` 1.96.0 后通过——
  「1.96 能否编 mac 侧代码」这个疑点已消除。
- 本项目 CI 无 Linux 作业；`cfg(unix)` 相关代码只经 macOS 路径覆盖。
- Windows 侧 sccache 不认 cargo 新 build-dir 布局的 argfile（rustc 命令行 56KB
  超 32KB 上限，`os error 206`）已修：CI 摘掉 sccache 交给 Swatinem/rust-cache；
  macOS ARG_MAX 1MB 量级不受影响，继续用 sccache。

### 6.6 与 §5 判据的对账

| # | 判据 | 现状 |
|---|---|---|
| 1 | 桌面七项门禁不回归 | ✅ 839→840 测试全过（新增 1 条）；clippy/fmt/tsc/eslint/体量全绿 |
| 2 | `npm run build:mobile` 产出 dist-mobile | ✅（M1/M4 验收过） |
| 3 | src-mobile 随迁测试全绿（含 17 DOM 条目） | ✅ 全套 vitest 189 文件 / 1467 测试全绿（M3 补 scripts 1 条 DOM、补迁 changelog 守卫 2 文件 15 条） |
| 4 | Android cargo check 通过 | ✅ 0 error / 0 warning（§6.3） |
| 5 | 包名全仓无 `com.pastepanda.app` 残留 | ✅ grep 零命中（Kotlin 9 文件 + Rust 2 装载点） |
| 6 | #12 交叉修正落地 | ✅ 两命令移出清理名单并注册 |
| 7 | 真机五项验收 | ⏳ M5 待真机 |
