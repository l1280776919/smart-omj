param([string]$PlatformToolsDirectory)
$ErrorActionPreference = 'Stop'
if (-not $PlatformToolsDirectory) {
    $adbCommand = Get-Command adb.exe -ErrorAction Stop
    $PlatformToolsDirectory = Split-Path -Parent $adbCommand.Source
}
$sourceDirectory = (Resolve-Path -LiteralPath $PlatformToolsDirectory).Path
$repositoryDirectory = Split-Path -Parent $PSScriptRoot
$resourceDirectory = Join-Path $repositoryDirectory 'src-tauri\resources\adb'
$requiredFiles = @('adb.exe', 'AdbWinApi.dll', 'AdbWinUsbApi.dll', 'NOTICE.txt', 'source.properties')
foreach ($fileName in $requiredFiles) {
    if (-not (Test-Path -LiteralPath (Join-Path $sourceDirectory $fileName) -PathType Leaf)) {
        throw "Android platform-tools is missing $fileName. Packaging stopped."
    }
}
New-Item -ItemType Directory -Force -Path $resourceDirectory | Out-Null
$sourceVersion = Get-Content -Raw -LiteralPath (Join-Path $sourceDirectory 'source.properties')
$manifest = @{ source = 'Android SDK platform-tools (local developer SDK)'; version = $sourceVersion.Trim(); files = @() }
foreach ($fileName in $requiredFiles) {
    $sourceFile = Join-Path $sourceDirectory $fileName
    $destinationFile = Join-Path $resourceDirectory $fileName
    Copy-Item -LiteralPath $sourceFile -Destination $destinationFile -Force
    $hasher = [System.Security.Cryptography.SHA256]::Create()
    $fileStream = [System.IO.File]::OpenRead($destinationFile)
    try { $hashValue = [System.BitConverter]::ToString($hasher.ComputeHash($fileStream)).Replace('-', '').ToLowerInvariant() }
    finally { $fileStream.Dispose(); $hasher.Dispose() }
    $manifest.files += @{ file = $fileName; sha256 = $hashValue }
}
$manifestJson = $manifest | ConvertTo-Json -Depth 5
[System.IO.File]::WriteAllText((Join-Path $resourceDirectory 'manifest.json'), $manifestJson, (New-Object System.Text.UTF8Encoding($false)))
Write-Output "Staged ADB resources with NOTICE, version and SHA-256. Clean Windows validation is still required."
