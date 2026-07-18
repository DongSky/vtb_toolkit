# VTB Toolkit — 功能差距分析与下一阶段开发计划

> 基于 2026-07 竞品调研(弹幕姬 / blivechat / LAPLACE Chat / BililiveRecorder / blrec / DDTV / Holodex / bilive / Opus Clip)与真实直播间(320、24158116)调试结论。

## 一、与竞品的功能差距

### 1.1 弹幕侧(对标:弹幕姬、blivechat、LAPLACE Chat)

| 缺失功能 | 竞品参照 | 影响 | 优先级 |
|---|---|---|---|
| **登录态(SESSDATA/buvid3 Cookie)** | 所有竞品 | 匿名连接用户名打码(`星***`)、uid=0,弹幕数据价值大减;取流拿不到原画 | **P0** |
| **OBS 浏览器源输出** | blivechat 的核心形态 | 主播无法把弹幕/同传字幕叠加进直播画面,当前只有桌面窗口 | **P0** |
| 弹幕/SC 自动翻译显示 | blivechat(译成日语给海外观众) | 翻译管线已有,但未接到弹幕显示 | P1 |
| SC/舰长记录簿 + 答谢清单导出 | LAPLACE、弹幕姬插件 | 主播下播答谢的刚需 | P1 |
| 弹幕统计报告(词云/互动率/营收) | LAPLACE Chat | 数据复盘 | P1 |
| 弹幕 TTS 朗读 | LAPLACE Chat | 单人主播不看屏幕也能听弹幕 | P2 |
| 弹幕发送(需登录) | DDTV | 场控操作 | P2 |
| 用户备注/历史(粉丝画像) | LAPLACE Chat | 认出老观众 | P3 |

### 1.2 录制侧(对标:录播姬、blrec、DDTV)

| 缺失功能 | 竞品参照 | 影响 | 优先级 |
|---|---|---|---|
| **断流重连+重新取流** | blrec"无缝拼接" | 流 URL 有效期短,当前 ffmpeg `-reconnect` 对过期 URL 无效,断流即停 | **P0** |
| **弹幕与视频联动录制** | 录播姬弹幕XML、DDTV | 当前 App 内录制不落弹幕日志(diag 才有),离线 highlight 无弹幕信号 | **P0** |
| FLV 时间戳修复 | 录播姬/blrec 核心卖点 | B站流时间戳跳变常见,不修复则切片/字幕对不齐 | P1 |
| 磁盘空间管理(不足自动清理/停录) | blrec | 无人值守录制安全网 | P1 |
| 事件通知(Bark/Telegram/ServerChan) | blrec | 开播/录完/出错推送到手机 | P1 |
| 多房间同时监控 UI | 所有录制竞品 | 后端支持,前端只有单房间表单 | P1 |
| 关播自动转 MP4 / 弹幕转 ASS | 录播姬工具箱 | 录完即可看、可投稿(修播放器兼容性时已在 diag 做了 remux,产品路径未接) | P1 |
| Webhook / REST API | 录播姬 GraphQL+REST | 与外部工作流集成 | P2 |
| 流参数变化自动分割、备线切换 | blrec / DDTV | 长时间录制稳定性 | P2 |

### 1.3 字幕/同传侧(对标:Holodex TLdex)

| 缺失功能 | 影响 | 优先级 |
|---|---|---|
| 同传字幕 OBS 叠加条 | 同传结果只在 App 内,观众看不到 | **P0**(与 OBS 输出同一基建) |
| 字幕历史存档/导出 | 实时字幕不落盘,下播即失 | P1 |
| 更高质量 ASR 模型管理(下载/切换 small/medium/large) | tiny 模型繁简混杂、幻觉多([BLANK_AUDIO]) | P1 |

### 1.4 切片侧(对标:bilive、danmaku_tools、Opus Clip)

| 缺失功能 | 影响 | 优先级 |
|---|---|---|
| 高能进度条(弹幕密度曲线叠加在录播时间轴) | 切片man 找点效率核心工具 | P1 |
| 多模态 LLM 标题/复核接入管线与 UI | 代码已有(`multimodal.rs`)但完全没接线 | P1 |
| 字幕烧录切片(切片直接带双语字幕) | 烤肉man 一步出成品 | P1 |
| 实时 highlight 提示(直播中滚动检测) | 只有离线模式,直播时无法即时标记名场面 | P2 |
| 歌切模式(按歌曲边界切分) | 歌回场景 | P2 |
| biliup 自动投稿 | bilive 全自动闭环 | P3 |

## 二、现有实现的改进点(真实调试暴露)

