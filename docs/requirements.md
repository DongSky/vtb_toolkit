# VTB Toolkit — 需求文档

> 2026-09 补充：YouTube 优先匿名读取，接受网页协议维护成本；除评论发送外，录制、日志、打点、字幕、复盘与投稿按 B 站工作流对齐。Bilibili / YouTube 共用可单独打开的透明 OBS 网页，支持混合与独立直播筛选。当前实现与限制见 [使用指南](youtube-and-obs.md)，开源维护依据见 [选型记录](youtube-integration-research.md)。

> 面向虚拟主播（VTuber）及其切片/烤肉创作者的桌面工具合集，基于 Tauri 2（Rust 后端 + React 前端）。
> 本文档基于 2026-07 的互联网调研（blivedm 协议源码、录播姬/blrec/DDTV、blivechat/弹幕姬/LAPLACE Chat、Holodex、流式 ASR 与多模态方案）编写。

## 1. 背景与定义

- **虚拟主播（VTuber）**：以 Live2D/3D 虚拟形象进行直播的创作者，主阵地为 B 站直播与 YouTube。直播内容以杂谈、游戏、歌回（唱歌回）为主，观众互动强依赖弹幕、SC（醒目留言）、礼物与舰长（大航海）。
- **切片（Clip）/切片man**：从长直播录像（往往 2–6 小时）中截取精彩片段（highlight），加工成短视频二次传播的创作者。工作流：看录播 → 定位高能点 → 剪切 → （可选）加字幕 → 投稿。
- **烤肉man/字幕组**：切片基础上做翻译+字幕（日→中为主），"烤肉"即翻译加工生肉（无字幕外语视频）。工作流的痛点是听译和打轴（字幕时间对齐）。
- **高能点（Highlight）**：弹幕密度/礼物爆发、主播情绪激动（笑声、尖叫）、名场面。B 站官方"高能进度条"即基于弹幕密度。

## 2. 核心功能需求

### F1 弹幕实时显示（vtb-danmaku + 前端）
- 连接 B 站直播间弹幕服务器（blivedm 协议：16 字节包头、brotli 压缩、WBI 签名握手、30s 心跳）。
- 事件类型：弹幕、SC、礼物、舰长购买、进场、点赞、看过人数、开播/下播。
- 多种内置显示主题（如：简洁滚动、气泡、透明 OBS 叠加层、经典弹幕姬风格）。
- **自定义主题**：用户可通过主题编辑器（CSS 变量 + 可视化画板）自定义配色、字体、动画、头像/粉丝牌显示；主题可导入导出 JSON。
- 弹幕过滤：关键词黑名单、等级/粉丝牌门槛、礼物金额门槛。
- 作为独立窗口（可置顶、点击穿透）供 OBS 捕捉，或提供本地 HTTP 页面作 browser source。

### F2 自动录制（vtb-recorder）
- 监听：轮询 room_init（10–30s）+ 弹幕流 LIVE/PREPARING 事件双通道，开播即启动录制。
- 取流：getRoomPlayInfo（qn 10000 原画需登录 cookie；FLV 优先，fmp4/HLS 备选），Referer+UA 伪装，URL 过期自动重取。
- 录制：ffmpeg `-c copy` 无损拉流；支持按时长（`-f segment`）与按大小切分；FLV/TS 容器保证断电安全。
- 关播后自动导出：合并/整理分段文件，写 `*.meta.json`（房间、标题、起止时间、分段列表），可选自动转 MP4、自动触发离线处理管线（F6）。
- 同时录制弹幕流到 JSONL/XML（与录播姬兼容格式可后续加），供 highlight 分析与弹幕回放。

### F3 实时语音转文字（vtb-asr）
- 中英日多语言，自动语种检测；支持混说场景。
- 本地推理优先：whisper 系（whisper.cpp / sherpa-onnx 流式 zipformer / SenseVoice）+ silero-VAD 分句；Rust 侧经 whisper-rs / sherpa-rs 集成。
- 云 API 备选（OpenAI/Deepgram 等），可在设置中切换。
- 音频源：系统音频/麦克风采集（主播模式）或直播拉流音频（录制者模式）。
- 输出带时间戳的 TranscriptSegment 流（partial + final），实时字幕窗口显示。

