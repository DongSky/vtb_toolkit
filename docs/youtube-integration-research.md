# YouTube 接入与双平台直播评论网页：开源选型

核查日期：2026-09-12。本文记录选型依据与最终实现方向。依据 GitHub 仓库状态、发布记录、具体源码和 npm/crates.io 版本记录；匿名评论已完成真实验证，Google 授权/上传未实测。

用户要求：面向主播、场控、录播员与切片人，将已有 B 站工作流扩展到 YouTube；提供可单独打开、可添加到 OBS 等直播软件的网页评论窗口。YouTube 优先匿名读取，接受网页接口变化带来的维护成本，并优先复用仍在维护的开源实现。

## 选型结论

推荐 **YouTube.js 负责评论与频道信息，yt-dlp 负责取流，复用现有 vtb-overlay 提供网页输出**。

YouTube.js 的 npm 包名是 `youtubei.js`，当前正式版为 `18.0.0`。它具备匿名会话、实时评论、Super Chat、Super Sticker、会员与赠送会员解析，以及发送评论、消息删除/替换事件和 Studio 上传接口。读取公开评论不需要用户配置 Google API Key。发送和上传需要登录，且必须单独验证，不能由“存在方法”推断生产环境可靠性。

桌面运行方案采用 `youtubei.js/web` + Tauri Rust HTTP 适配：上游支持自定义 `fetch`，由 Rust 发出受限的 YouTube 请求，避开 WebView 的 CORS 和受限请求头问题。实际 Windows 验证通过，用户无需为评论功能额外安装 Node.js。

## 最近维护情况与候选比较

“最近维护”同时检查源码提交内容与发布版本，不能只看仓库的 `pushed_at`，也不能把依赖更新当作直播协议修复。

