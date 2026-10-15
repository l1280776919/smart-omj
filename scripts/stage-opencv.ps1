param([string]$LlvmDirectory = 'C:\Program Files\LLVM\bin')
$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$root = Split-Path -Parent $PSScriptRoot
$cache = Join-Path $root 'src-tauri/target/native-deps'
$runtime = Join-Path $root 'src-tauri/native-runtime'
$licenses = Join-Path $root 'src-tauri/resources/opencv'
foreach ($path in @($cache, $runtime, $licenses)) { New-Item -ItemType Directory -Force -Path $path | Out-Null }
if (!(Test-Path (Join-Path $LlvmDirectory 'libclang.dll'))) { throw 'Install LLVM with libclang before building OpenCV bindings.' }
$url = 'https://github.com/opencv/opencv/releases/download/4.12.0/opencv-4.12.0-windows.exe'
$archive = Join-Path $cache 'opencv-4.12.0-windows.exe'
$expected = 'b753b14d880b9bc8d89d6acd3b665c040baec0211078435432fcae117db707af'
function HashFile([string]$path) {
    $stream = [IO.File]::OpenRead($path)
    $sha = [Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($sha.ComputeHash($stream))).Replace('-', '').ToLowerInvariant() }
    finally { $stream.Dispose(); $sha.Dispose() }
}
if (!(Test-Path $archive)) { Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $archive }
if ((HashFile $archive) -ne $expected) { throw 'OpenCV archive SHA256 mismatch. Remove the invalid archive and retry.' }
$sdk = Join-Path $cache 'opencv'
if (!(Test-Path (Join-Path $sdk 'build/x64/vc16/lib/opencv_world4120.lib'))) {
    $process = Start-Process -FilePath $archive -ArgumentList @('-y', ('-o"' + $cache + '"')) -WindowStyle Hidden -PassThru -Wait
    if ($process.ExitCode -ne 0) { throw 'OpenCV extraction failed.' }
}
Copy-Item -LiteralPath (Join-Path $sdk 'build/x64/vc16/bin/opencv_world4120.dll') -Destination $runtime -Force
Copy-Item -LiteralPath (Join-Path $sdk 'build/LICENSE') -Destination (Join-Path $licenses 'OpenCV-LICENSE.txt') -Force
Copy-Item -LiteralPath (Join-Path $sdk 'sources/COPYRIGHT') -Destination (Join-Path $licenses 'OpenCV-COPYRIGHT.txt') -Force
# Retain source third-party notices for the official world binary, even for modules not called by this app.
$thirdparty = Join-Path $sdk 'sources/3rdparty'
Get-ChildItem -LiteralPath $thirdparty -Recurse -File | Where-Object { $_.Name -match '^(LICENSE|COPYING|COPYRIGHT|NOTICE)' } | ForEach-Object {
    $relative = $_.FullName.Substring($thirdparty.Length).TrimStart('\', '/')
    $destination = Join-Path (Join-Path $licenses 'third-party') $relative
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
    Copy-Item -LiteralPath $_.FullName -Destination $destination -Force
}
# The official world binary includes IPP; its package is pinned by OpenCV's 4.12 source.
$ippUrl = 'https://raw.githubusercontent.com/opencv/opencv_3rdparty/767426b2a40a011eb2fa7f44c677c13e60e205ad/ippicv/ippicv_2022.1.0_win_intel64_20250130_general.zip'
$ippArchive = Join-Path $cache 'ippicv.zip'
$ippHash = 'f87c59573cece75260de2bfe1ecfa305c931a420feb57e03310ae06bb0e8f3c7'
if (!(Test-Path $ippArchive)) { Invoke-WebRequest -UseBasicParsing -Uri $ippUrl -OutFile $ippArchive }
if ((HashFile $ippArchive) -ne $ippHash) { throw 'IPP archive SHA256 mismatch.' }
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::OpenRead($ippArchive)
try {
    foreach ($entry in $zip.Entries) {
        if ($entry.Name -match '(?i)(license|copying|copyright|notice|eula|third-party-programs)') {
            $destination = Join-Path $licenses ('ipp/' + $entry.FullName)
            New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
            [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $destination, $true)
        }
    }
} finally { $zip.Dispose() }
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
$vs = & $vswhere -latest -products '*' -property installationPath
if (!$vs) { throw 'Visual Studio Build Tools with the C++ redistributable is required.' }
$redist = Get-ChildItem -Path (Join-Path $vs 'VC/Redist/MSVC/*/x64/Microsoft.VC*.CRT') -Directory | Sort-Object FullName -Descending | Select-Object -First 1
if (!$redist) { throw 'Cannot find redistributable x64 VC runtime.' }
foreach ($name in @('msvcp140.dll', 'concrt140.dll', 'vcruntime140.dll', 'vcruntime140_1.dll')) {
    Copy-Item -LiteralPath (Join-Path $redist.FullName $name) -Destination $runtime -Force
}
$vsLicenses = Join-Path $vs 'Licenses'
Get-ChildItem -LiteralPath $vsLicenses -Recurse -File | Where-Object { $_.Name -in @('Redist.txt', 'ThirdPartyNotices.txt') } | ForEach-Object {
    $relative = $_.FullName.Substring($vsLicenses.Length).TrimStart('\', '/')
    $destination = Join-Path (Join-Path $licenses 'vc-runtime') $relative
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
    Copy-Item -LiteralPath $_.FullName -Destination $destination -Force
}
$files = Get-ChildItem -LiteralPath $runtime -Filter '*.dll' | ForEach-Object { @{ name = $_.Name; sha256 = HashFile $_.FullName; version = $_.VersionInfo.FileVersion } }
@{ version = '4.12.0'; archive_url = $url; archive_sha256 = $expected; ipp_notice_source = $ippUrl; ipp_sha256 = $ippHash; vc_runtime_source = $redist.FullName; files = @($files) } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $licenses 'manifest.json') -Encoding UTF8
Write-Output 'OpenCV 4.12.0 SDK, native runtime, notices and hash manifest prepared.'
