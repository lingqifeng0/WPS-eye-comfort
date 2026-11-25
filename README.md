# WPS 护眼控制器（WPS Eye Comfort）

轻量级 **纯 Rust + Win32 + Direct2D/DirectWrite** 编写的 Windows WPS Office 原生深色外观与护眼调色工具。  
零 Python、零 Electron、零 WebView2，单文件开箱即用，支持 **WPS 文字 / 演示（PPT）/ 表格** 三端原生深色皮肤与实时文档页内护眼调色。

![WPS 护眼控制器控制面板](docs/preview-panel.png)

## 核心特性

- **极小单文件体积**：[WPS-Eye-Comfort-Rust.exe](WPS-Eye-Comfort-Rust.exe) 仅 **0.93 MB**，已内置 32 位原生辅助 DLL、校验清单与双态托盘图标，直接双击运行。
- **超低内存托盘常驻**：点关闭 `×` 或「收至托盘」自动隐藏面板并释放 Direct2D/DirectWrite 渲染资源，保持 WPS 深色生效；静默托盘常驻工作集仅 **0.25–3.5 MB**。
- **原生 2025 深色外观（文字 · PPT · 表格）**：动态生成 `.kuip` + Qt RCC 深色皮肤并调用 WPS 原生换肤接口，保留原版图标、功能区与下拉菜单，无遮罩、无闪烁，三端同开不回弹。
- **文字文档页内 4 滑块实时调色**：
  - 独立调节 **亮度 / 对比度 / 棕褐色 / 灰度**，内置 WCAG ≥ 4.5:1 对比度兜底；
  - 支持 **护眼深底** 与 **柔和浅底（兼容固定黑字）** 一键切换，附带自动文字与固定纯黑字双行实时可读性预览。
- **开机静默自启（免管理员权限）**：面板底部与托盘右键均支持一键开关，写入当前用户启动项 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`，开机静默驻留托盘并在后续打开 WPS 时自动延续上次开关状态。
- **严格校验与无损恢复**：启动前对目标 WPS 二进制执行 SHA-256 + RVA 特征校验，绝不修改 WPS 安装文件与文档内容；退出或恢复时忠实还原启用前的原生皮肤、护眼模式与显示选项。

![WPS 原生深色实景效果](docs/preview-wps-dark.png)

## 快速使用

1. 直接下载并双击根目录的 [WPS-Eye-Comfort-Rust.exe](WPS-Eye-Comfort-Rust.exe)（建议先放到常用固定目录）。
2. 打开 WPS 文字、演示（PPT）或表格，在面板中开启 **原生深色外观** 或 **文字页面调色**。
3. 点击右上角 `×` 或 **收至托盘** 隐藏窗口，WPS 深色与护眼效果保持生效；右键托盘图标可随时快捷切换、开启 **开机自动启动（静默托盘）** 或 **恢复原版并退出**。

### 命令行参数

| 参数 | 说明 |
| --- | --- |
| `--minimized` | 启动后直接隐藏到系统托盘（不弹窗、延迟加载图形引擎） |
| `--auto-start` | 自动延续上次保存的「原生深色外观」与「文字页面调色」开关状态 |
| `--enable-skin` | 启动时强制开启三端原生深色外观 |
| `--enable-page` | 启动时强制开启 WPS 文字页面调色 |

## 适用环境与边界说明

- **已实测验证环境**：Windows 10 / 11 + **WPS Office 12.8.2.19823（32 位 x86）**。
- **安全版本锁定**：原生接口强依赖二进制校验清单 `native/v3/manifest.json`；若本地 WPS 版本不一致，程序会主动拒绝注入并给出提示，避免崩溃或异常。
- **文档内固定颜色文字**：WPS 原生页面颜色接口仅控制纸张底色与「自动颜色」文字，文档内显式设为纯黑 `#000000` 或彩色的文字保持原值；遇到含大量固定黑字的文档时，可直接切换面板中的 **柔和浅底（兼容固定黑字）**。
- 配置文件与临时生成的深色皮肤缓存位于 `%LOCALAPPDATA%\WpsNativeEye`，不触碰 WPS 安装目录。

## 目录结构

```text
WPS-eye-comfort/
├── WPS-Eye-Comfort-Rust.exe     # 独立单文件成品（0.93 MB，开箱即用）
├── wps-eye-rust/                # 纯 Rust + Direct2D + Win32 托盘控制器源码
│   ├── Cargo.toml
│   ├── build.rs                 # 编译并嵌入图标与 DPI 清单资源
│   ├── assets/                  # 矢量生成的多尺寸双态图标与 Win11 清单
│   └── src/
│       ├── main.rs              # 窗口消息循环、单实例与生命周期管理
│       ├── ui_d2d.rs            # Direct2D / DirectWrite 暗色 Fluent 面板
│       ├── tray.rs              # 原生系统托盘、双态图标与 TaskbarCreated 恢复
│       ├── autostart.rs         # HKCU 注册表开机自启与任务管理器状态检测
│       ├── controller.rs        # 后台 COM 线程、偏好记忆与会话恢复账本
│       ├── native_agent.rs      # 32 位进程模块校验、CNG SHA-256 与协议 v4 通信
│       ├── skin_builder.rs      # 纯 Rust 2025 .kuip + Qt RCC v2 深色皮肤生成器
│       ├── palette.rs           # 四维正交护眼调色与 WCAG 对比度算法
│       └── wps_bridge.rs        # WPS 文字原生 COM 对象模型桥接
├── native/                      # 32 位 C++ 原生辅助 DLL 源码与产物
│   ├── build-v3.ps1             # MSVC x86 / Zig 构建脚本
│   ├── v3/                      # 已编译产物 WpsNativeEyeV3.dll 与 manifest.json
│   ├── v3-src/                  # C++ 源码（协议 v4 换肤与原子页内显示选项）
│   └── vendor/minhook/          # MinHook 源码及 BSD 许可
├── build_native_manifest.py     # WPS 导出函数、RVA 字节签名与 SHA-256 清单生成脚本
├── docs/                        # README 预览截图
└── LICENSE
```

## 从源码编译

### 1. 构建 32 位原生 DLL（可选，仓库已附带编译好的 `native/v3/WpsNativeEyeV3.dll`）

需要安装 Visual Studio Build Tools（MSVC x86）与 Python 3.12（含 `pefile`）：

```powershell
powershell -ExecutionPolicy Bypass -File .\native\build-v3.ps1
```

### 2. 构建纯 Rust 单文件 EXE

需要安装 Rust 工具链与 Windows SDK（`rc.exe` / `cvtres.exe`）：

```powershell
cd wps-eye-rust
cargo test
cargo build --release
Copy-Item -Force target\release\wps-eye-comfort.exe ..\WPS-Eye-Comfort-Rust.exe
```

## 开源许可

本项目原创代码采用 [MIT License](LICENSE) 开源；第三方依赖 MinHook 遵循 [native/vendor/minhook/LICENSE.txt](native/vendor/minhook/LICENSE.txt) 中的 BSD 许可。WPS Office 及相关商标归金山软件所有。