### F4 实时同传翻译（vtb-translate）
- 流式翻译管线：对 final 的 ASR 分段做增量翻译，目标语言可配置（中↔日↔英）。
- LLM API 可插拔（Claude/OpenAI/Gemini/DeepL）。
- **个性化翻译**：用户提供术语表（人名/梗/专有名词固定译法）+ 参考翻译样例（few-shot 风格模仿），组装进 system prompt；支持按主播建档。
- 翻译结果与原文对照显示，可推送到弹幕显示窗口（同传字幕条）。

### F5 Highlight 检测与自动切片（vtb-highlight）
- 信号融合打分（滑动窗口）：
  1. 弹幕密度峰值（z-score）与情感/关键词（草/哈哈哈/？？？/泪目/名场面）；
  2. 礼物/SC 爆发；
  3. 音频能量与事件（笑声、尖叫、长时间高能）；
  4. 语义 LLM：分块分析带绝对时间戳的 ASR 文本，不依赖观众规模发现完整的铺垫、笑点、反转和情绪段落；
  5. 视觉 LLM：录制完成后对全片稀疏抽帧，结合对应转录发现精彩操作、反应和视觉梗；
  6. 候选融合：分别保留规则、语义和画面分数，添加可配置的前后冗余，合并重叠候选并限制最大输出数量；
  7. 可选多模态 LLM 复核：对最终候选再次抽帧打分并生成切片标题。
- 输出 Highlight 列表（起止、分数、原因、建议标题），一键 ffmpeg `-c copy` 切片导出。
- 实时模式（直播中滚动检测并提示）与离线模式（对完整录播全量分析）。

### F6 离线处理管线（vtb-pipeline）
- 输入完整录播文件：抽音频 → ASR → 翻译 → 字幕导出（SRT/ASS 双语）→ highlight 检测 → 批量切片。
- 各阶段可单独运行/重跑，进度可视化，任务队列持久化。

## 3. Brainstorm 增补功能（来自竞品调研）

| 功能 | 来源启发 | 优先级 |
|---|---|---|
| SC/舰长记录册（感谢名单导出） | 弹幕姬插件生态 | P1 |
| 弹幕词云与直播数据报告（弹幕数/互动率/营收统计） | LAPLACE Chat | P1 |
| 歌回歌切辅助（按歌曲边界切分+曲名识别） | Holodex Musicdex | P2 |
| 弹幕高能进度条（录播回放时叠加密度曲线） | B站高能进度条 | P1（highlight 副产品） |
| 双语字幕烧录导出（切片直接带字幕） | 烤肉man工作流 | P1 |
| OBS WebSocket 联动（开播自动切场景/录制状态提示） | blivechat/OBS 生态 | P2 |
| 弹幕 TTS 朗读 | LAPLACE Chat | P2 |
| 多房间同时监控/录制 | DDTV/blrec | P1 |
| 直播事件时间轴（SC/舰长/开播事件标记在录播时间轴上） | 切片man定位痛点 | P1 |
| 剪辑软件工程导出（Premiere/达芬奇 XML 标记点） | AI clipping 产品 | P3 |
| YouTube/Twitch 平台扩展 | Holodex | P3 |

## 4. 非功能需求

- **测试**：每个 crate 单元测试 + 集成测试；协议/解析层用真实样本数据；网络集成测试标 `#[ignore]` 手动跑；前端 Vitest。
- **可靠性**：录制进程崩溃恢复、断流重连、磁盘空间检查。
- **合规**：仅用公开 API；提示用户遵守平台条款；登录态（cookie）本地加密存储，绝不上传。
- **性能**：弹幕渲染 60fps（虚拟列表）；ASR 延迟 < 2s（流式）；录制 CPU 占用低（stream copy）。

## 5. 技术架构

```
Tauri 2 App
├── 前端 React + TS（弹幕渲染/主题编辑器/任务面板/设置）
│     └── Tauri events ← 后端事件流
└── Rust workspace
    ├── vtb-common     共享领域类型（LiveEvent/Transcript/Highlight...）
    ├── vtb-danmaku    B站弹幕客户端（协议/WBI/流式客户端）✅
    ├── vtb-recorder   开播监听+ffmpeg录制+切分+导出 ✅(核心)
    ├── vtb-asr        流式/离线 ASR（whisper.cpp/sherpa + VAD）
    ├── vtb-translate  LLM 同传 + 术语表/参考翻译个性化
    ├── vtb-highlight  多信号融合 highlight 检测 + 切片
    └── vtb-pipeline   离线批处理管线编排
```
