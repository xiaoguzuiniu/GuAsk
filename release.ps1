# 本地打包 release 版安装包（带自动更新签名）
# 用法：在项目根目录运行  .\release.ps1
$ErrorActionPreference = "Stop"
$key = "$env:USERPROFILE\.tauri\guask.key"
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content $key -Raw
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""
npx.cmd -y @tauri-apps/cli@2 build
Get-ChildItem src-tauri\target\release\bundle\nsis | Select-Object Name, @{n="MB";e={[math]::Round($_.Length/1MB,1)}}