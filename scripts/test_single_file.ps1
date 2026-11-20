param([ValidateSet('local', 'github')][string]$Source = 'local')
$ErrorActionPreference = 'Stop'
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$tempRoot = Join-Path $projectRoot '.tmp'
$runRoot = Join-Path $tempRoot ('single-file-' + [Guid]::NewGuid().ToString('N'))
$version = [Regex]::Match((Get-Content -LiteralPath (Join-Path $projectRoot 'Cargo.toml') -Raw), '(?m)^version = "([^"]+)"').Groups[1].Value
$executable = Join-Path $projectRoot ("dist\PayMerge-PDF-$version-windows-x64\PayMerge-PDF.exe")
$runner = Join-Path $projectRoot 'target\x86_64-pc-windows-gnullvm\debug\examples\update_acceptance.exe'
$outLog = Join-Path $projectRoot (".cache\single-file-$Source-stdout.log")
$errLog = Join-Path $projectRoot (".cache\single-file-$Source-stderr.log")
$previousTemp = $env:TEMP
$previousTmp = $env:TMP
$previousData = $env:PAYMERGE_DATA_DIR
try {
    New-Item -ItemType Directory -Path $runRoot -Force | Out-Null
    $env:TEMP = $runRoot
    $env:TMP = $runRoot
    $env:PAYMERGE_DATA_DIR = Join-Path $runRoot 'user-data'
    $arguments = @($executable, $runRoot, $Source) | ForEach-Object { '"' + $_ + '"' }
    $process = Start-Process -FilePath $runner -ArgumentList $arguments -WorkingDirectory $projectRoot -WindowStyle Hidden -Wait -PassThru -RedirectStandardOutput $outLog -RedirectStandardError $errLog
    $output = Get-Content -LiteralPath $outLog -Encoding UTF8 -Raw
    Write-Output $output
    if ($process.ExitCode -ne 0 -or !$output.Contains('single-file updater acceptance passed')) {
        Get-Content -LiteralPath $errLog -Encoding UTF8 | Write-Output
        throw 'Single-file upgrade acceptance failed.'
    }
} finally {
    $env:TEMP = $previousTemp
    $env:TMP = $previousTmp
    $env:PAYMERGE_DATA_DIR = $previousData
    if (Test-Path -LiteralPath $runRoot) {
        $resolved = (Resolve-Path -LiteralPath $runRoot).Path
        if (!$resolved.StartsWith($tempRoot + '\single-file-', [StringComparison]::OrdinalIgnoreCase)) {
            throw 'Refusing cleanup outside this isolated test directory.'
        }
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
[PSCustomObject]@{
    source = $Source
    version = $version
    upgrade_passed = $true
    rollback_passed = $true
    renamed_executable_supported = $true
    program_directory_contains_only_one_exe = $true
    personal_files_preserved = $true
    isolated_directory_removed = !(Test-Path -LiteralPath $runRoot)
    computer_use = $false
} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $projectRoot ".cache\single-file-$Source-result.json") -Encoding UTF8
