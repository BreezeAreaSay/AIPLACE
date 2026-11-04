# Triad Canvas installer for Windows (x64).
#   irm https://raw.githubusercontent.com/BreezeAreaSay/AIPLACE/main/install.ps1 | iex
# Env: $env:TRIAD_VERSION = "v0.2.0" (pin), $env:TRIAD_INSTALL_DIR, $env:TRIAD_NO_SETUP = "1"
$ErrorActionPreference = "Stop"
$Repo = "BreezeAreaSay/AIPLACE"
$Bin = "triad"
$Target = "x86_64-pc-windows-msvc"
$InstallDir = if ($env:TRIAD_INSTALL_DIR) { $env:TRIAD_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "triad\bin" }

if ($env:TRIAD_VERSION) { $Tag = $env:TRIAD_VERSION } else {
  Write-Host "▸ ищу последний релиз…"
  $Tag = (Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest" -Headers @{ "User-Agent" = "triad-installer" }).tag_name
}
$Ver = $Tag.TrimStart("v")
$Name = "$Bin-$Ver-$Target"
$Url = "https://github.com/$Repo/releases/download/$Tag/$Name.zip"
$Tmp = Join-Path ([IO.Path]::GetTempPath()) ("triad-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $Tmp | Out-Null
Write-Host "▸ скачиваю $Url"
Invoke-WebRequest $Url -OutFile (Join-Path $Tmp "pkg.zip")
Expand-Archive (Join-Path $Tmp "pkg.zip") -DestinationPath $Tmp -Force
New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
Copy-Item (Join-Path $Tmp "$Name\$Bin.exe") (Join-Path $InstallDir "$Bin.exe") -Force
Remove-Item $Tmp -Recurse -Force
Write-Host "▸ установлено: $InstallDir\$Bin.exe ($Tag)"

$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
  [Environment]::SetEnvironmentVariable("Path", "$InstallDir;$UserPath", "User")
  $env:Path = "$InstallDir;$env:Path"
  Write-Host "▸ добавил $InstallDir в PATH пользователя (новые терминалы увидят triad)"
}
if (-not $env:TRIAD_NO_SETUP) {
  Write-Host "▸ настройка (ключ OpenRouter). Потом можно повторить: triad setup"
  & (Join-Path $InstallDir "$Bin.exe") setup
} else {
  Write-Host "▸ дальше: triad setup → triad"
}
