<div align="center">

<img src="src-tauri/icons/128x128@2x.png" width="112" alt="GuAsk logo" />

# GuAsk

**轻量的 Windows 划词翻译 / AI 提问 / 截图提问工具**

选中任意文字即可翻译或提问，框选屏幕即可让 AI 看图作答——不切窗口，不打断思路。

[![Release](https://img.shields.io/github/v/release/xiaoguzuiniu/GuAsk?style=flat-square&color=3b82f6)](https://github.com/xiaoguzuiniu/GuAsk/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/xiaoguzuiniu/GuAsk/total?style=flat-square&color=10b981)](https://github.com/xiaoguzuiniu/GuAsk/releases)
[![Build](https://img.shields.io/github/actions/workflow/status/xiaoguzuiniu/GuAsk/release.yml?style=flat-square&label=build)](https://github.com/xiaoguzuiniu/GuAsk/actions)
![Platform](https://img.shields.io/badge/platform-Windows%2010%20%7C%2011-0078d4?style=flat-square)
![Tauri](https://img.shields.io/badge/Tauri-2-24c8db?style=flat-square&logo=tauri&logoColor=white)
![Rust](https://img.shields.io/badge/Rust-stable-dea584?style=flat-square&logo=rust&logoColor=white)

[下载](#-下载安装) · [功能](#-功能特性) · [快速上手](#-快速上手) · [配置](#%EF%B8%8F-配置说明) · [从源码构建](#-从源码构建)

</div>

---

## ✨ 功能特性

- **划词即译**：在任意软件中选中文字，双击 Ctrl（可配置）即弹出翻译；外文自动译为中文，单个单词给出音标、释义与例句。
- **AI 提问与追问**：基于选中内容直接提问，支持多轮连续追问，回答以 Markdown 流式渲染。
- **截图提问**：`Alt+W` 框选屏幕任意区域，把截图连同问题一起发给多模态模型。
  - **多张截图**：同一个问题可附带多张截图。
  - **长截图**：框选区域后自行滚动，自动识别重叠并拼接，适合聊天记录、长网页、长代码。
- **程序员模式**：自动识别代码、报错和日志，切换到代码解释模式，说明原因并给出修复建议。
- **快捷提问**：`Alt+Q` 不选中文字也能直接唤出提问窗口。
- **会话与历史**：每种模式独立保留会话；历史记录可搜索，并可在弹窗中恢复继续追问。
- **多模型切换**：兼容所有 OpenAI 接口格式的服务（OpenAI、DeepSeek、通义千问、Ollama、各类中转等），提问时可随时切换模型。
- **轻量常驻**：基于 Tauri 2 + Rust，安装包约 3 MB，常驻托盘，资源占用低。
- **自动更新**：安装版启动后自动检测 GitHub Releases 上的新版本，一键升级。

## 📦 下载安装

前往 [Releases](https://github.com/xiaoguzuiniu/GuAsk/releases/latest) 下载最新版本：

| 文件 | 说明 |
| --- | --- |
| `GuAsk_x.y.z_x64-setup.exe` | **安装版（推荐）**，支持自动更新，无需管理员权限 |
| `GuAsk_x.y.z_x64_portable.exe` | **绿色版**，免安装，双击即可运行；不支持自动更新 |

> **系统要求**：Windows 10 / 11（x64），需要 [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)（Windows 11 及较新的 Windows 10 已内置）。

> [!NOTE]
> 安装包暂未进行代码签名，部分安全软件可能会拦截。请在安全软件中将其加入信任，或改用绿色版。

## 🚀 快速上手

1. 运行 GuAsk，程序会常驻系统托盘。**左键单击托盘图标**打开设置。
2. 填写 **接口地址（Base URL）** 与 **API Key**，点击「获取模型」后选择模型并保存。
3. 在任意位置选中文字，**双击 Ctrl**，即可看到翻译或提问窗口。

### 默认快捷键

| 操作 | 快捷键 |
| --- | --- |
| 划词后唤出翻译 / 提问 | 双击 `Ctrl`（可改为双击 Alt / Shift、显示小图标或自动弹出） |
| 快捷提问（无需划词） | `Alt + Q` |
| 截图提问 | `Alt + W` |
| 切换普通截图 / 长截图 | 截图时按 `Tab` |
| 完成长截图 | `Enter` 或点击「完成」 |
| 取消截图 | `Esc` 或鼠标右键 |
| 发送 / 换行 | `Enter` / `Shift + Enter` |

### 托盘菜单

右键托盘图标：**启用划词**、**开机自启**、**快速提问**、**截图提问**、**退出**。

## ⚙️ 配置说明

| 配置项 | 说明 |
| --- | --- |
| 接口地址 / API Key | 任意兼容 OpenAI `/v1/chat/completions` 的服务 |
| 模型 | 翻译与提问可分别选用；截图提问需选用支持图片输入的模型（如 GPT-4o、Claude、Qwen-VL 等） |
| 触发方式 | 双击 Ctrl / Alt / Shift、划词小图标、划词自动弹出 |
| 目标语言 | 翻译的目标语言，默认简体中文 |
| 提示词 | 翻译、单词、提问、代码解释的系统提示词均可自定义 |
| 历史记录 | 可开启或关闭，支持搜索与清空 |

配置文件位于 `%APPDATA%\com.gu.guask\config.json`。API Key 仅保存在本机，不会上传到任何第三方。

## 🛠 从源码构建

**环境要求**：[Rust](https://www.rust-lang.org/tools/install)（stable）、[Node.js](https://nodejs.org/)（用于运行 Tauri CLI）、Visual Studio C++ 生成工具。

```powershell
git clone https://github.com/xiaoguzuiniu/GuAsk.git
cd GuAsk/src-tauri

# 开发运行
cargo run

# 打包发布版（输出至 src-tauri/target/release/bundle/nsis）
cd ..
npx @tauri-apps/cli@2 build
```

### 项目结构

```text
GuAsk
├─ src-tauri/            Rust 后端（Tauri 2）
│  ├─ src/
│  │  ├─ main.rs         入口、托盘、全局快捷键、命令注册
│  │  ├─ selection.rs    全局鼠标/键盘监听、划词获取、弹窗定位
│  │  ├─ shot.rs         截图、长截图拼接
│  │  ├─ llm.rs          OpenAI 兼容接口与流式输出
│  │  ├─ history.rs      历史记录存储
│  │  └─ config.rs       配置读写
│  └─ tauri.conf.json
├─ ui/                   前端（原生 HTML / CSS / JS）
│  ├─ popup.html/js      翻译与提问弹窗
│  ├─ shot.html          截图遮罩
│  └─ settings.html      设置页
└─ .github/workflows/    GitHub Actions 自动构建与发布
```

### 发布新版本

1. 同步修改 `src-tauri/tauri.conf.json` 与 `src-tauri/Cargo.toml` 中的版本号。
2. 提交后推送标签：`git tag v0.2.0 && git push origin v0.2.0`。
3. GitHub Actions 将自动构建安装版与绿色版、生成更新签名，并发布到 Releases。

> 自动更新依赖仓库 Secret `TAURI_SIGNING_PRIVATE_KEY`（更新签名私钥）。

## 🗺 路线图

- [x] 划词翻译与 AI 提问
- [x] 程序员模式
- [x] 截图提问、多图、长截图
- [x] 自动更新与 CI 发布
- [ ] API Key 加密存储（Windows 凭据管理器）
- [ ] OCR 文字识别
- [ ] 代码签名

## 🤝 参与贡献

欢迎通过 [Issues](https://github.com/xiaoguzuiniu/GuAsk/issues) 反馈问题或提出建议，也欢迎提交 Pull Request。

## 🙏 致谢

- [Tauri](https://tauri.app/)：跨平台桌面应用框架
- [rdev](https://github.com/Narsil/rdev)：全局输入事件监听

## 📄 许可证

本项目基于 [MIT License](LICENSE) 开源。
