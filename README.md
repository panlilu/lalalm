# LalaLM — 本地大模型管理器

一个用 **Tauri 2** 编写的本地 AI 大语言模型管理工具，类似 [unsloth](https://github.com/unslothai/unsloth) 的模型中心 + LM Studio 的本地库管理，内置 **aria2c** 高速下载器，开箱即用。

![platform](https://img.shields.io/badge/platform-macOS%20(arm64%20%2F%20x64)-blue) ![framework](https://img.shields.io/badge/Tauri-2.x-orange)

## 功能特性

### 🔍 发现模型（Discover）
- 搜索框带占位提示、最近搜索与热门推荐下拉
- 一键搜索 **Hugging Face / hf-mirror（国内镜像）/ ModelScope（魔搭）** 三大来源
- 排序：最多下载 / 热门趋势 / 最多点赞 / 最近更新；可开关「仅 GGUF」过滤
- 模型详情页：量化文件列表（Q4_K_M、Q5_K_M、Q8_0…自动识别并排序）、推荐量化一键下载、README 渲染、文件大小 / 参数量 / 更新时间等信息

### 📦 本地模型（On Device）
- 自动扫描并汇总多个目录：
  - LalaLM 自有模型库
  - Hugging Face 缓存（`~/.cache/huggingface/hub`，兼容 `hf` CLI / transformers）
  - LM Studio（`~/.lmstudio/models` 及旧版 `~/.cache/lm-studio/models`）
  - ModelScope 缓存（`~/.cache/modelscope/hub`）
  - llama.cpp 等任意**自定义搜索路径**
- 解析 GGUF 头信息：架构、量化版本、上下文长度、层数等
- 存储统计：各缓存目录占用一目了然
- 管理：在 Finder 中显示 / 打开文件夹 / 移动到… / 删除（移入废纸篓）/ 批量操作

### 🚀 下载任务
- 内置 aria2c sidecar（随应用打包，无需安装），多线程分块加速
- 实时进度监视页：进度条、速度、ETA、总速度
- 支持 **暂停 / 继续 / 取消 / 重试（断点续传）**，重启应用后可续传
- 下载历史记录持久化

### 🖥 系统状态
侧边栏实时显示 CPU、内存、GPU/显存（Apple Silicon 显示统一内存）、磁盘剩余空间。

### ⚙️ 设置
- 下载来源切换 + Hugging Face / ModelScope Token
- 自定义下载位置（默认 `~/.lalalm/models`）
- aria2c 并发配置：单服务器连接数、分块数、最小分块大小、同时下载数
- 各缓存扫描开关与自定义路径管理

## 开发

```bash
pnpm install          # 安装前端依赖
pnpm fetch:aria2      # 构建内置 aria2c（仅首次，见下方说明）
pnpm tauri dev        # 开发运行（自动启动 Vite + 应用，支持热更新）
pnpm tauri build      # 打包 .app / .dmg
```

> ⚠️ **白屏说明**：Tauri 的 **debug 构建**（`cargo build` 产物、`target/debug/lalalm`）
> 不内嵌前端资源，而是加载 `http://localhost:5173`。直接运行它会因连接不上 dev
> server 而**白屏**（inspect 可见 `Failed to load resource: Could not connect to the server`）。
> - 日常开发请用 `pnpm tauri dev`
> - 想双击直接用，请执行 `pnpm tauri build`，打开产物
>   `src-tauri/target/release/bundle/macos/LalaLM.app`（release 构建已内嵌全部资源）

### 内置 aria2c 说明

`scripts/fetch-aria2.sh` 会从源码构建 aria2c 1.37.0（使用 Apple TLS，不依赖任何第三方动态库，
产物只链接 macOS 系统库），输出到 `src-tauri/binaries/aria2c-{target-triple}`，
由 Tauri `externalBin` 机制打包进 `.app/Contents/MacOS/`。运行时若找不到内置二进制，
会回退使用系统 PATH 中的 aria2c（便于开发调试）。

Windows 支持预留：脚本支持 `WINDOWS=1 bash scripts/fetch-aria2.sh` 预先下载静态版
aria2c（abcfy2 静态构建），Tauri 代码已做跨平台路径处理，后续可平滑适配。

### Windows 版本

代码已全平台适配：系统代理走注册表（`HKCU\...\Internet Settings` 的 ProxyEnable /
ProxyServer），GPU 名称经 PowerShell CIM 探测，文件管理器用 `explorer`，图标为
`icons/icon.ico`（256/48/32/16 多尺寸），aria2c 使用官方静态 win64 构建
（`src-tauri/binaries/aria2c-x86_64-pc-windows-msvc.exe`，已随仓库准备）。

在 Windows 机器上本地构建：

```powershell
pnpm install
bash scripts/fetch-aria2.sh        # Git Bash 下自动下载 Windows 版 aria2c
pnpm tauri build --bundles nsis    # 产出 NSIS 安装程序
# → src-tauri/target/release/bundle/nsis/LalaLM_0.1.0_x64-setup.exe
```

也可以直接用 GitHub Actions：推送 `v*` 标签或手动触发 `build` 工作流
（`.github/workflows/build.yml`），会同时产出 macOS（.app/.dmg）与 Windows（NSIS
安装包）工件供下载。

#### 在 macOS 上本地交叉构建 Windows exe

已验证可行（mingw-w64 路线）。一次性环境准备：

```bash
rustup target add x86_64-pc-windows-gnu
brew install mingw-w64 makensis
# tauri-cli 需要的 NSIS 打包环境（Windows 版 nsis + 原生 makensis + 插件）：
mkdir -p ~/Library/Caches/tauri/NSIS/Plugins/x86-unicode
curl -fL -o /tmp/nsis3.zip https://github.com/tauri-apps/binary-releases/releases/download/nsis-3/nsis-3.zip
python3 -m zipfile -e /tmp/nsis3.zip ~/Library/Caches/tauri/NSIS/
N=~/Library/Caches/tauri/NSIS
mv "$N"/nsis-3.08/* "$N/" && rm -rf "$N/nsis-3.08"
cp "$(brew --prefix)/bin/makensis" "$N/Bin/makensis"
curl -fL -o /tmp/appid.zip https://github.com/tauri-apps/binary-releases/releases/download/nsis-plugins-v0/NSIS-ApplicationID.zip
curl -fL -o /tmp/nsproc.zip https://github.com/tauri-apps/binary-releases/releases/download/nsis-plugins-v0/NsProcess.zip
python3 -m zipfile -e /tmp/appid.zip /tmp/appid_x && python3 -m zipfile -e /tmp/nsproc.zip /tmp/nsproc_x
find /tmp/appid_x /tmp/nsproc_x -name "*.dll" -exec cp {} "$N/Plugins/x86-unicode/" \;
```

之后随时构建（sidecar 会自动使用 `binaries/aria2c-x86_64-pc-windows-gnu.exe`，
仓库内已有该文件的拷贝）：

```bash
pnpm tauri build --target x86_64-pc-windows-gnu --bundles nsis
# → src-tauri/target/x86_64-pc-windows-gnu/release/bundle/nsis/LalaLM_0.1.0_x64-setup.exe
```

> 注意：mingw（gnu 目标）构建的 exe 与官方 MSVC 构建在运行上没有区别，都依赖系统
> WebView2 Runtime（Win10+ 一般自带；缺失时安装器会引导安装）。代码签名需在
> Windows 主机上执行。

## 技术栈

| 层 | 技术 |
| --- | --- |
| 前端 | React 18 + TypeScript + Vite，手写 CSS 设计系统（暗色主题） |
| 后端 | Rust + Tauri 2，tokio 异步运行时 |
| 下载 | aria2c JSON-RPC（进程内托管，`--stop-with-process` 防孤儿进程） |
| 模型解析 | 自实现 GGUF header 解析（量化 / 架构 / 上下文长度） |
| 系统信息 | sysinfo + system_profiler（GPU 探测） |

## 目录结构

```
├── scripts/
│   ├── fetch-aria2.sh     # 构建/获取内置 aria2c
│   └── gen-icon.mjs       # 生成应用图标 (PNG/icns)
├── src/                   # React 前端
│   ├── pages/             # Discover / ModelDetail / OnDevice / Downloads / Settings
│   ├── components/        # Sidebar / Markdown / 图标等
│   └── store.tsx          # 全局状态 + Tauri 事件订阅
└── src-tauri/
    ├── binaries/          # aria2c sidecar（fetch:aria2 生成）
    └── src/
        ├── hub.rs         # HF / hf-mirror / ModelScope API
        ├── aria2.rs       # aria2c 进程管理 + JSON-RPC
        ├── downloads.rs   # 下载任务管理 / 断点续传 / 事件推送
        ├── registry.rs    # 本地模型扫描与管理
        ├── gguf.rs        # GGUF 元数据解析
        └── stats.rs       # CPU/RAM/GPU/磁盘
```

## 路线图

- [ ] Windows 适配（aria2c 静态二进制已就绪，需补窗口/托盘细节）
- [ ] 下载队列优先级与限速设置
- [ ] Ollama / safetensors 目录识别增强
- [ ] 模型卡对比视图
