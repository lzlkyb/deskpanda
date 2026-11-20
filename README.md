# DeskPanda

[![CI](https://github.com/lzlkyb/deskpanda/actions/workflows/build.yml/badge.svg)](https://github.com/lzlkyb/deskpanda/actions/workflows/build.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Release](https://img.shields.io/github/v/release/lzlkyb/deskpanda)](https://github.com/lzlkyb/deskpanda/releases)
[![Tauri 2](https://img.shields.io/badge/Tauri-2-FFC131?logo=tauri&logoColor=white)](https://tauri.app)
[![React 19](https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=black)](https://react.dev)
[![Rust 1.96](https://img.shields.io/badge/Rust-1.96-DEA584?logo=rust&logoColor=black)](https://www.rust-lang.org)

**两台电脑之间的远程桌面：远程控制、文件传输、跨设备剪贴板。独立应用，托盘常驻，自带签名自动更新。**

> 当前版本 v0.1.1（2026-11-20 首发）。从 PastePanda 拆出，基于 Deskbase Tauri 2 壳子独立演进。
> 远程桌面功能目前仅 Windows 交付；macOS 可安装、可用托盘与更新，但远程功能排期在 S6（见「平台支持」）。

## 它能做什么

- **远程控制**：键鼠操作、三档画质（流畅/均衡/清晰）、画面范围（单屏/全屏/指定显示器）、系统声音、暂停画面、解除输入锁定。
- **文件传输**：双向传文件，带速度与剩余时间的进度条、可取消、可改接收目录；大文件与长中文名均已双机实测。
- **剪贴板互通**：文字（含中文与 emoji）和图片双向同步，以真实粘贴结果为准。
- **配对与发现**：邀请码配对、短码（`PP-XXXX-XXXX`）粘贴、局域网「附近的设备」自动发现；与主产品 PastePanda 互不串台（独立组播地址与 ALPN）。
- **入站请求浮层**：有人请求远程这台电脑时，即使主窗口已关进托盘、或前方是全屏应用，也会置顶弹出确认卡（不抢焦点，超时 120 秒自动消失）。
- **一直在线**：托盘常驻，关掉主窗口仍在后台待命；关窗销毁 webview 省内存，需要时托盘一键唤回。
- **自动更新**：签名校验、多源回退（GitHub + Gitee 镜像）、可撤销的重启安装。

## 平台支持

| 平台 | 状态 | 说明 |
|------|------|------|
| Windows 10/11 x64 | ✅ 完整 | 远程控制 / 文件传输 / 剪贴板 / 系统声音全部可用；S5 双机端到端验收已过 |
| macOS (arm64) | ⚠️ 壳子可用 | 托盘、单实例、窗口、更新均正常；远程桌面功能未迁入，打开工作台会显示「暂不支持 macOS」的明确提示。迁入排期 S6 |

> 为什么 mac 只有壳子：远程采集/编码/音频/键鼠注入依赖一整套尚未迁入的平台桥接。
> 详见 `docs/DeskPanda-拆分方案-2026-11-19.md` 的 S6 章节。

## 快速开始

到 [Releases](https://github.com/lzlkyb/deskpanda/releases) 下载安装包（Windows: NSIS `.exe`；macOS: `.zip`）。

1. 两台电脑都安装同一版本（协议未做版本协商，版本号须一致）。
2. 一台打开「配对」生成邀请码，另一台输入 —— 或等「附近的设备」互相出现后点同意。
3. 在设备卡片上点「远程控制」，被控端确认浮层点允许，即可开始。

> 首次启动 Windows 会弹防火墙「允许访问」，请允许（专用/公用网络都勾上），否则局域网发现与直连不可用。
> 完整双机验收步骤与判据见 `docs/S5-双机验收清单.md`。

## 实测指标

来自 S5 双机验收（v0.1.1，Windows release + NSIS）：

| 项 | 值 |
|---|---|
| 安装包体积 | 8.7 MB（含 FFmpeg 运行库压缩后） |
| 安装后目录 | 约 33 MB（exe 27.9 + ffmpeg 5.2） |
| 冷启动到窗口出现 | 约 1.0 s |
| 空闲常驻内存（主进程） | 46 MB（WebView2 独立子进程未计入） |
| 体积红线 | 安装包 ≤ 15 MB / 安装后 ≤ 45 MB |

## 架构与技术栈

- **前端**：React 19 + TypeScript + Vite + Tailwind CSS 4 + Zustand + TanStack Query
- **后端**：Tauri 2 + Rust 1.96（工具链版本唯一源：`rust-toolchain.toml`）
- **P2P/网络**：iroh（端点与中继）、组播局域网发现、独立 ALPN `deskpanda/rc/1`
- **媒体**：Win32 DXGI 采集 + H264/AV1 + FFmpeg 运行库（AV1 / 硬件编码，缺失时自动退软件编码）
- **存储**：rusqlite（bundled），只建远程设备与设置两张表

关键目录：

```
CLAUDE.md                                  工程纪律（AI 与开发者都先读）
docs/协作开发规范-2026-11-20.md            多人协作规矩（权限/分支/PR/发版）
docs/DeskPanda-拆分方案-2026-11-19.md      拆分背景与 S1–S6 阶段计划
docs/S5-双机验收清单.md                    双机验收判据与实测数据
.github/workflows/build.yml                CI：门禁 + tag 双平台发版
.githooks/pre-push                         push 前 7 项本地门禁
desktop/
  src/                                     React 前端（components/rc/* 为远程桌面业务）
  src-tauri/                               Rust 后端
    src/rc/                                远程桌面核心（采集/编码/输入/文件/剪贴板）
    src/net/                               端点、发现、帧传输
    src/platform/                          屏幕/设备能力
    src/commands/                          命令分层（system / update / rc*）
    src/security/path.rs                   路径白名单安全原语
  scripts/                                 版本同步 / 发版链 / updater.json 生成
```

## 从源码构建

**前置**：Node 22+、Rust 1.96.0（由 `rust-toolchain.toml` 固定）、Tauri 2 系统依赖（Windows：WebView2 + VS C++ Build Tools；macOS：Xcode CLT）。

```bash
cd desktop
npm install                 # 首次若报 peer 依赖错误，加 --legacy-peer-deps 跑一次
npm run dev                 # tauri dev：前端 HMR；改 Rust 需重启

npm test                    # vitest 单测
npm run lint                # eslint（react-hooks 规则）
npx tsc --noEmit            # 类型检查

cd src-tauri
cargo fmt --check
cargo clippy --all-targets --no-default-features -- -D warnings
cargo test --no-default-features
```

> **push 前启用门禁钩子**：`git config core.hooksPath .githooks`。钩子会跑 7 项检查
> （组件体量 / fmt / eslint / tsc / vitest / clippy / cargo test），CI 同标准把关。
> 不要在 push 前另起独立 `cargo test`，会争抢 `target/` 构建锁。

打包 exe / 安装包需带签名私钥（本机在 `~/.tauri/deskpanda.key`），且先询问维护者：

```bash
export TAURI_SIGNING_PRIVATE_KEY="$(cat "$HOME/.tauri/deskpanda.key")"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
cd desktop && npm run build
```

## 参与开发

欢迎 Issue 与 PR。动手前请读：

- `CLAUDE.md` —— 代码与技术纪律（版本号单一真源、组件体量红线、后台循环六问、提交前审 diff 等）
- `docs/协作开发规范-2026-11-20.md` —— 角色权限、分支模型、PR/Review 规则、发版豁免、上手清单

合入门槛：PR + 4 项 CI 检查全绿 + 至少 1 人 approve；`main` 禁止直推与强推。
提交前缀：`feat:` / `chg:` / `fix:` / `refactor:` / `docs:`。

## 版本历史

| 版本 | 摘要 |
|------|------|
| v0.1.1 | 远程电脑独立成应用（DeskPanda）：托盘常驻、入站请求浮层、FFmpeg 随包、签名自动更新 |
| v0.1.0 | Deskbase 壳子基线：复制即可开始新的桌面应用 |
| v0.0.9 | 壳子预览版：三层资产初版拼齐 |

详细变更见 `CHANGELOG.md`。

## 许可

[MIT](LICENSE) © DeskPanda contributors
