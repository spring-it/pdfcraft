# 沃飞翼览 PDF v0.10.2：Windows 当前用户 MSI

This dedicated release branch starts at the upstream `v0.10.1` tag. It builds **two MSI packages**, `x64` and `x86`, that install for the **current user** under:

```text
%LOCALAPPDATA%\pdfcraft
%LOCALAPPDATA%\pdfcraft\models
%LOCALAPPDATA%\pdfcraft\PdfCraftData
```

`PdfCraftData` is created when the app first runs. The MSI includes the repository's `portable.txt` marker so settings, logs, crash recovery and newly created digital IDs remain in this non-roaming LocalAppData tree. Uninstall intentionally leaves user data untouched.

## Fully unattended install

Use the MSI matching the Windows architecture:

```powershell
msiexec.exe /i "pdfcraft-0.10.2-windows-x64-localappdata.msi" /qn /norestart /L*v "%TEMP%\pdfcraft-install.log"
msiexec.exe /i "pdfcraft-0.10.2-windows-x86-localappdata.msi" /qn /norestart /L*v "%TEMP%\pdfcraft-install.log"
```

Choose **one** of these commands, not both. `/qn` suppresses the installer UI entirely; no Next or Finish clicks are required. Without `/qn`, this MSI still has **no authored installer wizard**, although Windows may show system progress or security prompts depending on policy and file signing. The MSI does not need elevation for a normal per-user install.

Uninstall without prompts:

```powershell
msiexec.exe /x "pdfcraft-0.10.2-windows-x64-localappdata.msi" /qn /norestart
```

For an x86 install use the x86 MSI in the uninstall command. Exit status 0 means success; 3010 means Windows requests a reboot.

This is a **separate per-user installer** with its own UpgradeCode. It does not upgrade or remove the original per-machine PdfCraft MSI. It automatically creates both a **per-user desktop shortcut named `沃飞翼览 PDF`** and a per-user Start Menu shortcut—even with `/qn` and no user interaction. Both shortcuts are removed on uninstall. It makes no machine-wide PDF associations, Explorer context-menu entries or HKLM changes. It is suitable for managed silent deployment and does not install MCP or any background service.

The red PDF document icon supplied for this release replaces the earlier animal logo throughout app resources: Windows EXE/taskbar/MSI/shortcut, embedded document icon, runtime PNG, and Linux/macOS icon assets. The earlier `v0.10.1-localappdata.1` release is preserved unchanged.

## Build and release

On the `v0.10.1-localappdata-msi` branch, `.github/workflows/localappdata-msi.yml` builds both architectures on Windows runners, checks MSI metadata, tests real `/qn` installs and uninstalls, then publishes exactly the two MSI assets to the **prerelease** `v0.10.2-wofei.1`. The original v0.10.1 release is not modified.

Locally, install Rust MSVC targets and WiX 5.0.2, then run:

```powershell
pwsh packaging/windows/package.ps1 -Arch x64 -LocalAppDataMsi
pwsh packaging/windows/package.ps1 -Arch x86 -LocalAppDataMsi
```

Signing is optional in the existing packaging script; if no signing credentials are configured, Windows may display SmartScreen warnings for the unsigned installers. `/qn` controls Windows Installer UI, not OS-enforced security warnings.


## 沃飞翼览 PDF 定制版（v0.10.2）

- 用户可见名称为“沃飞翼览 PDF”；Rust crate、PDF CLI 和安装目录仍保持 pdfcraft，避免破坏原 MSI 升级路径。
- 首页已去掉 ArtCraft 社区卡片；工具栏已去掉 Discord 入口；首页增加“添加水印”一键入口：未打开文档时先选择 PDF，成功打开后直接进入水印对话框。
- 水印参考“pdf水印工具加强版.html”：中文或英文文字、本机字体、常用模板、透明度、角度、颜色、单个/平铺、九宫格定位、水平/垂直间距、奇偶页及自定义页范围。
- 中文水印使用系统字体轮廓先渲染为透明 PNG，然后使用现有 PDF 水印图片管线嵌入，不打包、不上传受版权保护的系统字体；缺少汉字字形时给出提示。
- 原 ArtCraft 和 PdfCraft 项目版权、MIT/Apache-2.0 条款、NOTICE 等继续保留；所有文档均为本机操作。