| 项目 | 核查到的维护证据 | 适用范围 | 结论 |
| --- | --- | --- | --- |
| [YouTube.js](https://github.com/LuanRT/YouTube.js) | 默认分支 2026-09-09 有解析器功能提交；v18.0.0 于 2026-08-13 发布，npm 对应版本同日发布 | 匿名评论、频道/视频、付费消息、会员、消息操作与登录后发送；浏览器/Node/Deno | 首选，MIT；覆盖范围最接近完整工作流 |
| [yt-dlp](https://github.com/yt-dlp/yt-dlp) | 2026-08-30 仍有源码提交；正式版 2026.08.19 | 取流、录制入口、直播与回放评论归档 | 保留当前录制基础；主项目 Unlicense，分发可执行文件时仍需核对其依赖许可 |
| [brainrot](https://github.com/pykeio/brainrot) | 2026-09-02 连续修复 YouTube 客户端信息、频道查询和 Signaler 事件过滤 | Rust 原生匿名评论、SC、会员、赠送会员、回放 | Apache-2.0；技术栈合适，但事件覆盖不全，仅作备选 |
| [Social Stream Ninja](https://github.com/steveseguin/social_stream) | `sources/youtube.js` 于 2026-09-08 提交源站删除消息同步改动 | 浏览器扩展/桌面采集、跨平台评论聚合、OBS 主题与互动 | 适合参考产品行为；GPL-3.0，不直接复制进当前 MIT workspace |
| [chat-downloader](https://github.com/xenova/chat-downloader) | 仓库最后推送 2025-11-05；最新正式版 v0.2.8 发布于 2023-09-03 | Python 匿名直播/回放评论下载 | MIT；可参考日志格式，不作为本次首选运行依赖 |
| [pytchat](https://github.com/taizan-hokuto/pytchat) | 原仓库已归档，最后推送及 v0.5.5 发布均在 2021-07-24 | Python 评论读取 | 不采用原仓库 |

额外查到的近期项目包括 [YTChatHub](https://github.com/yusufipk/YTChatHub)、[YTLiveChat](https://github.com/Agash/YTLiveChat) 和 [youtube-live-chat-downloader](https://github.com/abhinavxd/youtube-live-chat-downloader)。前者 2026-09-02 有 dashboard/overlay 重写，后两者分别包含 C# 实现和 Go 匿名读取实现；可参考，但没有比 YouTube.js 更适合本项目完整功能范围的明确优势。YTLiveChat 最近几次提交主要是构建/CI 依赖更新，不能据此认定评论协议近期被修复。

## 源码核查中的关键差异

YouTube.js 的 [`LiveChat.ts`](https://github.com/LuanRT/YouTube.js/blob/ef3afbe435edc86ba3357b6fa951548ee3d46053/src/parser/youtube/LiveChat.ts) 提供 `start`、`chat-update`、`metadata-update`、`error`、`end` 事件、评论筛选、重试和 `sendMessage`。上游示例显式演示文本、Super Chat 和付费贴纸；解析器还包括会员、会员礼赠和消息删除/替换等类型。核心 `LiveChat.ts` 最近提交时间是 2024-12-27，近期维护主要体现在更广泛的解析器和会话代码；应通过真实直播与固定样本测试确认当前可用性。

brainrot 在 2026-09-02 的三个 YouTube 修复是实质性更新，尤其值得保留为 Rust 备选。但核查 [`ChatEvent::from_action`](https://github.com/pykeio/brainrot/blob/2e9d38298dc287d4a6daff5f5d6c37a0a01e2f39/src/youtube/mod.rs) 发现：虽然底层声明了付费贴纸、删除和替换类型，对外转换只产出消息、会员和会员礼赠，其他分支被忽略；没有与 YouTube.js 相当的发送/上传接口。另外 crates.io 的 `0.3.0` 是 2026-02-22 发布的，**不含 9 月修复**；选择它时不能直接依赖 0.3.0 并声称用了最新协议。

yt-dlp 的 [`youtube_live_chat.py`](https://github.com/yt-dlp/yt-dlp/blob/bbc809a1161d3bfca51fa36f59dda35556ee85a0/yt_dlp/downloader/youtube_live_chat.py) 已处理 live/replay continuation、片段重试、服务端等待时间和逐行 JSON 动作。它适合归档与故障诊断；若用作实时界面主数据源，还需处理片段缓冲、文件尾读、取消与重连，维护界面交互不如直接消费评论事件方便。

认证方面，[YouTube.js 上游说明](https://github.com/LuanRT/YouTube.js/blob/ef3afbe435edc86ba3357b6fa951548ee3d46053/examples/auth/README.md) 明确表示旧 OAuth2 登录目前仅适用于 TV 客户端，已不推荐，建议 Cookie 登录。这里指该库的 YouTube TV 登录流程，不是 Google 官方 Data API OAuth 整体失效。公开评论默认匿名。最终实现不采用 Cookie 登录；投稿使用独立的官方桌面 OAuth，排除 YouTube 评论发送。

浏览器方面，上游 README 声明支持现代浏览器；`SessionOptions` 有 `fetch`、`retrieve_player` 等选项。[旧浏览器示例](https://github.com/LuanRT/YouTube.js/blob/ef3afbe435edc86ba3357b6fa951548ee3d46053/examples/browser/README.md) 已标记过时，但仍清楚说明需要代理请求，不能直接访问 YouTube。集成时应依据当前库接口验证 Tauri 适配，避免把过时播放器示例引入评论功能。

## 实现与验证

已采用 `youtubei.js/web` + Tauri Rust 受限 HTTP 适配，无需用户为评论功能安装 Node.js。轮询复用上游端点与解析器，由应用管理取消、去重、重连、预览/录制消费者和 WebView 重载恢复。Windows 已实际验证匿名评论及双平台独立网页显示。

YouTube 公开直播当前可能只提供分离音视频；yt-dlp 使用 `bestvideo+bestaudio/best` 选择格式，FFmpeg 显式映射两路输入并复用录制监督、分段及停止逻辑。频道监听固定到当前视频，避免录制中切到另一场直播。

平台身份、原始金额、会员文案、自定义表情、删除日志、打点、翻译、统计与 XML 导出已经接入共同工作流。OBS 使用本地固定端口、动态状态 API、独立透明页面、筛选及配置。功能使用、平台差异、依赖和验证限制详见 [使用说明](youtube-and-obs.md)。

按用户最终范围，**YouTube 评论发送、自动答谢、定时发言不实现**。上传采用 Google 官方 Data API 桌面 OAuth（PKCE、仅上传权限）及分块续传，凭据存于系统凭据库，默认私享；不采用上游 Cookie/Studio 上传。未执行真实账号授权或投稿。

## 可复核的维护证据

- YouTube.js：[最近核查的源码提交](https://github.com/LuanRT/YouTube.js/commit/ef3afbe435edc86ba3357b6fa951548ee3d46053)、[v18.0.0 发布](https://github.com/LuanRT/YouTube.js/releases/tag/v18.0.0)、[npm 包](https://www.npmjs.com/package/youtubei.js)、[直播评论示例](https://github.com/LuanRT/YouTube.js/blob/ef3afbe435edc86ba3357b6fa951548ee3d46053/examples/livechat/index.ts)。
- yt-dlp：[2026.08.19 发布](https://github.com/yt-dlp/yt-dlp/releases/tag/2026.08.19)、[2026-08-30 源码提交](https://github.com/yt-dlp/yt-dlp/commit/bbc809a1161d3bfca51fa36f59dda35556ee85a0)。
- brainrot：[Signaler 修复](https://github.com/pykeio/brainrot/commit/2e9d38298dc287d4a6daff5f5d6c37a0a01e2f39)、[客户端信息修复](https://github.com/pykeio/brainrot/commit/e6a237aaf4b87542260784d3e7a0aee7e78de136)、[频道查询修复](https://github.com/pykeio/brainrot/commit/6f99644ccffa3663bdfdc792a831ac3e88aa7834)、[crates.io 版本](https://crates.io/crates/brainrot/versions)。
- Social Stream Ninja：[2026-09-08 删除消息同步](https://github.com/steveseguin/social_stream/commit/833d92fa5989fb71ef2249a9aca4d8258a73c517)。
- chat-downloader：[正式发布记录](https://github.com/xenova/chat-downloader/releases)。
- pytchat：[原仓库与归档状态](https://github.com/taizan-hokuto/pytchat)。