1. **弹幕客户端无重连**:连接断开就结束,没有 host 列表轮询 failover、指数退避重连、token 过期重新握手。竞品都把"断线重连、一条不丢"当基本功。
2. **AutoRecorder 无健康监控**:ffmpeg 死了/码率归零不会被发现;黑屏无声问题(HEVC bug)靠人发现——诊断工具里的自检(编码/亮度/音频RMS)应进产品路径。
3. **ASR 幻觉过滤**:whisper 在纯音乐/静音段输出 `[BLANK_AUDIO]`、`[clears throat]` 等标注甚至幻觉句,进入字幕与翻译(浪费 API);需按规则过滤+置信度门限。
4. **翻译串行延迟**:逐段阻塞调用,13 段翻译耗时明显;应并发窗口(保序)+失败重试+流式输出。
5. **配置不持久化**:房间号/输出目录/模型路径/API key 每次重填;API key 明文——需 tauri-plugin-store + 系统钥匙串。
6. **实时字幕依赖独立拉流**:与录制各拉一路流,浪费带宽;录制中应从同一路流分流音频。
7. **highlight 窗口参数固定**:10s 窗口/权重不可调,不同直播类型(游戏/杂谈/歌回)最优参数不同。
8. **前端整体较素**:无房间信息卡(标题/封面/在线人数)、无录播文件管理页、无任务历史。

## 三、下一阶段开发计划

### Phase 1 — 地基补强(P0,先做)

**M1.1 账号登录与 Cookie 管理**
- 方案:扫码登录(`passport.bilibili.com/x/passport-login/web/qrcode/generate` + poll),SESSDATA/buvid3/refresh_token 存系统钥匙串(keyring crate);`vtb-danmaku`/`vtb-recorder` 的 reqwest Client 注入 cookie_store;弹幕 auth 包带真实 uid。
- 验收:弹幕显示完整用户名;取流可拿 qn=10000 原画。

**M1.2 弹幕客户端可靠性**
- 方案:`DanmakuClient` 增加重连循环——host_list 轮询、指数退避(1s→60s)、认证失败(-101)时重新 getDanmuInfo;新增 `ConnectionState` 事件(Connected/Reconnecting/Failed)上报 UI。
- 验收:kill 网络 30s 后自动恢复,事件不重不漏(以 seq 去重)。

**M1.3 录制稳定性闭环**
- 方案:AutoRecorder 增加 watchdog——监控输出文件增长速率,停滞 >15s 则杀 ffmpeg、重新 `getRoomPlayInfo` 取新 URL 续录(文件名递增 part 号);录制自检(codec/YAVG/RMS)进产品路径;关播导出时 remux `rec.mp4`(faststart)。
- 验收:模拟 URL 过期(手动断网 1 分钟)能自动续录出新分段。

**M1.4 弹幕-视频联动录制**
- 方案:`recorder_start` 同时启动 danmaku 客户端,JSONL 写入会话目录(复用 `vtb-pipeline::danmaku_log`),`session_start` 写入 meta.json;离线管线自动发现同目录弹幕日志。
- 验收:录完的目录拖进离线处理,highlight 自动带弹幕信号。

**M1.5 OBS 输出服务器**
- 方案:Tauri 后端起本地 HTTP+WebSocket 服务(axum,127.0.0.1 随机端口),提供 `/overlay/danmaku` 与 `/overlay/subtitle` 页面(复用现有主题 CSS,透明背景),事件经 WS 推送;OBS 添加浏览器源即用。
- 验收:OBS 中弹幕流式显示、同传字幕条实时更新。

**M1.6 配置持久化与密钥安全**
- 方案:tauri-plugin-store 存常规配置;API key 走钥匙串;设置页 UI。

### Phase 2 — 生态对齐(P1)

- **M2.1 多房间管理**:房间卡片列表(标题/封面/状态,`getInfoByRoom`),每房间独立 监听/录制/弹幕 开关。
- **M2.2 通知系统**:开播/关播/录制异常推 Bark + Telegram + ServerChan(可插拔 Notifier trait)。
- **M2.3 SC/舰长记录簿与场次报告**:SQLite(sqlx)落库弹幕事件 → 下播生成报告(SC列表/营收/弹幕TOP/词云数据),导出 CSV/Markdown。
- **M2.4 高能进度条**:离线管线输出 per-window 密度曲线 JSON;前端录播回放页用 canvas 画曲线,点击跳转、框选导出切片。
- **M2.5 多模态复核接线**:离线管线可选开启——highlight 候选抽帧→`AnthropicJudge`→复核分+自动标题写回 `highlights.json`,UI 展示。
- **M2.6 字幕烧录导出**:切片时可选 `-vf subtitles=bilingual.ass`(需重编码,x264 fast);双语 ASS 样式沿用现有模板。
- **M2.7 ASR 模型管理**:设置页列出 tiny/small/medium/large-v3-turbo,下载到 `~/.cache/vtb-toolkit`,显示 RTF 基准;幻觉过滤规则([BLANK_AUDIO]/重复n-gram/纯标点)。
- **M2.8 弹幕 XML 导出**:JSONL→B站兼容 XML 转换器,接入录播姬生态(可用 danmaku2ass 转 ASS)。
- **M2.9 磁盘管理**:录制前检查剩余空间,低于阈值告警/停录/滚动清理(可配)。

### Phase 3 — 差异化能力(P2)

