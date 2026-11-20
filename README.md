# 合票 · PayMerge-PDF

发票、购买记录、支付记录，合成一份 A4 PDF。Rust 原生 Windows 程序，极简水墨界面，**只需一个 EXE**。

[下载单文件版](https://github.com/ZhangStudyLife/PayMerge-PDF/releases/latest/download/PayMerge-PDF.exe) · [使用说明](docs/使用说明.md) · [7.2 测试报告](docs/7.2-测试报告.md)

![合票](docs/previews/workspace-ready.png)

## 使用

下载 `PayMerge-PDF.exe` 即可运行，无需安装 Python 或 Rust，也不需要旁置 DLL、升级器、字体、说明或许可文件。

在“设置”填写姓名、组别及保存位置。点击“添加”或拖入一份 PDF 与两张截图，截图按购买、支付的顺序添加。核对票号、金额，点击“合并 PDF”。缺失或冲突的关键字段需要补填并确认；图片型 PDF 通过人工填写完成输出，不包含 OCR。

保留原有多页排布、A4 / 300 DPI / 10 mm 边距、等比缩放和自动命名。文件名为“姓名_组别_金额_票号后四位.pdf”，重名可覆盖、加序号或取消。票据在本地处理，不上传。

## 单文件与设置

分发物和程序目录只需要一个 EXE。PDFium、中文字体、毛笔字标题字体和第三方许可均内嵌；运行时将 PDF 引擎校验并加载到用户缓存，配置保存在 `%LOCALAPPDATA%\PayMerge-PDF`。字体及许可在内存中读取，“设置 → 许可”可以查看许可全文。程序目录不会生成 DLL、升级器、许可文件或新的 `config.json`。

旧版 `config.json` 可在第一次启动时从 EXE 旁自动迁移到用户数据目录，保留姓名、组别和输出路径规则；旧配置原件不删除。

## 更新

启动时和运行期间每 6 小时检查本仓库正式 GitHub Releases，也可点击“更新”。下载单 EXE、核对 GitHub SHA256 后，点击“重启并升级”。程序复制自身作为临时升级进程，等待旧实例退出后替换原 EXE，并校验新版版本号和内嵌 PDF 引擎；失败时恢复原文件。个人设置和输出 PDF 不参与替换，升级暂存文件完成后清理。支持中文、空格路径及重命名后的 EXE。

**7.1 及更早版本首次切换到单文件版需要手动下载。** 旧版本采用 ZIP 更新协议；7.2 起使用 EXE 更新协议。之后发布新版时，更新 `Cargo.toml` 版本号，创建对应 `vX.Y.Z` 正式 Release 并上传 `PayMerge-PDF.exe`，客户端即可发现并校验。

## 构建与测试

开发和 Windows 交叉编译在 Linux x64 服务器完成。工具链、依赖缓存、临时文件和构建产物均在项目内，不修改全局 shell 配置。

```bash
bash scripts/setup.sh
source scripts/env.sh
bash scripts/check.sh
bash scripts/build.sh
bash scripts/build-windows-tests.sh
cargo build --locked --target x86_64-pc-windows-gnullvm --examples
```

固定 Rust 1.100.0、LLVM-MinGW 20261117 UCRT、目标 `x86_64-pc-windows-gnullvm`、eframe 0.37.0、pdfium-render 0.9.5、PDFium 8125；其余依赖见 `Cargo.lock`。发布启用优化、LTO 和符号剥离。`scripts/prepare_bundle.py` 在构建前压缩 PDFium、字体和许可，产物为 `dist/PayMerge-PDF-X.Y.Z-windows-x64/PayMerge-PDF.exe`。

Windows 在项目映射目录运行 `scripts/test_windows.ps1` 和 `scripts/test_single_file.ps1 -Source local`。正式发布后可用 `-Source github` 验证实际下载、升级与回滚。脚本将用户数据及运行缓存隔离到项目 `.tmp`，并在结束时清理；无需安装 Windows 编译环境。`PAYMERGE_DATA_DIR` 用于测试隔离，正常运行无需设置。

UI 通过 egui_kittest 离屏渲染检查。测试不使用 Computer Use 或系统鼠标键盘注入。真实票据、个人配置及开发缓存不入库。字体来源和子集记录见 `assets/font-source.json` 与 `assets/ink-font-source.json`。
