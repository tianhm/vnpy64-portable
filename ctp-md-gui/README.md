# CTP 行情

期货行情看板。界面用 [nsh](https://github.com/longbridge/nsh)，行情接口用 [ctp2rs](https://github.com/pseudocodes/ctp2rs) 的 Md API。只订阅行情，不下单。

不需要账号或密码。选中经纪商后，程序只用该条目里的 `BrokerID` 做 CTP 行情握手（`ReqUserLogin` 的 UserID、Password 为空）。

云端机器访问不到柜台前置。这里用 `--mock` 验证界面。真实行情在你自己的 Windows 机器上编译运行。本仓库没有附带可执行文件。

## 在本机运行

Windows 10/11 x64。

1. 安装 [rustup](https://rustup.rs) 的 stable 工具链（nsh 需要 rustc 1.85 或更新）。
2. 安装 Visual Studio Build Tools，勾选「使用 C++ 的桌面开发」（MSVC 和 Windows SDK）。
3. 安装 LLVM，并把 `clang` 加进 PATH（bindgen 要 libclang）。例如 `winget install LLVM.LLVM`。
4. 拉代码并编译：

```bat
git clone https://github.com/tianhm/vnpy64-portable.git
cd vnpy64-portable
git fetch origin cursor/ctp-md-gui-a66f
git checkout cursor/ctp-md-gui-a66f
cd ctp-md-gui
cargo run --release -- --mock
```

`--mock` 不访问网络，板上会有 `rb2610` 和 `ag2612`。

接柜台：

```bat
cd ctp-md-gui
cargo run --release
```

把 CTP **6.7.7 评测版** 的 `thostmduserapi_se.dll` 放到 `ctp-md-gui` 目录（`cargo run` 的当前目录），或放到 `target\release\` 里 exe 的旁边，或：

```bat
cargo run --release -- --dynlib C:\ctp\thostmduserapi_se.dll
```

`cargo build` 下载的 ctp2rs 检出里有一份和默认头文件匹配的库，可直接拷贝：

```text
%USERPROFILE%\.cargo\git\checkouts\ctp2rs-*\*\ctp-dyn\api\ctp\v6.7.7\v6.7.7_20240607_traderapi64_se_windows\thostmduserapi_se.dll
```

然后：

1. 在左侧搜索并点选经纪商（例如「渤海」）。
2. 点选服务器，或按 Enter，再按 **连接**。程序按该服务器 `MarketData` 的顺序试前置，并显示当前连上的那一个。
3. 在合约框输入代码，按 **订阅**。**退订** 去掉当前选中的合约。

内置 `broker.xml` 已经够用。若要改用本机另一份 Shinny 列表（UTF-8 或 GB2312）：

```bat
cargo run --release -- --broker-xml C:\Users\tianhm\AppData\Roaming\Q73N_shinny_22009_beta\20261022200046\broker.xml
```

只连柜台、前置失败时不要退回模拟：

```bat
cargo run --release -- --ctp
```

## 模拟和真实行情

| 方式 | 命令 | 行为 |
| --- | --- | --- |
| 模拟 | `cargo run --release -- --mock` | 不加载动态库，不访问前置 |
| 自动 | `cargo run --release` | 先试所选服务器的行情前置，都不通时改用模拟，并写明原因 |
| 仅柜台 | `cargo run --release -- --ctp` | 失败就停在「连接失败」 |

环境变量与命令行相同：`CTP_MD_MODE=mock|ctp|auto`，`BROKER_XML`，`CTP_MD_DYNLIB`，`CTP_MD_FLOW`，`CTP_MD_INSTRUMENTS`（逗号分隔），`CTP_MD_TICK_MS`，`CTP_MD_FRONT_TIMEOUT_SECS`，`CTP_MD_REPLAY`，`CTP_MD_CONFIG`。也可以把 `config.example.toml` 复制为 `config.toml` 再 `--config config.toml`。不要把账号、密码或授权码写进配置。

快捷键：`j` / `k` 或上下键移动，左右键换栏，Enter 在经纪商栏进入服务器、在服务器栏连接，`u` 退订。

## CTP 动态库

默认按 **CTP 6.7.7**（非 union）编译。`CreateFtdcMdApi` 是 `(const char* flow, bool udp, bool multicast)`。ctp2rs 要的文件名：

| 系统 | 文件 | 放哪里 |
| --- | --- | --- |
| Windows | `thostmduserapi_se.dll` | 当前目录，或 exe 同目录，或 `CTP_MD_DYNLIB` |
| Linux | `thostmduserapi_se.so` | 同上。若还有依赖的 `.so`，把该目录加进 `LD_LIBRARY_PATH` |
| macOS | `thostmduserapi_se.dylib`（或官方 framework 里的 `thostmduserapi_se`） | `CTP_MD_DYNLIB` 指到具体文件 |

流文件目录默认是运行目录下的 `md_flow/`（已 gitignore）。

不要用本仓库自带的 `Win64\vnctpmd.pyd` 或 `Linux64\thostmduserapi.so`。那是 vn.py 时代大约 6.3.5 的库，结构体布局和 6.7.7 对不上，加载后会崩。

6.7.11 / 6.7.13 的创建函数多一个生产模式参数。换库时要同时换编译特性，避免和默认的 6.7.7 叠在一起：

```bat
cargo run --release --no-default-features --features ctp_v6_7_11
```

`ctp_v6_7_13` 同样写法。这两版按生产模式创建 API（和 ctp2rs 的示例一致）。

Linux 上先看模拟是否起来：

```bash
cd ctp-md-gui
cargo test
cargo run -- --mock
```

Linux 还需要 GTK 4 开发包和 libclang（`libgtk-4-dev`、`libclang-dev`）。

## broker.xml

根下多个 `broker`：`BrokerID`、`BrokerName`、`BrokerEName`，可选 `ApiType`。`UFX`、`XONE` 以及其他不是 `CTP` 的 `ApiType` 会跳过。没有 `ApiType` 的当作 CTP。

`Servers/Server/Name` 是服务器名。`MarketData/item`（大小写不敏感，因此包含金瑞期货的 `MarKetData`）是 `host:port`，程序会补上 `tcp://`。`Trading/item` 是交易前置，不用。

随包的 `broker.xml` 是 UTF-8，内容和上传的 GB2312 列表相同，但去掉了 `AuthCode2` / `AuthCode3`。解析器遇到这些节点会忽略，不会显示，也不会提交。

## 开发

```bash
cd ctp-md-gui
cargo test
cargo clippy --all-targets -- -D warnings
```

`cargo test` 里的界面测试走 `--mock` 同一条 UI，不连接柜台。
