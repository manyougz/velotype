# Velotype

<div align="center">

![Velotype banner](../assets/icon/velotype-banner.png)

**使用 Rust 与 GPUI 构建的原生 Markdown 即时渲染编辑器。**

[编辑器展示](./showcase/showcase.md)

[English](../README.md) | [中文](README.zh-CN.md)

[![Rust](https://img.shields.io/badge/Rust-2024-f74c00?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![GPUI](https://img.shields.io/badge/GUI-GPUI%200.2-4b7bec)](https://gpui.rs/)
[![Platforms](https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS-2ea44f)](#快速开始)
[![Portable](https://img.shields.io/badge/app-portable%20single%20binary-8b5cf6)](#特性)
[![Export](https://img.shields.io/badge/export-HTML%20%7C%20PDF-0ea5e9)](#特性)
[![Release](https://img.shields.io/badge/releases-GitHub-181717?logo=github)](https://github.com/manyougz/velotype/releases)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](../LICENSE)

</div>

Velotype 是一个基于 Rust 与 [GPUI](https://gpui.rs/) 的块级 Markdown 编辑器，支持所见即所得模式(WYSIWYG)和 Markdown 原文本编辑双模式。

项目仍处于早期阶段，但核心方向已经稳定：原生 UI、即时渲染编辑、源文本兜底、规范化 Markdown 序列化，以及覆盖颜色、字体、间距和布局 token 的自定义能力。

## 特性

- **🧱 Block 模型:** Markdown 结构会被表达为可编辑块，让文档结构清晰可控易扩展，无需预览窗同步循环。
- **⚡ 原生 UI:** 基于 GPUI 的桌面原生渲染，不依赖 Electron、Tauri 或任何 WebView 外壳。
- **✍️ 编辑模式:** Velotype 支持所见即所得模式(WYSIWYG)和原文本编辑两种模式，满足常见编辑场景需求。
- **🚀 性能与稳定:** Rust 驱动解析、状态更新和渲染；解析策略遵循标准导向，并在不稳定场景回退为 raw Markdown。
- **🎨 主题自定义:** 主题可自定义全局颜色、字体、间距、菜单、弹窗、编辑器布局 token 和语言包。
- **📦 便携单文件:** Velotype 编译后以单文件形式存在，无需安装，原生便携，支持 Windows、Linux 和 macOS 平台。

Velotype 已经支持将当前 Markdown 文档导出为 HTML 和 PDF。HTML 导出会把当前主题映射为 CSS，PDF 导出复用同一套主题化 HTML 管线，以保证视觉输出一致。

Velotype 目标支持 Windows、Linux 和 macOS。应用天然适合作为独立二进制文件分发，release 构建可以直接运行，无需安装。

## 快速开始

### 1. 下载 release

从 [Velotype Releases](https://github.com/manyougz/velotype/releases) 页面下载 Windows、Linux 或 macOS 构建。Release 产物按便携桌面程序设计，可直接运行。

### 2. 从源码构建

前置需求：

- Git
- 支持 Rust 2024 edition 的 Rust toolchain
- Cargo
- GPUI 与系统工具链所需的平台原生构建依赖

本地构建并运行 Velotype：

```bash
git clone https://github.com/manyougz/velotype.git
```

```bash
cargo build --release
```

如果一切正常，构建产物将在 `target/release` 下储存，取出可执行文件即可使用

## Roadmap

Velotype 目前已经支持几乎所有基础 Markdown 语法，以及绝大多数常见扩展 Markdown 语法，包括标题、段落、列表、任务列表、引用、callout、表格、代码块、行内格式、链接、reference-style 链接与图片、脚注、独立图片、注释块和安全的原生 HTML 处理。

语法支持仍会持续更新。仍在计划中的能力包括：

- [x] ~~脑图绘图与渲染支持~~
- [x] ~~LaTeX 数学语法与渲染支持~~
- [ ] 内置图床
- [ ] 更完善的 IME 功能

## 自定义主题 & 翻译

Velotype 将视觉主题和界面语言包拆分管理。主题文件可以覆盖全局颜色、字体、尺寸、菜单、弹窗、表格控件、图片占位、代码高亮颜色，以及布局相关 token。缺失字段或空值会继承内置 Velotype 默认值，因此自定义主题既可以非常小，也可以完整覆盖。

语言包也采用相同的局部配置策略。缺失文案会回退到英文，导入后的语言包会在写入应用配置目录前进行规范化。

可以从以下示例文件开始：

- [自定义主题 JSONC](custom-theme.example.jsonc)
- [自定义语言 JSONC](custom-language.example.jsonc)

在应用内使用 `主题 -> 添加主题配置` 或 `语言 -> 添加语言配置` 导入 `.json` / `.jsonc` 文件。JSONC 注释用于方便编写和分享示例；应用规范化保存后的配置文件会是严格 JSON。

> 十分感谢您帮助翻译 Velotype 或丰富 Velotype 主题生态！

## 架构

| Layer | Responsibility |
| --- | --- |
| `editor` | 窗口级编辑器状态：视图模式、保存/关闭流程、撤销、选择、source mapping、树结构变更、导出和文件拖拽。 |
| `components::block` | 可编辑块运行时、GPUI 输入处理、块渲染、块事件、图片/表格/代码块运行时状态。 |
| `components::markdown` | Markdown 数据模型，以及 inline、link、image、footnote、table、HTML、code highlight 的解析与序列化辅助逻辑。 |
| `export` | HTML 与 PDF 导出管线。 |
| `theme` | 视觉主题 token、内置主题默认值、自定义主题导入和全局主题管理。 |
| `i18n` | 内置 UI 文案、自定义语言包、系统 locale 匹配和运行时语言切换。 |
| `net` | 远程图片加载所需的 HTTP client 集成。 |

编辑器使用原生 block tree 作为运行时模型。导入时，稳定支持的 Markdown 会转换为结构化块；保存时，block tree 会重新序列化为规范化 Markdown。对于当前运行时不稳定支持的语法，Velotype 会保留原始源码，并保持其可见、可编辑。

## 贡献

这个仓库仍在快速迭代。报告解析或渲染问题时，请尽量提供：

- 最小 Markdown 输入；
- 期望的渲染或序列化结果；
- 实际结果；
- 问题发生的编辑模式。

提交代码变更时，建议优先提供小补丁，并基于现有 parser/runtime 模型扩展，而不是整体替换现有实现。

## 许可证

Velotype 使用 [Apache License 2.0](../LICENSE)。
