# VTB Toolkit

面向虚拟主播（VTuber）与切片/烤肉创作者的桌面工具合集，基于 **Tauri 2**（Rust 后端 + React 前端）。

界面支持 **简体中文 / English / 日本語**，顶部即时切换并自动保存。
完整图文教程：[离线三语网页](docs/user-guide.html) · [中文](docs/user-guide.zh-CN.md) · [English](docs/user-guide.en.md) · [日本語](docs/user-guide.ja.md)。也可在软件顶部点击「使用教程」。

## 功能

| 模块 | 说明 |
|---|---|
| 🗨️ 弹幕实时显示 | 连接 B 站直播间（blivedm 协议 + WBI 签名），弹幕/SC/礼物/舰长实时渲染；3 套内置主题 + 可视化自定义主题编辑器（CSS 变量调色、导入导出 JSON） |
| ⏺️ 自动录制 | 开播监听（轮询 room_init）→ 自动取流（playurl，原画 FLV 优先）→ ffmpeg 无损拉流录制；支持按时长/大小切分；关播自动优雅停止并导出 `*.meta.json` 会话清单 |
| 🎙️ 语音转文字 | whisper.cpp 本地推理（macOS Metal），中英日多语言 + 自动语种检测；能量 VAD 分句；实时流式与离线整档两种模式 |
| 🌐 实时同传 | LLM 流式翻译（Anthropic / OpenAI 兼容端点可插拔）；**个性化**：按主播建档的术语表（固定译名）+ 参考翻译（风格 few-shot）+ 滚动上下文 |
| ✂️ Highlight 检测与切片 | 四信号 z-score 融合 + 面向低热度直播的 ASR 语义粗剪与全片抽帧分析；Anthropic / OpenAI 兼容视觉模型可插拔，候选自动补前后文、去重后交给 ffmpeg 切片 |
| 📦 离线处理管线 | 给定完整录播：抽音频 → ASR → 翻译 → SRT/双语 ASS 字幕 → highlight → 批量切片，全程进度事件 |

## 架构

YouTube 已接入匿名直播评论、频道收藏与监听录制、SC/会员、评论日志、打点、字幕和复盘。双平台透明评论网页可单独打开或添加为 OBS 浏览器源；投稿使用独立的 Google OAuth，不包含 YouTube 评论发送。使用与 Windows 环境说明见 [YouTube 与 OBS 指南](docs/youtube-and-obs.md)。

```
Tauri 2 App（src-tauri：命令 + 事件桥）
├── React 前端（src/：弹幕渲染、主题编辑器、录制/离线面板）
└── Rust workspace（crates/）
    ├── vtb-common     共享领域类型
    ├── vtb-danmaku    B站弹幕客户端（16字节帧协议、brotli/zlib、WBI 签名、wss 客户端）
    ├── vtb-recorder   开播监听 + ffmpeg 录制/切分 + AutoRecorder 编排
    ├── vtb-asr        VAD + whisper.cpp（trait 可插拔）
    ├── vtb-translate  术语表/参考翻译个性化 + LLM 后端
    ├── vtb-highlight  多信号融合检测 + 切片 + 多模态复核
    └── vtb-pipeline   离线批处理编排（字幕导出/能量分析/弹幕日志）
```

## 开发

依赖：Rust ≥1.85、Node ≥20、ffmpeg（PATH 中）。

```bash
npm install
npm run tauri dev          # 启动开发版应用
npm test                   # 前端 Vitest
npm run test:rust          # Rust workspace

# 网络/模型相关集成测试（手动执行）：
cargo test -p vtb-danmaku --test live_integration -- --ignored   # 真实B站连接
cargo test -p vtb-asr --test whisper_integration -- --ignored    # 下载 tiny 模型 + macOS say 语音
```

whisper 模型：从 [ggerganov/whisper.cpp](https://huggingface.co/ggerganov/whisper.cpp) 下载 `ggml-*.bin`（建议 `small`/`medium` 平衡中英日效果），在“离线处理”面板填入路径。

## 构建安装包

构建前请安装 Rust ≥1.85、Node.js ≥20，以及 [Tauri 2 对应平台的系统依赖](https://v2.tauri.app/start/prerequisites/)。macOS 最低支持 10.15。脚本会用 `npm ci` 安装锁定的前端依赖，并生成当前平台的安装包：

```bash
# Linux（deb + AppImage）
./scripts/build-linux.sh

# macOS（app + dmg）
./scripts/build-macos.sh
```

```powershell
# Windows PowerShell（NSIS exe + MSI）
pwsh -File scripts/build-windows.ps1
```

脚本会先清理上一次的 `target/release/bundle/`，新产物也位于该目录。推送到 `main`、提交 Pull Request 或手动运行 `Build` workflow 时，GitHub Actions 会先扫描凭据泄露，再并行构建 Windows、macOS 和 Linux 版本；安装包可从该次 workflow 的 Artifacts 下载。

## 凭据安全

- 不要提交 `.env`、私钥、签名证书或本地包管理器凭据；这些文件已由 `.gitignore` 排除。
- B 站登录态和持久化的 LLM API Key 使用系统钥匙串（macOS Keychain、Windows Credential Manager、Linux Secret Service）。
- 文本、视觉与图片生成在“设置 → AI 服务”分别配置；API Key 按用途、服务类型和地址隔离，保存于系统凭据库，不随安装包分发。视觉默认复用文本配置。
- CI 会使用 Gitleaks 扫描完整 Git 历史，防止凭据随提交进入仓库。

## 说明与合规

- 弹幕匿名连接时 B 站会对用户名打码（uid=0）；完整信息需自行提供登录 Cookie（本地保存，不上传）。
- 原画画质取流需要登录 Cookie；匿名默认取可用最高档。
- 请遵守直播平台服务条款与主播的二创/切片授权规则；商业用途请先获得授权。

## 文档

- [YouTube 与 OBS 使用指南](docs/youtube-and-obs.md)
- [YouTube 开源选型依据](docs/youtube-integration-research.md)

- [已有功能总览](docs/features.md) — 当前可用能力、完整工作流、输出文件与已知限制
- [需求文档](docs/requirements.md) — 功能定义、竞品调研、brainstorm
