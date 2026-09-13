# AI 设置与 Windows 分发检查

检查日期：2026-09-13。版本：0.1.0，Windows x64。

本次完成统一的文本、视觉和图片生成配置，并更新中英日教程。检查未发现个人 API Key 被写入源码、教程或分发文件。测试使用本机模拟服务和专门创建的虚拟密钥，没有调用付费 AI 服务。

## 密钥与安装包

- 新版 AI Key 存在 Windows 系统凭据库，按用途、服务类型及规范化地址隔离；普通 `settings.json` 只保存公开配置。应用在运行时读取密钥。
- 检查了构建脚本、Vite/Tauri 配置、源码、前端产物、教程、程序和 DLL、NSIS/MSI 安装包及生成的安装清单。没有找到将环境 Key 编译进程序的配置。
- 在内存中读取项目环境文件、相关环境变量、应用配置及仅属于本工具的系统凭据，用 UTF-8、UTF-16 和 JSON 编码比对；审计日志只保存数量和文件路径，不输出密钥值。并检查常见私钥格式。
- 清理前唯一的工具凭据是本次创建的虚拟 Key，产物中精确匹配为 0；清理后工具凭据为 0。此次检查范围内未发现个人 Key。
- 前端检测到的 YouTube 客户端常量已与 `youtubei.js` 依赖中的公开常量逐值对照，不是用户个人 API Key。匿名 YouTube 评论读取不要求用户填写 Key。
- MSI 经只读数据库接口提取并解压内部 CAB，没有执行安装。有效载荷为应用 EXE 和 DLL，没有 `.env`、用户 `settings.json`、凭据备份、录播或模型。DLL 与构建产物 SHA-256 相同；EXE 仅安装类型标记的 3 个字节不同（`UNK` / `MSI`），其余一致。
- NSIS 生成清单只打入主 EXE；另有依条件执行的 WebView2 安装逻辑，没有用户目录或配置文件资源。检查结合了构建输入和安装清单；并未把压缩包的原始字节扫描视为完整解包检查。
- GitHub 推送前补充运行 Gitleaks v8.30.1：待提交改动与本地 31 个历史提交均未检测到泄露密钥，扫描报告使用完全脱敏。工具下载包已核对官方 SHA-256 校验值。不对未检查的外部目录或历史分发包作结论。

## 验证结果

| 检查 | 结果 |
| --- | --- |
| `npm test` | 20 个文件，107 个测试通过 |
| `cargo test --workspace --offline` | 381 个测试通过，5 个网络/模型相关测试忽略 |
| `npm run build` | TypeScript 与前端生产构建通过 |
| Windows release 与 NSIS/MSI 构建 | 成功生成两种安装包 |
| Windows 实机设置 | 中英日布局、保存、重启读取、清除虚拟 Key 通过 |
| 文本/视觉测试 | 文本认证请求、视觉复用、独立视觉无认证请求通过本机模拟服务验证 |
| 图片测试 | 模型查询通过；未调用图片生成或编辑 |
| 失败提示 | HTTP 401 正确显示，不回显服务端响应中的密钥 |
| 配置恢复 | 恢复调试前的 4 项公开设置，其他设置保持原值；虚拟 Key 已删除 |
| 三语教程 | 20 章、31 张实际界面截图；语言切换、搜索和长截图放大已检查 |
| 内置教程 | 最终程序的 `/guide` 响应与 `public/guide/index.html` 逐字节相同，14,719,726 字节 |

Windows Node 在重复执行构建前置命令时曾出现 libuv assertion。交付构建先单独完成 `npm run build`，再用仅位于 `target` 的临时配置跳过重复的 `beforeBuildCommand`，执行 Tauri 的 NSIS/MSI 打包；正式 Tauri 配置未因此修改。

已执行要求的 `cargo fmt --all -- --check` 和 `cargo clippy --workspace --all-targets -- -D warnings`。全量检查被已有的格式差异和警告阻断，例如 account/credentials、asr/audio 的格式，以及 recorder/danmaku/highlight 的测试模块警告；本次修改的 Rust 文件已格式化。不能将全量 fmt/clippy 标为通过。

真实服务的账号授权、额度、模型权限、图像生成/编辑及远端投稿没有在本次测试中验证。连接测试成功代表该测试请求成功，不能代替实际服务兼容性和输出质量检查。

## 交付文件校验

| 文件 | SHA-256 |
| --- | --- |
| `target/release/bundle/nsis/vtb-toolkit_0.1.0_x64-setup.exe` | `ab53c3fe734ec34a80f0955a1af0bcd40a386f302ef534c3a99b83b2ddc0f27b` |
| `target/release/bundle/msi/vtb-toolkit_0.1.0_x64_en-US.msi` | `6be597249e1bbd48534df66b82d3ee31cadd1459b4e371cd8f0cfbbc428add6d` |
| `public/guide/index.html` | `862cd039516f2e969e83a8ce3bf7e949a70ac781da7bbaeb4f41343db692deca` |

本地详细检查日志位于未分发的 `target/ai-*.log`、`target/ai-secret-audit*.json`。模拟服务已停止。
