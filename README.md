# CC Switch — 原生桌面应用

> **版权声明**：本仓库基于 [CC Switch](https://github.com/farion1231/cc-switch)（MIT 许可证）进行适配和修改。原始项目由 [farion1231](https://github.com/farion1231/cc-switch) 开发，所有权利归原作者所有。

[English](README_EN.md) | 中文

> 原始 Tauri 2 桌面版文档：[README_tauri.md](README_tauri.md)（英文）· [README_TAURI_ZH.md](README_TAURI_ZH.md)（中文）· [README_TAURI_JA.md](README_TAURI_JA.md)（日文）

**CC Switch** 是一款用于管理 AI CLI 工具配置的全方位管理工具，支持 Claude Code、Codex、Gemini CLI、OpenCode、OpenClaw 和 Hermes。

本仓库是 [CC Switch](https://github.com/farion1231/cc-switch) 的一个适配分支，在**保留核心业务逻辑**的前提下，将 Tauri 2 桌面应用重构为**原生 GTK+WebKit 桌面应用**，支持 **Ubuntu 20.04** 等不支持 Tauri 2 的系统。

本分支同时针对国产 ARM64 Linux（例如麒麟 V10 SP1、华为擎云 L420）做了兼容处理：使用 WebKit2GTK 4.0，启动时自动关闭容易导致 Mali 显卡白屏的合成模式，并提供可直接安装的 ARM64 `.deb` 打包流程。

> 原始 Tauri 2 桌面版文档请参阅 [README_tauri.md](README_tauri.md)（英文）、[README_TAURI_ZH.md](README_TAURI_ZH.md)（中文）。

---

## Releases

| 版本 | 下载 | 说明 |
|------|------|------|
| v1.0.0 | [cc-switch-web_1.0.0_amd64.deb](https://github.com/greluoqixi/cc-switch-web/releases/download/v1.0.0/cc-switch-web_1.0.0_amd64.deb) | x86_64，适配 Ubuntu 20.04+ |
| v1.0.0 | `cc-switch-web_1.0.0_arm64.deb`（本地构建） | ARM64，适配麒麟 V10 SP1 等系统 |

安装方式：

```bash
curl -sL https://github.com/greluoqixi/cc-switch-web/releases/download/v1.0.0/cc-switch-web_1.0.0_amd64.deb -o cc-switch-web_1.0.0_amd64.deb
sudo dpkg -i cc-switch-web_1.0.0_amd64.deb
```

---

## 适用环境

| 环境 | 说明 |
|------|------|
| **Ubuntu 20.04 (Focal)** | ✅ 本分支的主要目标平台 |
| Ubuntu 22.04+ / Debian / macOS | ✅ 也可编译运行 |
| 需要 GLib >= 2.70 的系统 | ✅ 可编译 web-server 模式 |

### 为什么不直接使用 Tauri 2？

Ubuntu 20.04 缺少 Tauri 2 必需的依赖：
- `libwebkit2gtk-4.1-dev`（20.04 仅提供 4.0 版本）
- `glib-2.0 >= 2.70`（20.04 提供 2.64）
- `libsoup-3.0-dev`

本分支通过条件编译将所有 Tauri 依赖设为可选（`desktop` feature），核心业务逻辑作为独立 library 编译。前端通过嵌入式 WebKit 视图（WebKit2GTK 4.0）呈现，无需浏览器。

---

## 保留的功能

本分支保留了 CC Switch 的**全部核心业务逻辑**（约 100+ 个 API 端点），涵盖：

| 类别 | 功能 |
|------|------|
| **Provider 管理** | CRUD、切换、排序、50+ 官方预设、Live 配置同步、统一供应商 |
| **MCP 服务器管理** | 跨 5 个应用的统一 MCP 服务器管理（Claude/Codex/Gemini/OpenCode/Hermes） |
| **Prompts/Skills** | Markdown 编辑器、GitHub 仓库安装、跨应用同步、SSOT 存储、启动时自动从应用目录导入已有 Skills |
| **代理服务** | 本地 HTTP 代理、自动故障转移、断路器、Live 接管/恢复、热切换 |
| **Session 管理器** | 浏览/搜索/恢复 Claude/Codex/Gemini/OpenCode/OpenClaw/Hermes 对话历史 |
| **用量/费用追踪** | 请求日志、Token 统计、费用汇总、Provider/Model 对比图表 |
| **WebDAV 同步** | 通过 WebDAV 进行数据库和 Skills 云同步 |
| **通用配置片段** | 跨 Provider 共享配置模板，减少重复输入 |
| **自定义端点** | 为每个 Provider 添加/管理 API 端点 |
| **Claude 插件集成** | 自动写入 Claude Code 插件配置（primaryApiKey） |
| **跳过 Claude 安装确认** | 写入/清除 `~/.claude.json` 的 `hasCompletedOnboarding` 字段 |
| **终端启动** | 在 Provider 卡片中一键打开终端，自动注入环境变量 |
| **开机自启** | 设置面板中切换 XDG 自动启动 |

---

## 未实现的功能

以下功能依赖 Tauri 桌面平台 API，在本分支中不可用：

| 功能 | 原因 | 替代方案 |
|------|------|----------|
| **系统托盘** | 依赖 `tauri::tray` | 关闭窗口即退出应用 |
| **Deep Link** | 依赖 `tauri-plugin-deep-link` | 手动配置导入 |
| **自动更新** | 依赖 `tauri-plugin-updater` | 通过 apt 管理更新 |
| **原生菜单/快捷键** | 依赖 Tauri 菜单系统 | 浏览器快捷键 |
| **Copilot/Codex OAuth 认证** | 依赖桌面浏览器流程 | 暂不支持 |

> 注：文件选择对话框（SQL 备份导入/导出、ZIP 安装）通过浏览器兼容层（tauri-mock）已支持，使用浏览器原生文件选择器。

---

## 编译依赖

### 系统依赖

```bash
# Ubuntu 20.04 — 基础编译依赖
sudo apt install build-essential pkg-config libssl-dev libsqlite3-dev

# 原生桌面窗口（编译 native-shell 必需）
sudo apt install libgtk-3-dev libwebkit2gtk-4.0-dev
```

**不需要**安装 `libwebkit2gtk-4.1-dev`（Tauri 2 依赖，20.04 没有）。

### Rust 工具链

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# Rust 1.70+ 即可，推荐 1.85+
```

### Node.js 和 pnpm（仅构建前端时需要）

```bash
# Node.js 20.x
curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
sudo apt install nodejs

# pnpm
npm install -g pnpm
```

---

## 构建和运行

### 开发模式

```bash
# 1. 构建前端
pnpm install
pnpm build:renderer

# 2. 编译 web 服务器 + 原生窗口
cargo build -p web-server
cd native-shell && cargo build && cd ..

# 3. 运行原生窗口
./native-shell/target/debug/native-shell
```

### 前后端分离开发（热重载）

```bash
# 终端 1：启动后端 API 服务器
cargo run -p web-server

# 终端 2：启动 Vite 开发服务器（热重载）
pnpm dev:renderer
# 浏览器访问 http://localhost:3000?native=1，API 通过 Vite proxy 转发
```

### 编译原生窗口

```bash
cargo run --manifest-path native-shell/Cargo.toml
```

---

## 打包为 .deb 安装包

构建 `.deb` 安装包，包含两个二进制：
- `cc-switch` — 原生 GTK+WebKit 桌面窗口（双击直接使用）
- `cc-switch-web` — 嵌入式 HTTP 服务器（由原生窗口自动管理）

### 构建

```bash
# 方式 1：专用脚本
./build-deb.sh

# 方式 2：通过管理脚本
./run-web.sh package
```

输出：`cc-switch-web_1.0.0_<架构>.deb`，架构由 `dpkg --print-architecture` 自动检测（例如 `arm64`、`amd64`）。

### 安装

```bash
sudo dpkg -i "./cc-switch-web_1.0.0_$(dpkg --print-architecture).deb"
```

安装后双击桌面图标或从应用菜单启动 **CC Switch**。

### 使用方式

1. 双击桌面图标打开 CC Switch
2. 关闭窗口时自动停止所有后台服务
3. 如需开机自启，在 **设置 → 窗口行为 → 开机启动** 中开启

### 卸载

```bash
sudo dpkg -r cc-switch-web            # 保留配置
sudo dpkg --purge cc-switch-web       # 完全清除
```

---

## 适配版使用说明

### 1. 安装与启动

在 Debian/Ubuntu/麒麟等系统上，先确认 CPU 架构：

```bash
dpkg --print-architecture
```

使用与架构匹配的安装包。ARM64 设备应使用 `cc-switch-web_1.0.0_arm64.deb`：

```bash
sudo dpkg -i cc-switch-web_1.0.0_arm64.deb
```

安装后从应用菜单启动 **CC Switch**。程序会自动启动本地后台服务并打开 GTK 窗口，不需要手动打开浏览器。关闭窗口会同时停止后台服务。

如果 `dpkg` 报依赖未满足，可先安装运行库后重试：

```bash
sudo apt-get update
sudo apt-get install libgtk-3-0 libwebkit2gtk-4.0-37
sudo dpkg -i cc-switch-web_1.0.0_arm64.deb
```

### 2. 首次配置

打开应用后，按以下顺序完成配置：

1. 在 **Providers** 页面添加或导入 AI 服务商，填写 API 地址、密钥和模型。
2. 点击对应 Provider 的启用/切换操作，将配置写入目标 CLI 工具。
3. 在 **MCP、Prompts、Skills** 页面按需添加共享资源。
4. 在 **设置** 页面检查数据库备份、开机启动和 WebDAV 选项。

应用数据默认保存在：

```text
~/.cc-switch/cc-switch.db       # SQLite 主数据库
~/.cc-switch/settings.json       # 应用设置
~/.cc-switch/backups/            # 自动备份
```

### 3. WebDAV 云同步

WebDAV 同步位于 **设置 → 云同步**，支持以下操作：

- **测试连接**：只检查地址、认证和远端目录权限，不修改本地数据。
- **上传**：上传数据库配置快照和 `skills.zip`，并更新远端 manifest。
- **下载**：下载远端快照，校验文件大小和 SHA-256 后替换本地配置。
- **获取远端信息**：查看设备名、生成时间、快照版本和远端路径。

建议先在当前设备执行一次上传，再在其他设备执行下载。下载前程序会创建本地数据库备份；Provider、MCP、设置等共享配置会被远端快照覆盖，本地请求日志、用量汇总等设备专属数据会保留。

填写 WebDAV 时，基础地址应填写服务商提供的 URL（例如 `https://dav.example.com/remote.php/dav/files/user`），路径不要重复填写远端同步目录。用户名和密码保存后会在界面中脱敏显示。

#### 数据库版本兼容

本适配版本地运行时的数据库 Schema 上限为 10。针对 WebDAV 下载，导入器会将更新版快照（例如 `user_version=16`）按本地基线导入；快照中的完整表结构和数据仍会保留，导入前会自动备份。这个兼容处理只作用于 WebDAV SQL 导入，不代表本程序可以直接打开一个尚未经过同步导入的 v16 本地数据库。

修改 **应用数据目录** 后需要完全关闭并重新打开 CC Switch，新的目录才会生效。

### 4. 常见问题

#### 窗口白屏

ARM Mali 显卡可能不支持 WebKit 合成模式。本分支已在启动时自动设置 `WEBKIT_DISABLE_COMPOSITING_MODE=1`。如果仍然白屏，请先关闭所有 CC Switch 进程，再从终端启动并查看日志：

```bash
pkill -x cc-switch-web-server || true
WEBKIT_DISABLE_COMPOSITING_MODE=1 /usr/bin/cc-switch-web
```

#### 图标不显示

安装脚本会注册绝对路径图标并刷新桌面缓存。如果应用菜单仍显示旧图标，可执行：

```bash
sudo update-desktop-database /usr/share/applications
gtk-update-icon-cache -f -t /usr/share/icons/hicolor
```

必要时注销当前桌面会话后重新登录。

#### 仍然提示 `Unknown command`

这通常是旧后台进程仍在运行。完全退出窗口后执行：

```bash
pkill -x cc-switch-web-server || true
pkill -x cc-switch-web || true
```

然后重新从应用菜单启动。确认服务端口正常：

```bash
curl http://127.0.0.1:2891/health
```

#### 下载提示“数据库版本过新”

请确认已经安装本分支最新构建包，并完全重启应用。新版只在 WebDAV 下载导入路径处理远端高版本快照；如果错误来自应用启动阶段直接打开 v16 数据库，则需要使用对应的新版本程序，或先切换回兼容版生成的本地数据目录。

### 5. 从源码构建 ARM64 安装包

```bash
git clone https://github.com/greluoqixi/cc-switch-web.git
cd cc-switch-web
pnpm install
./build-deb.sh
sudo dpkg -i "./cc-switch-web_1.0.0_$(dpkg --print-architecture).deb"
```

也可以显式指定版本和架构：

```bash
CC_SWITCH_VERSION=1.0.0 CC_SWITCH_ARCH=arm64 ./build-deb.sh
```

构建脚本会依次构建 Vite 前端、`web-server` 后台、GTK 原生窗口，并把前端资源、桌面文件和图标一起装入 `.deb`。需要 Rust、Node.js/pnpm、`dpkg-deb`、GTK3 和 WebKit2GTK 4.0 开发包。

### 6. 本次适配的关键改动

- 用原生 GTK3 + WebKit2GTK 4.0 窗口替代 Tauri 2，兼容旧版 Linux 系统。
- 增加 Web-server 模式的配置目录和完整 WebDAV 命令路由。
- WebDAV 同步导入兼容高于本地 Schema 上限的远端 SQL 快照。
- ARM64 自动打包、绝对路径图标安装和桌面缓存刷新。
- 针对 Mali/WebKit 合成问题自动禁用合成模式并记录页面加载错误。

---

## 架构

```
┌─ 用户双击 ─────────────────────────────────┐
│  /usr/bin/cc-switch-web (native-shell)      │
│  ┌─ GTK3 窗口 ─────────────────────────┐   │
│  │  WebKit2GTK 4.0 WebView              │   │
│  │  → http://127.0.0.1:2891?native=1   │   │
│  │  (原生标题栏，无地址栏，可调整大小)    │   │
│  └──────────┬───────────────────────────┘   │
│             │ HTTP (回环)                   │
│  ┌──────────▼───────────────────────────┐   │
│  │  cc-switch-web-server (子进程)         │   │
│  │  127.0.0.1:2891                      │   │
│  └──────────────────────────────────────┘   │
│                                             │
│  打开窗口 → 启动服务 → 加载界面              │
│  关闭窗口 → 停止服务 → 结束进程              │
└─────────────────────────────────────────────┘
```

```
cc-switch/
├── web-server/              # HTTP 服务器入口
│   ├── Cargo.toml           # 依赖：cc_switch_lib + axum + tokio
│   └── src/main.rs          # REST API 服务器（~60+ 命令路由）
├── native-shell/            # 原生桌面窗口（独立 crate）
│   ├── Cargo.toml           # 依赖：web-view -> webkit2gtk-4.0
│   └── src/main.rs          # GTK+WebKit 窗口包装
├── src-tauri/               # 核心业务逻辑（原 Tauri 项目）
│   ├── Cargo.toml           # desktop feature 控制 Tauri 依赖
│   └── src/
│       ├── lib.rs           # 共享初始化逻辑 + 条件编译
│       ├── services/        # 全部业务逻辑（无 Tauri 依赖）
│       ├── database/        # SQLite DAO 层
│       ├── proxy/           # 代理服务器 + 故障转移
│       └── ...              # 各 CLI 工具配置模块
├── src/                     # 前端（React + TypeScript + Vite）
│   ├── lib/api/tauri-mock/  # Tauri API 的浏览器兼容 mock
│   └── ...
├── build-deb.sh             # .deb 打包脚本
└── vite.config.ts           # Vite 配置（含 Tauri mock 别名 + API proxy）
```

### 数据流

```
                    双击桌面图标
                         │
                    ┌────▼────┐
                    │ native  │  ← 启动子进程 cc-switch-web-server
                    │ -shell  │     打开 GTK+WebKit 窗口
                    └────┬────┘
                         │ HTTP (回环)
                   Axum REST API (端口 2891)  ← cc-switch-web-server
                         │
                    ┌────▼────┐
                    │ Services│  → ProviderService / McpService / ProxyService ...
                    └────┬────┘
                         │
                   ┌─────▼──────┐
                   │  SQLite DB │  ~/.cc-switch/cc-switch.db
                   └────────────┘
```

前端通过 Vite 别名将 `@tauri-apps/*` 导入重定向到浏览器兼容的 mock 模块，API 调用通过 `POST /api/invoke` 统一入口路由。

### 生命周期

- **打开窗口** → `native-shell` 启动 `cc-switch-web-server` 子进程 → 等待服务器就绪 → 打开 WebView 加载前端
- **使用** → 前端通过 HTTP API 与后端交互，所有业务逻辑在 `cc-switch-web-server` 进程中执行
- **关闭窗口** → `native-shell` 发送 shutdown 信号 → 终止子进程 → 完全退出（无残留进程）

---

## 常见问题

### Q: Skills 页面空白或显示"未安装任何 Skill"？

这是 web-server 模式的常见问题。修复方法：

1. **确认启动日志**：运行 `./run-web.sh` 后观察日志，看是否有 `✓ Auto imported N skill(s)` 或 `✓ Directly imported N skill(s)` 的输出。
2. **确认 SKILL.md 文件存在**：CC Switch 要求每个 Skill 目录下包含 `SKILL.md` 文件才能识别。
   ```bash
   ls ~/.claude/skills/*/SKILL.md
   ```
3. **手动导入**：在 Skills 页面点击右上角 **Import** 按钮，手动选择未管理的 Skill 导入。
4. **重启并强制重建**：如果仍有问题，可清空数据库后重启（注意备份）：
   ```bash
   # 关闭应用后执行
   mv ~/.cc-switch/cc-switch.db ~/.cc-switch/cc-switch.db.bak
   # 重新打开应用
   ```

### Q: 关于页面显示空白？

web-server 模式中工具版本检测（claude --version 等）不可用，返回空数组不影响正常功能。版本号从后端 health 接口动态获取。

### Q: 能否在 Ubuntu 22.04+ 上运行？
可以，原生窗口在任何安装有 `libwebkit2gtk-4.0-dev` 的 Linux 发行版上都能运行。

### Q: 能否同时保留 Tauri 桌面版构建？
可以。`cargo build -p cc-switch`（需 desktop feature）构建桌面版，`cargo build -p web-server` 和 `cargo build --manifest-path native-shell/Cargo.toml` 构建原生窗口版。两者互不干扰。

### Q: 数据文件在哪？
同原始版本：`~/.cc-switch/cc-switch.db`（SQLite 数据库）、`~/.cc-switch/settings.json`（设备设置）。

### Q: 数据是否与桌面版兼容？
核心配置和 WebDAV 快照格式保持兼容。适配版本地运行时支持的 Schema 上限为 10；更新版数据库请通过 WebDAV 下载导入，不要直接把 v16 等更高版本数据库文件放进本地数据目录。

### Q: SQL 导入/导出的文件保存在哪？

**浏览器模式**（`./run-web.sh` 启动）：
- **导入**：点击"选择SQL文件"会弹出浏览器文件选择器，从本地任意目录选择 `.sql` 文件
- **导出**：后端生成 SQL 后通过浏览器下载，文件保存在**浏览器的默认下载目录**（通常是 `~/Downloads/`）
- 文件名格式：`cc-switch-export-YYYYMMDD_HHMMSS.sql`

**原生窗口模式**（`native-shell` 启动）：
- 导入/导出使用 WebKit 下载功能，保存位置取决于 WebKit 的下载配置，建议在浏览器模式下操作以确保文件可定位。

---

> 原始 Tauri 2 桌面版由 [farion1231](https://github.com/farion1231/cc-switch) 开发，基于 MIT 许可证发布。
