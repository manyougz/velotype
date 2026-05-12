# Velotype

<div align="center">

![Velotype banner](./assets/icon/velotype-banner.png)

**A Rust + GPUI native Markdown editor with WYSIWYG and source editing modes.** 

[Editor Showcase](./assets/showcase/showcase.md)

[English](README.md) | [中文](assets/README.zh-CN.md)

[![Rust](https://img.shields.io/badge/Rust-2024-f74c00?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![GPUI](https://img.shields.io/badge/GUI-GPUI%200.2-4b7bec)](https://gpui.rs/)
[![Platforms](https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS-2ea44f)](#quick-start)
[![Portable](https://img.shields.io/badge/app-portable%20single%20binary-8b5cf6)](#features)
[![Export](https://img.shields.io/badge/export-HTML%20%7C%20PDF-0ea5e9)](#features)
[![Release](https://img.shields.io/badge/releases-GitHub-181717?logo=github)](https://github.com/manyougz/velotype/releases)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

</div>

Velotype is a block-based Markdown editor built with Rust and [GPUI](https://gpui.rs/). It supports both WYSIWYG-style rendered editing and Markdown source-text editing.

The project is still early, but the core direction is stable: native UI, instant rendered editing, source-text fallback, canonical Markdown serialization, and customization across color, typography, spacing, and layout tokens.

## Features

- **🧱 Block model:** Markdown structure is represented as editable blocks, keeping document structure clear, controllable, and extensible without a preview-pane synchronization loop.
- **⚡ Native UI:** Desktop-native rendering based on GPUI, without depending on Electron, Tauri, or any WebView shell.
- **✍️ Editing modes:** Velotype supports both WYSIWYG-style rendered editing and raw Markdown source editing for common authoring workflows.
- **🚀 Performance and stability:** Rust drives parsing, state updates, and rendering; the parser follows a standard-oriented strategy and falls back to raw Markdown in unstable cases.
- **🎨 Theme customization:** Themes can customize global colors, typography, spacing, menus, dialogs, editor layout tokens, and language packs.
- **📦 Portable single file:** After compilation, Velotype exists as a single executable file. It requires no installation, stays natively portable, and targets Windows, Linux, and macOS.

Velotype already supports exporting the current Markdown document to HTML and PDF. HTML export maps the active theme into CSS, while PDF export reuses the same themed HTML pipeline so visual output stays consistent.

Velotype targets Windows, Linux, and macOS. The app is naturally suitable for distribution as a standalone binary; release builds can run directly without installation.

## Quick Start

### 1. Download a release

Download the Windows, Linux, or macOS build from the [Velotype Releases](https://github.com/manyougz/velotype/releases) page. Release artifacts are designed as portable desktop programs and can be run directly.

> Note: macOS users need to sign the `app` file locally on their machine; otherwise, Velotype will not run properly.

### 2. Build from source

Prerequisites:

- Git
- A Rust toolchain with Rust 2024 edition support
- Cargo
- Platform-native build dependencies required by GPUI and the system toolchain

Build Velotype locally:

```bash
git clone https://github.com/manyougz/velotype.git
```

```bash
cargo build --release
```

If everything works, the build artifact will be stored under `target/release`. You can use the executable directly.

## Roadmap

Velotype already supports almost all basic Markdown syntax and most commonly used extended Markdown syntax, including headings, paragraphs, lists, task lists, quotes, callouts, tables, code blocks, inline formatting, links, reference-style links and images, footnotes, standalone images, comment blocks, and safe native HTML handling.

Syntax support will continue to improve. Planned work includes:

- [x] ~~Mind-map drawing and rendering support~~
- [x] ~~LaTeX math syntax and rendering support~~
- [ ] Workspace Mode and Outline Parsing
- [ ] Built-in image hosting
- [ ] More complete IME behavior

## Theme Customization & Translation

Velotype separates visual themes from UI language packs. Theme files can override global colors, typography, dimensions, menus, dialogs, table controls, image placeholders, code highlighting colors, and layout-related tokens. Missing fields or empty values inherit the built-in Velotype defaults, so a custom theme can be very small or fully override the default look.

Language packs use the same partial-configuration strategy. Missing strings fall back to English, and imported language packs are normalized before being written into the app configuration directory.

Start with the example files:

- [Custom theme JSONC](assets/custom-theme.example.jsonc)
- [Custom language JSONC](assets/custom-language.example.jsonc)

In the app, use `Theme -> Add Theme Config` or `Language -> Add Language Config` to import a `.json` or `.jsonc` file. JSONC comments are accepted for writing and sharing examples; normalized configuration files saved by the app are strict JSON.

> Thank you for helping translate Velotype or enrich the Velotype theme ecosystem.

## Architecture

| Layer | Responsibility |
| --- | --- |
| `editor` | Window-level editor state: view mode, save/close flow, undo, selection, source mapping, tree mutation, export, and file drop. |
| `components::block` | Editable block runtime, GPUI input handling, block rendering, block events, image/table/code runtime state. |
| `components::markdown` | Markdown data models and parse/serialize helpers for inline text, links, images, footnotes, tables, HTML, and code highlighting. |
| `export` | HTML and PDF export pipelines. |
| `theme` | Visual theme tokens, built-in theme defaults, imported custom themes, and the global theme manager. |
| `i18n` | Built-in UI strings, imported language packs, system locale matching, and runtime language selection. |
| `net` | HTTP client integration for remote image loading. |

The editor uses a native block tree as its runtime model. During import, stable supported Markdown is converted into structured blocks; during save, the block tree is serialized back into canonical Markdown. For syntax that is not stable enough in the current runtime, Velotype preserves the original source and keeps it visible and editable.

## Contributing

This repository is moving quickly. When reporting a parsing or rendering issue, please include:

- the minimal Markdown input,
- the expected rendered or serialized result,
- the actual result,
- the editing mode where it happens.

For code changes, prefer small patches that extend the existing parser/runtime model instead of replacing the current implementation wholesale.

## License

Velotype is licensed under the [Apache License 2.0](LICENSE).