- **M3.1 实时 highlight**:直播中滑窗增量计算(复用 fusion,流式 z-score),超阈值弹通知+自动打标记(存时间点,下播直接出切片候选)。
- **M3.2 翻译管线优化**:段级并发(有界队列保序)+重试+SSE 流式输出;术语命中高亮;每日 token 用量统计。
- **M3.3 弹幕 TTS**:macOS `say`/AVSpeech 起步,可配过滤规则(仅舰长/仅SC)。
- **M3.4 歌切模式**:音频指纹/能量+ASR 歌词密度检测歌曲边界,按曲导出。
- **M3.5 单流复用**:录制时实时字幕从录制 ffmpeg 分流(`-f tee` 或本地 relay),不再重复拉流。

### Phase 4 — 平台与生态(P3)

- YouTube/Twitch 支持(yt-dlp/streamlink 后端 + 平台 trait 抽象弹幕/取流)
- Webhook/本地 REST API(对齐录播姬,供外部自动化)
- biliup 投稿集成;插件系统(WASM 或本地脚本钩子)

### 工程原则(延续本阶段教训)

1. **每个功能必须有真实直播间验证路径**——vtb-e2e 增加对应 diag 工具;单测测不出 movflags/HEVC 这类真实环境 bug。
2. 产品路径与诊断路径共享实现(自检、remux 等不能只活在 diag 里)。
3. 网络协议层持续用"原始 cmd 普查"(diag_danmaku)监控 B站协议漂移(`_V2`/protobuf 化趋势)。
4. 里程碑顺序按依赖排:M1.1(登录)阻塞原画录制与弹幕完整性,最先做;M1.5(OBS)是弹幕显示与同传两条线的共同出口。


---

## 完成状态(2026-07-18)

全部四个 Phase 已实现并测试:

- **Phase 1(地基)**:扫码登录+钥匙串、弹幕重连 failover、录制 watchdog+URL 续录+健康自检+MP4 remux、弹幕-视频联动录制+自动发现、OBS 浏览器源服务器(弹幕层+同传字幕条)、配置持久化+密钥入钥匙串
- **Phase 2(生态对齐)**:多房间面板、通知(Bark/ServerChan/TG/Webhook)、SQLite 记录簿+场次报告(MD/CSV导出)、高能进度条复盘页(canvas+拖选切片)、多模态复核接线(打分+AI标题)、字幕烧录(libass 检测)、whisper 模型管理+幻觉过滤、弹幕XML导出(录播姬兼容)、磁盘安全网
- **Phase 3(差异化)**:实时 highlight 告警(滚动z-score+冷却)、翻译有界并发+重试+用量统计、弹幕TTS(macOS)、歌切(音乐段检测)、单流复用(录制 tee PCM → 实时字幕,不重复拉流)
- **Phase 4(平台与生态)**:Platform trait+yt-dlp 后端(YouTube/Twitch,graceful 降级)+platform_probe、本地 REST API(/api/status)、biliup 投稿(复盘页一键投稿)、脚本钩子插件(事件 JSON→stdin,30s 超时)

测试:248 Rust + 48 前端 = 296 全绿;应用启动验证通过。

---

## 二次差距盘点(2026-07-18,四个 Phase 完成后)

### 仍缺失的功能(vs 竞品)

| 功能 | 竞品 | 差距说明 | 建议优先级 |
|---|---|---|---|
| 弹幕发送/场控 | DDTV、神奇弹幕(已停更) | 需要 bili_jct csrf 发送接口;可做答谢/定时弹幕 | P1 |
| FLV tag 级时间戳修复 | 录播姬/blrec 核心卖点 | 我们依赖 ffmpeg copy,B站流时间戳跳变时切片/字幕可能错位;需 FLV 解析器逐 tag 修复 | P1 |
| 弹幕自动翻译显示 | blivechat(译日语给海外观众) | 翻译管线只接了语音,弹幕文本翻译未接到 overlay | P1 |
| 流参数变化自动分割/备线切换 | blrec/DDTV | 长时间录制稳定性场景 | P2 |
| 旧录播滚动清理 | blrec | 现在只有低磁盘停录,无自动清理 | P2 |
| 词云可视化 | LAPLACE | word_freq 数据已有,缺前端渲染 | P2 |
| 切片内置预览播放器 | 剪辑类工具 | 复盘页只能导出后外部播放 | P2 |
| 用户备注/粉丝画像 | LAPLACE | 认老观众 | P3 |
| 剪辑工程导出(Premiere XML/EDL) | AI clipping 商业品 | 高能点导出为剪辑软件标记 | P3 |
| YouTube/Twitch 弹幕(聊天) | Holodex | 平台 trait 只覆盖了取流,聊天协议未接 | P3 |
| 多平台录制 UI | - | yt-dlp 后端与 probe 已有,录制界面仍是 B站 room_id 流 | P3 |
| 自动更新/安装包分发/签名 | 成熟桌面产品 | 工程化发布 | P3 |
| Windows/Linux 全功能验证 | - | keyring 跨平台但 TTS(say)仅 macOS;磁盘检测非 unix 返回 MAX | P3 |

### 本轮补强(响应用户反馈)
- 翻译/AI 接口全链路可配置:provider(Anthropic/OpenAI兼容)+ API Base + 模型 + Key(钥匙串),解析优先级 显式→settings→环境变量→钥匙串→默认;修复实时字幕命令不传 base_url 的缺陷
