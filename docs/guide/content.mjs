// Each row is [简体中文, English, 日本語]. Keep feature coverage identical.
export const meta = {
  "title": [
    "VTB Toolkit 使用手册",
    "VTB Toolkit User Guide",
    "VTB Toolkit 操作ガイド"
  ],
  "subtitle": [
    "从直播到切片，一份可以离线阅读的完整指南",
    "An offline guide from live streams to finished clips",
    "配信から切り抜きまで、オフラインで読める操作ガイド"
  ],
  "intro": [
    "适用于主播、场控、录播员和切片创作者。点击截图可放大；顶部可切换三语或打印为 PDF。",
    "For streamers, moderators, recorders and clip creators. Click screenshots to enlarge; switch languages or print to PDF at the top.",
    "配信者・モデレーター・録画担当・切り抜き制作者向け。画像をクリックして拡大できます。上部で言語切替や PDF 印刷ができます。"
  ],
  "shot": [
    "Windows 实际界面 · 点击放大",
    "Actual Windows interface · Click to enlarge",
    "Windows の実画面・クリックで拡大"
  ],
  "demo": [
    "复盘曲线与高能为教程样例数据",
    "Review curves and highlights use tutorial sample data",
    "確認画面の曲線・ハイライトはチュートリアル用データです"
  ],
  "toc": [
    "操作目录",
    "Contents",
    "目次"
  ],
  "print": [
    "打印 / 保存 PDF",
    "Print / Save PDF",
    "印刷 / PDF 保存"
  ],
  "search": [
    "搜索功能或步骤…",
    "Search features or steps…",
    "機能や手順を検索…"
  ],
  "empty": [
    "没有匹配内容",
    "No matching sections",
    "一致する項目がありません"
  ],
  "close": [
    "关闭图片",
    "Close image",
    "画像を閉じる"
  ],
  "version": [
    "版本 0.1.0 · 更新于 2026-09-13",
    "Version 0.1.0 · Updated 2026-09-13",
    "バージョン 0.1.0・更新日 2026-09-13"
  ],
  "flow": [
    [
      "连接直播",
      "Connect",
      "配信に接続"
    ],
    [
      "录制与打点",
      "Record & mark",
      "録画・マーカー"
    ],
    [
      "字幕与分析",
      "Transcribe & analyze",
      "字幕・解析"
    ],
    [
      "复盘与切片",
      "Review & clip",
      "確認・切り抜き"
    ],
    [
      "封面与投稿",
      "Cover & publish",
      "サムネイル・投稿"
    ]
  ]
};
export const sections = [
  { id: "start", image: "danmaku", title: ["开始使用与三语界面","Getting started and interface language","はじめに・表示言語"], steps: [
    ["启动 Windows 安装版或 vtb-toolkit.exe。顶部依次是语言选择、使用教程和十个功能页签。选择简体中文、English 或日本語后立即生效，重启会记住选择。","Launch the Windows installation or vtb-toolkit.exe. The top bar provides language selection, this guide and ten feature tabs. Choose 简体中文, English or 日本語; the change is immediate and remembered after restart.","Windows インストール版または vtb-toolkit.exe を起動します。上部には言語選択、操作ガイド、10 個の機能タブがあります。简体中文・English・日本語を選ぶと即座に反映され、再起動後も保持されます。"],
    ["界面语言不会改变语音识别语种、翻译目标、昵称、聊天原文、备注或已填写的投稿内容。切换语言不会重新启动当前页的任务。","Interface language is separate from speech recognition and translation targets. Viewer names, chat, notes and authored upload fields remain unchanged. Switching language does not restart tasks on the current page.","表示言語は音声認識の言語や翻訳先とは独立しています。視聴者名、チャット、メモ、入力済みの投稿内容は変わりません。言語切替で現在のタスクが再起動することもありません。"],
    ["录制、抽帧和切片需要 FFmpeg/ffprobe；YouTube/Twitch 取流需要 yt-dlp。将它们加入 PATH；Windows 的 yt-dlp 也可放在程序旁边。Whisper 模型在字幕页下载，文本 AI 需要自己的接口配置。","Recording, frame extraction and clipping require FFmpeg/ffprobe. YouTube/Twitch stream resolution requires yt-dlp. Add these to PATH; Windows can also find yt-dlp beside the executable. Download a Whisper model in Subtitles and configure your own endpoint for text AI.","録画・フレーム抽出・切り抜きには FFmpeg/ffprobe、YouTube/Twitch の取得には yt-dlp が必要です。PATH に追加してください。Windows では yt-dlp を実行ファイルの隣にも置けます。Whisper モデルは字幕画面で取得し、テキスト AI 用の API は別途設定します。"],
    ["推荐流程：先选输出目录并收藏直播 → 连接评论及录制 → 打点/实时字幕/OBS → 下播后离线分析 → 复盘、封面、导出与投稿。","Suggested flow: choose an output folder and save streams → connect chat and record → add markers, live subtitles and OBS → process the recording offline → review, create covers, export and upload.","おすすめの流れ：保存先と配信を登録 → チャット接続・録画 → マーカー・リアルタイム字幕・OBS → 終了後にオフライン解析 → 確認・サムネイル・書き出し・投稿。"]
  ]},
  { id: "rooms", image: "rooms", title: ["房间收藏与双平台连接","Rooms and both platforms","ルーム管理と両プラットフォーム"], steps: [
    ["Bilibili：在房间页输入数字房间号并添加。卡片展示封面、主播、分区及直播/轮播/未开播状态，每 30 秒刷新。卡片可连接弹幕、开始/停止录制或移除收藏。","Bilibili: add the numeric room ID in Rooms. Cards show the cover, creator, category and live/replay/offline status, refreshed every 30 seconds. Connect chat, start/stop recording or remove a favorite from its card.","Bilibili：ルーム画面で数字のルーム ID を追加します。カードには画像・配信者・カテゴリ・配信中/再放送/未配信の状態が表示され、30 秒ごとに更新されます。カードからチャット接続、録画開始/停止、お気に入り削除ができます。"],
    ["YouTube：输入公开直播链接、视频 ID、@频道名或频道链接。固定视频只对应那场直播；频道收藏可发现频道的新直播。点击连接评论；录制前先在录制页设置输出目录。","YouTube: enter a public live URL, video ID, @handle or channel URL. A fixed video identifies one broadcast; a channel favorite can discover new broadcasts. Connect its chat, and set the output folder in Recording before starting a recording.","YouTube：公開配信 URL、動画 ID、@ハンドル、チャンネル URL を入力します。動画 ID は特定の配信、チャンネル登録は新しい配信の検出に使います。チャットを接続し、録画前に録画画面で保存先を設定してください。"],
    ["YouTube 评论优先匿名读取，不需要 API Key 或 Google 登录。会员限定、年龄限制、地区限制、未开播或已关闭评论的直播可能无法读取。匿名读取依赖 YouTube 网页接口，网站改版后可能需要更新软件。","YouTube chat uses anonymous reading without an API key or Google sign-in. Members-only, age/region-restricted, offline or chat-disabled streams may be unavailable. Reading relies on YouTube web interfaces, so website changes may require an app update.","YouTube チャットは API キーや Google ログインなしで匿名取得します。メンバー限定、年齢・地域制限、未配信、チャット無効の配信は取得できない場合があります。Web インターフェースを利用するため、YouTube の変更時にはアプリ更新が必要になることがあります。"]
  ]},
  { id: "chat", image: "danmaku", title: ["评论、翻译和观众备注","Chat, translation and viewer notes","チャット・翻訳・視聴者メモ"], steps: [
    ["弹幕页可输入 Bilibili 房间号连接多个房间，也可管理 YouTube 收藏；Twitch 栏可匿名连接频道。列表显示普通消息、礼物、SC、贴纸和会员事件，支持表情、头像及平台标识。","In Chat, connect Bilibili rooms, manage YouTube favorites or use the Twitch field for anonymous channel chat. The list displays messages, gifts, Super Chats, stickers and membership events, including emoji, avatars and platform badges.","チャット画面で複数の Bilibili ルームや YouTube お気に入りを接続できます。Twitch 欄では匿名でチャンネルチャットに接続できます。通常メッセージ、ギフト、SC、ステッカー、メンバーイベントに加え、絵文字・アバター・プラットフォーム名を表示します。"],
    ["选择翻译目标（中/英/日/韩），点击开启翻译。先在“设置 → AI 服务 → 文本模型”保存接口配置；译文会同时出现在工具和 OBS 评论层。界面语言不会自动设置这个目标。","Choose a translation target (Chinese, English, Japanese or Korean) and enable translation. First save the text model configuration in Settings → AI services. Translations appear both in the app and the OBS chat overlay. Interface language does not change this target.","翻訳先（中・英・日・韓）を選び、翻訳を有効にします。事前に「設定 → AI サービス → テキストモデル」で接続設定を保存してください。訳文はアプリと OBS のチャットに表示されます。表示言語を変えても翻訳先は変わりません。"],
    ["点击用户名可写观众备注，空备注会清除标签。备注按平台身份分开保存，避免同名观众混淆；原始昵称、评论及备注不会被界面国际化改写。B 站匿名连接可能只显示脱敏昵称，登录后可改善。","Click a username to add a viewer note; an empty note clears its tag. Notes are stored by platform identity to avoid confusing identical names. Localization leaves names, chat and notes intact. Anonymous Bilibili chat may mask names; sign-in can improve this.","ユーザー名をクリックして視聴者メモを付けます。空欄で保存するとタグが消えます。プラットフォーム別の識別子で保存されるため同名でも混同しません。匿名の Bilibili 接続では名前が伏せられることがあり、ログインで改善する場合があります。"],
    ["YouTube 删除消息及封禁相关事件会同步移除相应显示；断线会重连。阅读连接与录制监听是独立的，停止看评论不等于停止录制。","YouTube deletion and moderation events remove matching displayed messages, and connections retry after a disconnect. Chat connections and recording monitors are independent: stopping chat does not stop recording.","YouTube の削除・モデレーションイベントに応じて該当表示を除去し、切断時は再接続します。チャット接続と録画監視は独立しているため、チャット停止だけでは録画は止まりません。"]
  ]},
  { id: "themes", image: "themes", title: ["弹幕主题与样式","Chat themes and appearance","チャットのテーマと表示"], steps: [
    ["在弹幕页的主题选择器切换内置主题，点击编辑打开样式面板。可以改颜色、字号、字体、行距、圆角、入场动画、时间显示和粉丝牌显示。","Choose a built-in theme in Chat, then open Edit for appearance controls. Adjust colors, font size/family, spacing, corners, entry animation, timestamps and fan badges.","チャットのテーマ選択から内蔵テーマを選び、編集を開きます。色、文字サイズ・フォント、行間、角丸、入場アニメーション、時刻、ファンバッジを調整できます。"],
    ["编辑内置主题会生成可命名的副本。保存后自定义主题出现在选择器中；可导出 JSON 备份或粘贴 JSON 导入。OBS 页单独选择主题并把新地址复制到 OBS，已粘贴的旧地址不会自动更新。","Editing a built-in theme creates a named copy. Save it to the selector, export JSON for backup or paste JSON to import. Choose the theme separately in OBS Output and paste the new URL into OBS; existing URLs do not update automatically.","内蔵テーマの編集は名前を付けられるコピーを作ります。保存すると選択欄に追加され、JSON の書き出し・貼り付けによる読み込みもできます。OBS 出力では別途テーマを選び、新しい URL を OBS に貼り直してください。"]
  ]},
  { id: "moderation", image: "moderation", title: ["Bilibili 场控","Bilibili moderation tools","Bilibili の配信管理"], steps: [
    ["在账号页登录后，回到弹幕页选择 B 站房间并展开场控。发送框以你的账号发言；开启礼物、上舰或 SC 自动答谢前，检查模板和触发条件。YouTube 不提供评论发送或自动答谢。","Sign in through Accounts, select a Bilibili room in Chat and expand Moderation. The send field posts as your account. Review templates and triggers before enabling gift, membership or Super Chat thanks. YouTube has no comment sending or automatic thanks.","アカウント画面でログインし、チャットで Bilibili ルームを選んで配信管理を開きます。送信欄は本人のアカウントで発言します。ギフト・加入・SC の自動お礼を有効にする前にテンプレートと条件を確認してください。YouTube の送信・自動お礼には対応していません。"],
    ["礼物答谢模板使用面板列出的变量（如 {user}、{gift}、{count}）；会员/SC 使用内置模板。定时弹幕输入内容与间隔后启动，间隔至少 30 秒；用停止按钮结束。所有发送类设置均应在了解实际内容后启用。","Gift-thanks templates use the variables shown in the panel (such as {user}, {gift}, {count}); membership/SC thanks use built-in templates. Enter the scheduled message and interval, then start it; intervals must be at least 30 seconds. Stop it with the stop button. Review the actual outgoing content before enabling sends.","ギフトのお礼にはパネルの変数（例：{user}、{gift}、{count}）を使います。加入・SC は内蔵テンプレートを使います。定期メッセージと間隔を入力して開始します。間隔は 30 秒以上です。停止ボタンで終了し、送信内容を確認してから有効にしてください。"]
  ]},
  { id: "recording", image: "recorder", title: ["自动录制、分段与保留策略","Recording, segments and retention","自動録画・分割・保存期間"], steps: [
    ["进入录制页，填写可写的输出目录和 B 站房间号，或输入 YouTube/Twitch 直播 URL。选择不切分、按时长（秒）或按大小（字节），点击开始监听。监听会等待开播；频道 URL 可继续等待下一场直播。","In Recording, enter a writable output folder and a Bilibili room ID or a YouTube/Twitch live URL. Select single file, duration in seconds or size in bytes, then start monitoring. A monitor waits for the stream to start; a channel URL can keep watching for the next broadcast.","録画画面で書き込み可能な保存先と Bilibili ルーム ID、または YouTube/Twitch URL を入力します。分割なし・時間（秒）・サイズ（バイト）を選んで監視を開始します。未配信なら待機し、チャンネル URL では次の配信も監視できます。"],
    ["监听列表和底部日志显示状态。停止时会结束当前录制并整理文件；断流时程序尝试重新取流并续录到新分段，不能保证网络中断期间没有缺失。YouTube 通过 yt-dlp 获取视频和音频轨。","Use the monitor list and log for status. Stopping finalizes the current recording. On interruption the app tries to resolve the stream again and continues in a new part; lost network time cannot be guaranteed gap-free. YouTube video and audio tracks are resolved with yt-dlp.","監視一覧とログで状態を確認します。停止すると現在の録画を終了してファイルを整理します。切断時は再取得して新しい分割ファイルに続録しますが、切断中の欠落は防げない場合があります。YouTube の映像・音声トラックは yt-dlp で取得します。"],
    ["输出按来源及场次组织。视频之外还有 meta.json、弹幕 JSONL/XML、markers.jsonl 等日志，分析时应保留整场目录。MP4 整理失败时检查仍保留的原始 MKV/FLV 和日志。","Output is organized by source and session. Keep the whole folder, including meta.json, chat JSONL/XML and markers.jsonl, for analysis. If MP4 remux fails, inspect the preserved MKV/FLV and logs.","出力は配信元・セッション別に整理されます。解析に使う meta.json、チャット JSONL/XML、markers.jsonl を含め、フォルダー全体を保持してください。MP4 変換に失敗した場合は保持された MKV/FLV とログを確認します。"],
    ["滚动清理默认三项都为 0（关闭）。设置保留天数、每房间场次数或目标剩余 GiB 会删除符合策略的旧录播；重要素材请先备份。磁盘不足时监控会安全停止。","Retention defaults to zero for all three fields (disabled). Setting age, sessions per room or target free GiB can delete old recordings that meet the policy; back up important material first. Low disk space stops monitoring safely.","自動整理は 3 項目とも 0（無効）が初期値です。保存日数・ルーム別セッション数・空き GiB 目標を設定すると条件に合う古い録画が削除されます。重要な素材は先にバックアップしてください。容量不足では監視を停止します。"],
    ["通知设置支持 Bark、ServerChan、Telegram 或自定义 Webhook，用于开播、完成和异常提醒。按各服务填写地址/令牌并保存后，录制事件会实际推送到配置目标。","Notifications support Bark, ServerChan, Telegram and custom webhooks for start, completion and failure events. Enter and save the service-specific URL/token; recording events then send real notifications to that destination.","Bark・ServerChan・Telegram・独自 Webhook で開始・完了・異常を通知できます。サービスの URL・トークンを設定して保存すると、録画イベントが設定先へ実際に通知されます。"]
  ]},
  { id: "markers", image: "danmaku", title: ["直播打点与高能提示","Live markers and highlight alerts","配信中のマーカーとハイライト通知"], steps: [
    ["开始录制后，弹幕页会出现可打点的录制会话。选择 bilibili:房间号 或 youtube:视频ID，填可选备注，点击打点。多个录制同时运行时务必指定目标。","While recording, Chat lists sessions available for markers. Select bilibili:room-ID or youtube:video-ID, enter an optional note and add a marker. Choose the target explicitly when multiple recordings are active.","録画開始後、チャット画面にマーカー対象のセッションが表示されます。bilibili:ルームID または youtube:動画ID を選び、任意のメモを入力して追加します。複数録画中は必ず対象を選択してください。"],
    ["全局快捷键为 Ctrl+Shift+M（macOS 为 Cmd+Shift+M）。B 站房管还可发送“打点 备注”触发；YouTube 不使用这条房管口令。弹幕密度峰值可自动产生高能提示及自动打点。","The global shortcut is Ctrl+Shift+M, or Cmd+Shift+M on macOS. Bilibili room moderators may also send “打点 note” to trigger a marker; this command is not used on YouTube. Chat-density spikes can trigger automatic highlight alerts and markers.","全体ショートカットは Ctrl+Shift+M（macOS は Cmd+Shift+M）です。Bilibili のモデレーターは「打点 メモ」を送信して追加することもできます。YouTube ではこのコマンドを使いません。チャット急増は自動ハイライト通知・マーカーの対象になります。"],
    ["打点写入 markers.jsonl 并对齐录制时间轴。下播后在复盘中查看，或离线分析时作为候选片段依据；可导出时间戳列表用于简介和评论。","Markers are saved in markers.jsonl on the recording timeline. Review them later, use them as offline highlight candidates or export timestamps for descriptions and comments.","マーカーは録画の時間軸に合わせて markers.jsonl に保存されます。後から確認し、オフライン解析の候補に使うほか、説明欄・コメント用のタイムスタンプにも書き出せます。"]
  ]},
  { id: "live-subtitles", image: "subtitle", title: ["实时字幕、Whisper 模型与同传","Live subtitles, Whisper and translation","リアルタイム字幕・Whisper・同時翻訳"], steps: [
    ["在实时字幕页输入 B 站房间号或 YouTube 链接，选择源语种（或自动检测）、模型路径及热词表。模型列表可直接下载；下载完选择模型，再开始字幕任务。","In Live Subtitles, enter a Bilibili room ID or YouTube URL, choose the source language (or auto-detect), model path and glossaries. Download a model from the list, select it and start subtitles.","リアルタイム字幕で Bilibili ルーム ID または YouTube URL、原語（または自動判定）、モデルパス、用語集を選びます。一覧からモデルを取得して選択し、字幕処理を開始します。"],
    ["tiny 适合快速验证；small 适合速度/精度平衡；更大的模型需要更强硬件。识别在本机运行，噪声、音乐及低配机器可能降低准确率或造成延迟。热词表有助于人名和游戏术语。","tiny is useful for quick checks; small balances speed and accuracy. Larger models need more capable hardware. Recognition runs locally; noise, music and slow hardware can reduce accuracy or add delay. Glossaries help with names and game terms.","tiny は簡単な動作確認、small は速度と精度のバランスに向きます。大型モデルには高性能 PC が必要です。認識はローカルで行い、雑音・音楽・処理性能によって精度や遅延が変わります。用語集は人名やゲーム用語に役立ちます。"],
    ["启用翻译并选择目标语言时，需要有效 LLM 配置；原文及译文会显示在列表并发送到 OBS 字幕条。每个来源可独立停止。当前实时字幕只在内存展示，要保存 SRT/ASS 请对录播运行离线处理。","Translation requires a working LLM configuration and a selected target language. Source and translated text appear in the list and OBS subtitle bar. Stop each source independently. Live subtitles currently stay in memory; run Offline Processing on the recording to save SRT/ASS.","翻訳には有効な LLM 設定と翻訳先の指定が必要です。原文・訳文は一覧と OBS 字幕に表示され、配信ごとに停止できます。リアルタイム字幕は現在メモリ内のみです。SRT/ASS の保存には録画をオフライン処理してください。"],
    ["翻译配置在“设置 → AI 服务 → 文本模型”统一管理；页面中的设置按钮可直接跳转，保存后返回字幕页重新开始任务。","Manage translation in Settings → AI services → Text model. Use the shortcut, save, then return to Subtitles and restart the task.","翻訳は「設定 → AI サービス → テキストモデル」で管理します。ショートカットで設定・保存し、字幕画面へ戻ってタスクを再開始してください。"]
  ]},
  { id: "llm", image: "llm", title: ["设置 → AI 服务：文本模型","Settings → AI services: text models","設定 → AI サービス：テキストモデル"], steps: [
    ["打开顶部“设置”页签。文本模型用于弹幕/字幕翻译、语义粗剪、热词整理和封面创意；视觉与图片生成有各自的子页签。Whisper 语音识别在本机运行，不需要大模型 API Key。","Open Settings at the top. Text models power chat/subtitle translation, semantic rough cuts, glossary parsing and cover ideas. Vision and image generation have separate tabs. Local Whisper recognition requires no LLM API key.","上部の「設定」を開きます。テキストモデルはチャット・字幕翻訳、意味解析による粗編集、用語整理、サムネイル案に使います。映像解析・画像生成は別タブです。ローカルの Whisper 音声認識に LLM の API キーは不要です。"],
    ["选择 OpenAI 兼容或 Anthropic，填写 Base URL 和模型名称。OpenAI 兼容地址通常以 /v1 结尾；Anthropic 通常填写 https://api.anthropic.com。不要附加 /chat/completions 或 /messages。自建服务需支持所选 API。","Choose OpenAI-compatible or Anthropic, then enter the base URL and model name. OpenAI-compatible URLs usually end in /v1; Anthropic normally uses https://api.anthropic.com. Do not append /chat/completions or /messages. Self-hosted services must implement the selected API.","OpenAI 互換または Anthropic を選び、Base URL とモデル名を入力します。OpenAI 互換は通常 /v1 まで、Anthropic は通常 https://api.anthropic.com です。/chat/completions や /messages は付けません。自前サーバーも選択した API への対応が必要です。"],
    ["输入 API Key 后点“保存配置”。Key 按用途、服务类型和地址隔离，存入系统凭据库；普通 settings.json 不含 Key。留空保存会保留当前服务的已存 Key，输入新值则更新。无需 Key 的本地服务可勾选“无需认证”。","Enter the API key and select Save configuration. Keys are isolated by purpose, API type and URL in the OS credential store; plain settings.json contains no key. Saving a blank key keeps the current service’s saved key; entering a value replaces it. Select No authentication for local services that require no key.","API キーを入力して「設定を保存」を押します。キーは用途・API 種別・URL ごとに OS の資格情報ストアに保存され、通常の settings.json には入りません。空欄で保存すると現在のサービスのキーを保持し、入力すると更新します。キー不要のローカルサービスでは「認証不要」を選べます。"],
    ["改动后先保存，再点“测试连接”。文本测试只发送固定短文本，视觉测试还发送一张测试图片，可能产生少量 API 费用。底部“当前生效配置”显示已保存任务实际使用的地址、模型和密钥来源。","Save changes before selecting Test connection. Text tests send a fixed short prompt; vision tests also send a sample image and may incur a small API charge. Effective configuration shows the endpoint, model and key source used by new tasks.","変更を保存してから「接続テスト」を押します。テキストは固定の短文、映像解析はテスト画像も送信するため、少額の API 料金が発生する場合があります。「現在有効な設定」で新規タスクの接続先・モデル・キーの取得元を確認できます。"],
    ["允许环境变量回退时，地址和模型按“已填设置 → 环境变量 → 默认值”取值，密钥按“系统凭据库 → 匹配环境变量”取值。文本和视觉分别使用所选服务的 OPENAI_* 或 ANTHROPIC_*；图片服务使用 IMAGE_*（API_KEY、BASE_URL、MODEL）。环境 Key 只用于其指定地址或对应官方地址。","With environment fallback enabled, URL/model precedence is entered settings → environment → defaults; key precedence is OS credential store → matching environment. Text and vision use OPENAI_* or ANTHROPIC_* for the selected API; images use IMAGE_* (API_KEY, BASE_URL, MODEL). An environment key only applies to its declared URL or the corresponding official URL.","環境変数を許可すると、URL・モデルは「入力済み設定 → 環境変数 → 既定値」、キーは「OS 資格情報ストア → 対応する環境変数」の順になります。テキスト・映像解析は選択した API の OPENAI_* または ANTHROPIC_*、画像生成は IMAGE_*（API_KEY・BASE_URL・MODEL）です。環境のキーはそこで指定した URL または対応する公式 URL にだけ使用します。"],
    ["“清除已保存密钥”只删除当前用途、类型、地址对应的 Key；若允许环境回退，清除后仍可能使用环境 Key。改地址后需为新地址单独配置；要清除旧地址的 Key，先切回并保存旧地址。正在运行的任务需停止后重启才使用新配置。","Clear saved key only removes the key for the current purpose, type and URL. Environment fallback may still supply a key afterward. A new URL needs its own key; to clear an old URL’s key, return to and save that URL first. Stop and restart running tasks to use changed settings.","「保存済みキーを削除」は現在の用途・種別・URL のキーだけを削除します。環境変数を許可していれば、そのキーは引き続き使われる場合があります。新 URL のキーは別途設定します。旧 URL のキーを消す場合は旧 URL に戻して保存してください。実行中のタスクには停止・再開始後に新設定が適用されます。"],
    ["首次打开会迁移旧文本配置及已存密钥，并保留旧封面地址和模型。旧版封面 Key 仅在内存中，需重新输入保存。旧共享 Key 保留用于回退旧版，新版只读取迁移后的独立凭据。","The first visit migrates old text settings and saved credentials, plus the old cover URL/model. The old cover key existed only in memory and must be entered again. The legacy shared key is retained for rollback; this version only reads migrated, scoped credentials.","初回に旧テキスト設定・保存済みキーと、旧サムネイルの URL・モデルを移行します。旧画像キーはメモリ内のみだったため再入力が必要です。旧共有キーは旧版への復帰用に残し、新版は移行後の用途別資格情報だけを読みます。"]
  ]},
  { id: "ai-vision", image: "ai-vision", title: ["视觉模型：复用或独立配置","Vision models: reuse or configure separately","映像解析モデル：共有と個別設定"], steps: [
    ["默认勾选“复用文本模型配置”，视觉分析使用文本服务的地址、模型和 Key。共用模型必须支持图片输入；此模式不能在视觉页删除文本 Key。","Reuse text model configuration is enabled by default. Vision uses the text endpoint, model and key, so the model must accept images. The vision tab cannot delete the shared text key in this mode.","初期状態では「テキストモデルの設定を共有」が有効です。テキストの URL・モデル・キーを使うため、モデルは画像入力に対応している必要があります。この状態では映像解析タブから共有キーを削除できません。"],
    ["需要不同视觉模型时取消复用，填写该页的地址、模型和 Key，保存后测试。离线处理的画面扫描与高光复核会读取这组配置，文本翻译继续使用文本页的配置。","Disable reuse to choose a separate vision model. Enter its URL, model and key, save and test. Offline frame scanning and highlight review use this configuration while translation continues using the text settings.","別の映像解析モデルを使う場合は共有を解除し、URL・モデル・キーを設定して保存・テストします。オフラインのフレーム解析とハイライト確認はこの設定、翻訳はテキスト設定を使用します。"],
    ["本教程连接测试使用本机模拟服务和示例模型，只用于验证请求格式、设置和错误显示；截图中的测试通过不代表任何远端模型已认证可用。实际 AI 会把所需文本或画面发送至你保存的服务。","Tutorial connection tests use a local mock service and sample models to check request formats, settings and error display. A successful test in a screenshot does not certify a remote model. Actual AI tasks send required text or frames to your saved service.","ガイドの接続テストはローカルの模擬サービスとサンプルモデルで、要求形式・設定・エラー表示を確認しています。画像内の成功表示は外部モデルの利用確認ではありません。実際の AI 処理は必要な文字や画像を保存済みサービスに送信します。"]
  ]},
  { id: "ai-image", image: "ai-image", title: ["图片生成：独立服务与测试范围","Image generation: separate service and test scope","画像生成：専用サービスとテスト範囲"], steps: [
    ["在“设置 → AI 服务 → 图片生成”填写独立的 OpenAI 兼容地址、图像模型和 Key，再保存。图像 Key 同样存入系统凭据库，重启后仍有效。","In Settings → AI services → Image generation, enter a separate OpenAI-compatible URL, image model and key, then save. Image keys also persist in the OS credential store across restarts.","「設定 → AI サービス → 画像生成」で専用の OpenAI 互換 URL・画像モデル・キーを保存します。画像キーも OS の資格情報ストアに保存され、再起動後も保持されます。"],
    ["图片“测试连接”仅 GET 查询 /models 并检查模型名称，不生成收费图片；通过只证明模型查询成功，不证明 images/generations 或 images/edits 权限。某些中转不提供 /models，测试失败也需结合服务文档判断。","Test connection only sends GET /models and checks the model name. It creates no billable image. Success proves model discovery, not access to images/generations or images/edits. Some proxies omit /models, so interpret failures alongside the service documentation.","接続テストは GET /models でモデル名を確認するだけで、有料画像を生成しません。成功はモデル照会の確認で、images/generations・images/edits 権限の確認ではありません。/models 非対応の中継もあるため、失敗時はサービスの仕様も確認してください。"],
    ["回到复盘的封面助手，填写 prompt 和尺寸，再生成图片。未选候选帧使用文生图；选中候选帧使用参考图编辑。端点和模型必须支持相应接口。此处也有按钮可直接打开图片服务设置。","Return to the Cover Assistant in Review, enter the prompt and size, then generate. No selected frame uses text-to-image; selected frames use reference-image editing. The endpoint/model must support that operation. A shortcut opens image settings from this panel.","確認画面のサムネイル機能へ戻り、プロンプト・サイズを指定して生成します。候補フレーム未選択ならテキストから生成、選択済みなら参照画像による編集です。接続先・モデルの対応が必要です。この画面から画像サービス設定を直接開くこともできます。"]
  ]},
  { id: "offline", image: "offline", title: ["离线处理与 AI 粗剪","Offline processing and AI rough cuts","オフライン処理と AI 粗編集"], steps: [
    ["输入录播文件、输出目录及 Whisper 模型。选择源语种、热词表、是否翻译及目标语种；可手动填写弹幕 JSONL，否则尝试从录播附近发现日志。先保留与视频对应的原始场次目录。","Enter the recording file, output folder and Whisper model. Choose source language, glossaries, optional translation and target. Specify a chat JSONL if needed; otherwise nearby logs are discovered automatically. Keep the original session folder associated with the video.","録画ファイル、出力先、Whisper モデルを入力します。原語・用語集・翻訳の有無・翻訳先を選びます。チャット JSONL を指定するか、動画付近のログを自動検出させます。元のセッションフォルダーは保持してください。"],
    ["标准处理依次提取音频、识别、可选翻译、导出字幕、检测高能并切片。输出包含 transcript.json、SRT/双语 ASS、signals.json、highlights.json 和切片。开启烧录双语字幕需要带 libass 的 FFmpeg。","The pipeline extracts audio, transcribes, optionally translates, exports subtitles, detects highlights and cuts clips. Outputs include transcript.json, SRT/bilingual ASS, signals.json, highlights.json and clips. Burning bilingual subtitles requires FFmpeg with libass.","音声抽出→認識→任意の翻訳→字幕出力→ハイライト検出→切り抜きの順で処理します。transcript.json、SRT/二言語 ASS、signals.json、highlights.json、切り抜きを出力します。字幕焼き込みには libass 対応 FFmpeg が必要です。"],
    ["AI 语义粗剪分析转录中的梗、反转和操作；画面扫描按间隔抽帧寻找视觉高光。默认关闭，可分别开启；设置前后冗余、最大切片数和抽帧间隔。较小间隔意味着更多请求和更高成本，失败批次可能被跳过。","Semantic rough cuts find jokes, turns and actions in transcripts; visual scanning samples frames for visual highlights. Both default off and can be enabled separately. Set padding, maximum clips and frame interval. Shorter intervals mean more requests and cost; failed batches may be skipped.","意味解析は文字起こしからネタ・展開・プレイを、映像解析は間隔ごとのフレームから見どころを探します。初期状態は無効で個別に有効化できます。前後余白・最大切り抜き数・抽出間隔を指定します。短い間隔ほど API 利用量が増え、失敗したバッチは省略されることがあります。"],
    ["歌切模式可检测并单独导出歌曲段落。完成后把输出目录填入复盘页检查结果；自动候选仍需人工审看。多段录播请分别选择文件处理，不要假定程序会自动拼接整场。","Song mode detects and exports song sections separately. Open the output folder in Review when finished and inspect automatic candidates. Process recording parts individually; do not assume the app automatically concatenates an entire multipart session.","楽曲モードでは歌の区間を検出して個別出力できます。完了後は出力フォルダーを確認画面で開き、自動候補を目視確認してください。複数分割の録画は各ファイルを指定して処理し、全体が自動連結されるとは想定しないでください。"],
    ["语义粗剪使用文本模型，画面扫描与复核使用视觉模型；先在设置页保存对应配置，再启动离线任务。","Semantic rough cuts use the text model; frame scanning and review use the vision model. Save both configurations before starting an offline task.","意味解析の粗編集はテキストモデル、フレーム解析・確認は映像解析モデルを使います。オフライン処理の前に対応する設定を保存してください。"]
  ]},
  { id: "review", image: "review", title: ["复盘时间轴、统计与手动切片","Timeline review, statistics and manual clips","タイムライン・統計・手動切り抜き"], steps: [
    ["复盘页填写场次目录或离线输出目录并加载。点击时间轴定位预览，拖动选区后导出切片。若视频无法在 WebView 播放，可生成 MP4 代理副本后预览；这需要重编码时间和额外空间。","Load a session or offline output folder in Review. Click the timeline to seek, drag a selection and export a clip. If WebView cannot play the video, create an MP4 proxy for preview; re-encoding takes time and extra disk space.","確認画面でセッションまたはオフライン出力フォルダーを読み込みます。タイムラインをクリックして移動、ドラッグして範囲を選び、切り抜きを出力します。WebView で再生できない場合は MP4 プロキシを作成します。再エンコードに時間と容量が必要です。"],
    ["曲线中红色代表弹幕密度，蓝色代表音频能量，高能区间有橙色底色；绿色为手动打点、橙色为自动打点、黄色为 SC、紫色为会员、灰色为开播等事件。点击列表时间可定位。","Red shows chat density, blue audio energy, and orange shading highlights. Green ticks are manual markers, orange automatic markers, yellow Super Chats, purple memberships and gray stream events. Click a list timestamp to seek.","赤はチャット密度、青は音声エネルギー、オレンジの背景はハイライトです。緑の線は手動、オレンジは自動マーカー、黄は SC、紫はメンバー、灰は配信イベントです。一覧の時刻からも移動できます。"],
    ["场次报告包含消息数、独立发言者、热词云、发言榜和付费事件。币种分开统计，未知金额不强行换算。导出报告会保存当前界面语言的 Markdown 和 SC CSV；CSV 字段名保持稳定便于分析，观众内容保持原文。","Reports include message counts, unique viewers, a word cloud, top chatters and paid events. Currencies remain separate; undisclosed amounts are not converted. Export saves Markdown in the current interface language plus SC CSV. CSV column names stay stable for analysis, and viewer content remains original.","レポートにはメッセージ数、発言者数、ワードクラウド、ランキング、課金イベントがあります。通貨は別集計し、不明額は換算しません。現在の表示言語で Markdown と SC CSV を出力します。CSV 列名は解析用に固定し、視聴者の内容は原文を保持します。"],
    ["已有切片列表可预览和打开投稿。复盘依赖已有的日志与分析输出：只有视频而没有评论日志时，无法补回当时的弹幕统计。当前源视频查找会选择一个分段，请核实视频和时间轴对应。","The exported-clips list provides previews and upload controls. Review depends on recorded logs and analysis output: a video alone cannot recover historical chat statistics. Source discovery currently selects one recording part, so verify that the video matches the timeline.","出力済み切り抜き一覧からプレビューや投稿を開けます。統計は保存済みログと解析結果に依存し、動画だけでは過去のチャット統計を復元できません。現在の動画検出は 1 分割を選ぶため、時間軸との対応を確認してください。"]
  ]},
  { id: "exports", image: "review", title: ["剪辑工程与素材包导出","Editor projects and asset bundles","編集プロジェクトと素材の書き出し"], steps: [
    ["复盘中的导出 EDL 用于通用剪辑交换；FCPXML 可供 Final Cut Pro / DaVinci Resolve 导入；剪映草稿导出后按提示复制到剪映草稿目录。目标剪辑软件仍需能访问原视频路径，跨电脑请重新链接素材。","Export EDL for editing interchange, FCPXML for Final Cut Pro/DaVinci Resolve, or a Jianying draft and copy it into Jianying's draft folder as instructed. Editors still need access to the source video; relink media when moving to another computer.","EDL は汎用編集交換、FCPXML は Final Cut Pro/DaVinci Resolve、剪映草稿は案内された剪映の草稿フォルダーにコピーして使用します。編集ソフトから元動画にアクセスできる必要があり、別 PC では素材を再リンクしてください。"],
    ["导出时间戳会生成 timestamps.txt；导出素材包集中复制切片、字幕、弹幕 XML、时间戳和封面候选等已存在文件。没有生成的素材不会凭空补出，先完成离线处理和封面抽帧。","Timestamp export creates timestamps.txt. Asset bundling collects existing clips, subtitles, chat XML, timestamps and cover frames. Missing assets are not generated by bundling; complete offline processing and frame extraction first.","タイムスタンプ出力は timestamps.txt を作成します。素材パックは既存の切り抜き・字幕・チャット XML・時刻一覧・候補画像をまとめます。未生成の素材は追加されないため、先にオフライン処理やフレーム抽出を行ってください。"]
  ]},
  { id: "covers", image: "cover", title: ["封面助手","Cover assistant","サムネイルアシスタント"], steps: [
    ["在复盘加载结果后使用封面助手。抽取候选帧会从源视频采样；补充框可填写人设、梗、活动等上下文，再生成 AI 创意。创意包含标题、副标题、构图、配色、比例和图像 prompt，需要文本 LLM。","After loading Review, use the Cover Assistant to extract candidate frames. Add character, joke or event context, then request AI ideas with titles, subtitles, composition, palette, ratio and image prompts. Ideas require the text LLM.","確認結果を読み込んでサムネイル機能を使います。元動画から候補フレームを抽出し、キャラクター・ネタ・イベントなどの補足を加えて AI 案を生成します。タイトル・副題・構図・色・比率・プロンプトにはテキスト LLM が必要です。"],
    ["回到复盘的封面助手，填写 prompt 和尺寸，再生成图片。未选候选帧使用文生图；选中候选帧使用参考图编辑。端点和模型必须支持相应接口。此处也有按钮可直接打开图片服务设置。","Return to the Cover Assistant in Review, enter the prompt and size, then generate. No selected frame uses text-to-image; selected frames use reference-image editing. The endpoint/model must support that operation. A shortcut opens image settings from this panel.","確認画面のサムネイル機能へ戻り、プロンプト・サイズを指定して生成します。候補フレーム未選択ならテキストから生成、選択済みなら参照画像による編集です。接続先・モデルの対応が必要です。この画面から画像サービス設定を直接開くこともできます。"],
    ["生成结果及候选帧保存在当前复盘目录内，可加入素材包。AI 建议和图像需检查标题准确性与实际画面；无可用接口时仍可用本地候选帧制作封面。","Generated images and extracted frames are stored under the review folder and can be bundled. Check titles and images for accuracy. Without an image service you can still use locally extracted frames as covers.","生成画像と抽出フレームは確認フォルダー配下に保存され、素材パックにも含められます。タイトルや画像の正確さを確認してください。画像サービスがなくてもローカルの候補フレームを利用できます。"]
  ]},
  { id: "accounts", image: "account", title: ["账号与切片投稿","Accounts and uploads","アカウントと切り抜き投稿"], steps: [
    ["Bilibili 在账号页使用手机客户端扫码，完成登录后展示账号状态；凭据保存在系统凭据库，可退出登录。扫码登录用于弹幕/取流等功能。复盘中的 B 站投稿还要求独立安装 biliup CLI 并执行 biliup login。","Use the Bilibili mobile app to scan the QR code in Accounts. Sign-in status is shown and credentials are stored in the OS credential store; sign out here. This login supports chat/stream access. Uploading from Review additionally requires biliup CLI and its separate biliup login.","Bilibili はアカウント画面の QR コードを公式モバイルアプリで読み取ってログインします。資格情報は OS に保存され、ここでログアウトできます。この認証はチャット・取得用です。確認画面からの投稿には別途 biliup CLI と biliup login が必要です。"],
    ["YouTube 阅读不需登录，投稿必须使用官方 Google OAuth。先在 Google Cloud 项目启用 YouTube Data API，创建桌面应用 OAuth 客户端，按账号页填客户端 ID/secret 并保存；测试状态需将自己加入测试用户，再在浏览器亲自完成授权。","Reading YouTube needs no login; uploading requires official Google OAuth. Enable YouTube Data API in a Google Cloud project, create a Desktop app OAuth client, enter/save its ID and secret in Accounts, then complete authorization yourself in the browser. Add yourself as a test user while the OAuth app is in testing.","YouTube の閲覧にログインは不要ですが、投稿には公式 Google OAuth が必要です。Google Cloud で YouTube Data API を有効にし、デスクトップアプリの OAuth クライアントを作成します。ID・secret を入力保存し、ブラウザーで本人が認証してください。テスト中は自分をテストユーザーに登録します。"],
    ["在复盘切片列表点投稿 YouTube，填写标题、说明、逗号分隔标签、可见性和是否儿童内容。默认私享；确认字段后上传。可取消本地传输，但若已传完需到 YouTube Studio 核实状态，取消不等于删除远端视频。","Choose Upload to YouTube beside a reviewed clip. Enter title, description, comma-separated tags, visibility and made-for-kids status. Visibility defaults to private; review before uploading. Cancelling stops local transfer, but a completed transfer may already exist in YouTube Studio and is not deleted by cancellation.","確認済み切り抜きの YouTube 投稿を開き、タイトル・説明・カンマ区切りタグ・公開範囲・子ども向け設定を入力します。初期値は非公開です。キャンセルはローカル転送を止めますが、完了済み動画は削除されないため YouTube Studio で確認してください。"],
    ["Google 未审核客户端可能受到配额或上传可见性限制；以 YouTube Studio 的实际结果为准。本教程没有替你创建账号、完成真实授权或发布视频。","Unverified Google clients may face quota or upload-visibility restrictions. Check the actual result in YouTube Studio. This guide does not create accounts, perform real authorization or publish videos on your behalf.","未審査の Google クライアントには利用枠や公開範囲の制限がある場合があります。YouTube Studio で実際の結果を確認してください。このガイド作成ではアカウント作成・実認証・動画公開は行っていません。"]
  ]},
  { id: "glossaries", image: "hotwords", title: ["热词表与术语管理","Glossaries and terminology","用語集の管理"], steps: [
    ["热词表页输入表名和任意格式术语，例如“帕克=Puck”“GPK，别名鸡皮开”。启用 AI 规整需 LLM；不启用则本地规则解析，AI 不可用时也会尝试规则回退。保存后查看解析出的词条。","Enter a name and free-form terms in Glossaries, such as “帕克=Puck” or aliases for GPK. AI parsing needs an LLM; otherwise use local rules. If AI is unavailable, parsing attempts a rules fallback. Save and review the entries.","用語集に名前と自由形式の語句（例：「帕克=Puck」、GPK の別名）を入力します。AI 整理には LLM が必要です。無効時はローカル規則で解析し、AI 不可用時も規則への切替を試みます。保存後に内容を確認してください。"],
    ["条目包含词条、别名、读音、译法、类别及备注；可删除错误条目或整个表。勾选至少两张表并填写新名字可合并。当前界面提供查看/删除/重新导入与合并，没有逐格编辑功能。","Entries include terms, aliases, readings, translations, categories and notes. Delete incorrect entries or entire tables. Select at least two tables and a new name to merge. The current UI supports viewing, deleting, reimporting and merging, rather than editing individual cells.","語句・別名・読み・訳・分類・メモを保持します。誤った語句や用語集全体を削除できます。2 件以上を選び新しい名前で結合できます。現在は確認・削除・再入力・結合に対応し、セル単位の直接編集はありません。"],
    ["保存不会自动对所有任务启用；到实时字幕和离线处理页勾选所需表。原始表名及内容按输入保留，不随界面语言变化。","Saving does not enable a glossary for every task. Select the required tables in Live Subtitles and Offline Processing. Table names and entries remain as authored when interface language changes.","保存しただけでは全タスクに適用されません。リアルタイム字幕やオフライン処理で必要な用語集を選択してください。名前や語句は表示言語を変えても原文を保持します。"]
  ]},
  { id: "obs", image: "obs", title: ["独立评论网页与 OBS 浏览器源","Standalone chat pages and OBS browser sources","独立チャットページと OBS ブラウザーソース"], steps: [
    ["先连接评论，然后在 OBS 输出页启动本地服务，默认端口 18990。服务只监听 127.0.0.1；同一电脑的 OBS 和浏览器可访问。选择全部平台或单个平台，来源筛选留空表示全部。","Connect chat, then start the local server in OBS Output (default port 18990). It binds only to 127.0.0.1, so OBS and browsers on the same computer can use it. Choose all platforms or one platform; an empty source filter includes all streams.","チャットを接続してから OBS 出力でローカルサービス（初期ポート 18990）を開始します。127.0.0.1 のみで動作し、同じ PC の OBS・ブラウザーから使えます。全プラットフォームまたは 1 つを選び、配信フィルターを空欄にすると全配信を表示します。"],
    ["指定来源时使用 bilibili:12345 或 youtube:视频ID。选择主题，设置字号、宽度、行数、保留秒数、头像和平台标识；0 秒表示常驻，但仍受最大行数限制。改完重新复制地址。","Filter a source with bilibili:12345 or youtube:video-ID. Set theme, font size, width, row limit, retention seconds, avatars and platform badges. Zero seconds disables timed expiry, while the row limit still applies. Copy the updated URL after changes.","配信指定は bilibili:12345 または youtube:動画ID です。テーマ・文字サイズ・幅・行数・表示秒数・アバター・プラットフォーム名を設定します。0 秒は時間による消去なしですが行数制限は有効です。変更後は URL をコピーし直します。"],
    ["OBS 中添加“浏览器”源，粘贴评论地址，建议宽 420、高 700；字幕单独添加另一个浏览器源，建议 1920×200。评论网页无需主界面即可独立打开，但本地工具必须继续运行。","In OBS add a Browser source and paste the chat URL; start with 420×700. Add a separate Browser source for subtitles at about 1920×200. Chat pages can open in a separate browser window, but the local app must keep running.","OBS でブラウザーソースを追加してチャット URL を貼り、420×700 を目安に設定します。字幕は別のブラウザーソースを 1920×200 で追加します。ページは独立したブラウザーでも開けますが、ローカルアプリは起動したままにしてください。"],
    ["界面预览的棋盘格仅表示透明背景，正式 OBS 地址不含 preview=1。语言参数 lang=zh-CN/en/ja 只控制网页提示，不翻译观众消息。固定端口使重启后的地址保持稳定；更改端口需更新 OBS。","The checkerboard in preview indicates transparency; production URLs omit preview=1. lang=zh-CN/en/ja changes page labels, not viewer messages. A fixed port keeps URLs stable across restarts; update OBS if you change the port.","プレビューの市松模様は透明を示し、本番 URL に preview=1 は付けません。lang=zh-CN/en/ja はページの案内だけを変更し、視聴者の発言は翻訳しません。固定ポートなら再起動後も URL を保てます。ポート変更時は OBS も更新してください。"],
    ["字幕网页支持 size（字号）、hold（毫秒，默认 6000）、room 和 platform 参数。停止指定字幕来源会清除对应字幕。停止叠加层服务后网页会等待重连；应用内教程也使用该本地服务提供离线页面。","Subtitle URLs accept size, hold in milliseconds (default 6000), room and platform. Stopping a subtitle source clears its text. Overlay pages wait to reconnect after the server stops. The in-app guide also uses this server to serve its offline page.","字幕 URL は size、hold（ミリ秒・初期値 6000）、room、platform を指定できます。字幕元を停止すると該当字幕が消えます。サービス停止後は再接続を待ちます。アプリ内ガイドも同じローカルサービスでオフライン提供します。"]
  ]},
  { id: "advanced", title: ["本地文件、自动化与平台边界","Local files, automation and platform scope","ローカルファイル・自動化・対応範囲"], steps: [
    ["Windows 配置和模型位于用户应用数据目录；日志和录播位于你指定的输出位置。目录、房间和常用参数会持久化。备份时保存配置、整场录播文件和热词表；API Key / OAuth 凭据需在另一电脑重新配置。","Windows settings and models live under the user's application data folders; recordings use your chosen output location. Rooms, paths and common options persist. Back up settings, whole sessions and glossaries; configure API/OAuth credentials again on another computer.","Windows の設定・モデルはユーザーのアプリデータ配下、録画は指定の保存先にあります。ルーム・パス・一般設定は保存されます。設定・セッション全体・用語集をバックアップし、別 PC では API/OAuth 資格情報を再設定してください。"],
    ["本地服务的 /api/status 提供评论和录制状态，供本机自动化读取。仓库还有 本地脚本 hook、macOS say 弹幕 TTS、录制与字幕单流复用等后端能力；当前主界面未提供这些开关，本教程不把它们列为可直接点击的功能。","The local /api/status endpoint exposes chat and recording status for automation on this computer. The repository also has local script hooks, macOS say-based chat TTS and shared recording/subtitle stream capabilities, but their controls are not mounted in the current main UI.","ローカル /api/status はチャット・録画状態を同じ PC の自動化向けに提供します。リポジトリには ローカルスクリプトの hook、macOS say の読み上げ、録画・字幕のストリーム共有もありますが、現在のメイン画面に操作スイッチはありません。"],
    ["当前版本主要验证 Windows；macOS/Linux 的依赖安装和凭据库不同。真实 B 站/Google 登录、远端投稿及付费 AI 服务需在自己的账号环境中确认。本教程截图展示实际界面；用于说明流程的数据不代表所有远端服务已验证。","This version was primarily verified on Windows; dependency installation and credential stores differ on macOS/Linux. Verify actual Bilibili/Google sign-in, uploads and paid AI services in your own account environment. Screenshots show the real interface, without implying every remote service was tested.","この版は主に Windows で確認しています。macOS/Linux では依存ツールや資格情報ストアが異なります。実ログイン・投稿・有料 AI は本人の環境で確認してください。画像は実画面ですが、すべての外部サービスの動作確認を意味しません。"]
  ]},
  { id: "troubleshooting", title: ["常见问题与排查顺序","Troubleshooting","困ったとき"], steps: [
    ["启动空白：使用本次打包的可执行文件或安装包，避免直接运行依赖 Vite 的旧开发版。确认 Windows WebView2 可用；若出现界面错误页，记录错误再重新加载。开发者可用 scripts/dev-windows.ps1 启动完整后端开发环境。","Blank startup: use the current packaged executable/installer rather than an old development binary that expects Vite. Ensure WebView2 is available. If an error page appears, record the diagnostic and reload. Developers can use scripts/dev-windows.ps1 for the full backend development environment.","起動時が空白：Vite 前提の古い開発実行ファイルではなく、今回の配布版を使用してください。WebView2 を確認し、エラー画面が出たら内容を記録して再読み込みします。開発時は scripts/dev-windows.ps1 を使用できます。"],
    ["没评论：先在浏览器核实来源正在直播且评论可用，再检查平台/房间筛选、连接状态和网络。没录制：检查输出目录、磁盘空间、FFmpeg/yt-dlp 与直播 URL；查看录制日志，固定视频链接不会跳到另一场直播。","No chat: verify the stream and chat in a browser, then check filters, connection state and network. No recording: check output permissions, free space, FFmpeg/yt-dlp and URL, then read the log. A fixed video URL will not switch to a different broadcast.","チャットなし：ブラウザーで配信・チャットを確認し、フィルター・接続・ネットワークを確認します。録画なし：保存先、空き容量、FFmpeg/yt-dlp、URL、ログを確認します。固定動画 URL は別配信へ切り替わりません。"],
    ["OBS 空白：确认服务已启动、OBS 与工具在同一电脑、端口未冲突；单独打开评论网页的预览模式检查连接。没字幕：确认有语音、模型已下载且路径正确；检查独立的源语种、目标语种和 LLM 设置。","Blank OBS: ensure the server is running on the same computer, the port is available and the source filter is correct. Open a preview page to inspect connection status. No subtitles: check audible speech, downloaded model/path, source/target languages and LLM settings separately.","OBS が空白：同じ PC でサービスが起動しているか、ポート・配信フィルターが正しいかを確認し、プレビューで接続状態を見ます。字幕なし：音声の有無、モデル・パス、原語・翻訳先、LLM 設定を個別に確認します。"],
    ["离线失败或没切片：先处理短 MP4，关闭可选 AI/烧录，验证基础识别；再逐项开启。没有明显峰值时零高能片段是可能结果。可在复盘手动框选；字幕幻觉可通过更大模型、正确语种和更清晰音频改善。","Offline failure or no clips: try a short MP4 with optional AI and burn-in disabled, verify transcription, then enable options one by one. Zero highlights can be valid when no strong peaks exist; select clips manually in Review. Better models, correct language and cleaner audio can reduce hallucinations.","オフライン失敗・切り抜きなし：短い MP4 で AI・焼き込みを無効にし、認識を確認してから 1 項目ずつ有効にします。ピークがなければハイライト 0 件も正常です。手動選択を使えます。認識の誤りはモデル・言語指定・音質で改善する場合があります。"],
    ["投稿或图像生成失败：确认对应的独立登录/密钥、模型权限、配额和服务兼容性；保留错误文本，不要在反馈截图中展示令牌。未知的第三方错误保留原文方便诊断。","Upload or image generation failure: check the separate credentials, model access, quota and API compatibility. Preserve error text while keeping tokens out of screenshots. Unknown third-party diagnostics remain in their original language for troubleshooting.","投稿・画像生成失敗：それぞれの認証情報、モデル権限、利用枠、API 互換性を確認します。エラーは記録し、画像にトークンを載せないでください。未知の外部エラーは診断のため原文のまま表示します。"],
    ["AI 测试失败：检查已保存的地址和模型、密钥来源及服务类型。HTTP 401/403 通常与认证或权限有关；404 常见于路径不正确。网络错误检查代理、证书和服务是否运行；改完必须保存后再测。","AI test failure: inspect the saved URL/model, key source and API type. HTTP 401/403 commonly indicates authentication or permissions; 404 often indicates a wrong path. For network errors, check the proxy, certificates and whether the server is running. Save changes before testing again.","AI テスト失敗：保存済み URL・モデル、キーの取得元、API 種別を確認します。HTTP 401/403 は認証・権限、404 はパスの誤りがよくある原因です。通信エラーではプロキシ・証明書・サーバーの起動状態を確認し、変更を保存して再テストしてください。"]
  ]}
];
