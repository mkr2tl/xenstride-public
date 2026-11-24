# Testing a release build

A release provides `Xenstride-runtime-<version>-windows-x64.zip` (the runtime, `xcl.exe`) and
`Xenstride-runtime-<version>-source.zip` (the exact source it was built from), with
`SHA256SUMS.txt`. This is experimental software for compatibility research.

**Use only games and files you own or are otherwise authorised to use.** The runtime works on
already-unpacked game folders; it does not download, decrypt or unpack game packages and contains
nothing for circumventing DRM, licence or account checks. Do not share game files, dumps or
logs that contain them.

## Before you start

1. Check the download: `Get-FileHash .\Xenstride-runtime-<version>-windows-x64.zip` must print the
   value listed in `SHA256SUMS.txt`.
2. Requirements: Windows 10/11 x64; a D3D12 GPU with Shader Model 6.6 and mesh shaders; DXC
   (`dxcompiler.dll`, `dxil.dll`) from a Windows SDK, or a
   [DirectXShaderCompiler release](https://github.com/microsoft/DirectXShaderCompiler/releases)
   unpacked with `XCL_DXC_DIR` pointing at its `bin\x64`; the Xbox / Gaming Services runtime
   that Windows provides. On Windows 10, the D3D12 Agility SDK (see runtime/README.md). These are
   not part of the download.
3. Free memory: a large title can use tens of GB of commit while it loads; keep the Windows page
   file on automatic.

## Running

```powershell
.\xcl.exe run "D:\Games\MyGame"      # the folder of a game you own (unpacked)
```

The game folder is never modified. Logs and the run report are in
`%LOCALAPPDATA%\XCL\<title>\logs\` (`xcl.log`, `report.json`). For a report, run with
`$env:XCL_ACTIVITY = "1"` first: the log then shows activity and memory every 2 s.

## Reporting

Open an issue with: Windows version, GPU and driver, the release version and its SHA-256, what you
did and what happened, and the relevant lines of `xcl.log` / `report.json` (check them for personal
paths or names first). Do not attach game files, executables, dumps of a game process or recordings
of copyrighted content.

## Known limitations

See [docs/STATUS.md](STATUS.md). In short: the Xenstride project has not validated this release on
any game (upstream XCL's own game list is in runtime/README.md and was not re-checked here); a
newer retail title stops at the end of its world loading; BCPack-compressed textures and some
mesh pipelines are refused; memory use while a world loads is high.
