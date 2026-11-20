param([string]$Fixtures = '')
$ErrorActionPreference = 'Stop'
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$tempRoot = [IO.Path]::GetFullPath((Join-Path $projectRoot '.tmp'))
$runRoot = Join-Path $tempRoot ('windows-tests-' + [Guid]::NewGuid().ToString('N'))
if ($Fixtures) {
    $Fixtures = (Resolve-Path -LiteralPath $Fixtures).Path
    if (!$Fixtures.StartsWith($tempRoot + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Fixtures must be an isolated copy inside the project .tmp directory.'
    }
}
$previousTemp = $env:TEMP
$previousTmp = $env:TMP
$previousData = $env:PAYMERGE_DATA_DIR
$previousFixtures = $env:PAYMERGE_TEST_FIXTURES
try {
    New-Item -ItemType Directory -Path $runRoot -Force | Out-Null
    $env:TEMP = $runRoot
    $env:TMP = $runRoot
    $env:PAYMERGE_DATA_DIR = Join-Path $runRoot 'user-data'
    $env:PAYMERGE_TEST_FIXTURES = $Fixtures
    $artifacts = Get-Content -LiteralPath (Join-Path $projectRoot '.cache\windows-tests-artifacts.jsonl') -Encoding UTF8 |
        ForEach-Object { $_ | ConvertFrom-Json } |
        Where-Object { $_.reason -eq 'compiler-artifact' -and $_.profile.test -and $_.executable }
    if (!$artifacts) { throw 'Build Windows tests on the server first.' }
    $summaries = @()
    foreach ($artifact in $artifacts) {
        $serverPrefix = $artifact.manifest_path.Substring(0, $artifact.manifest_path.LastIndexOf('/') + 1)
        if (!$artifact.executable.StartsWith($serverPrefix)) { throw 'Unexpected artifact path.' }
        $executable = Join-Path $projectRoot $artifact.executable.Substring($serverPrefix.Length)
        $outLog = Join-Path $runRoot ($artifact.target.name + '-stdout.txt')
        $errLog = Join-Path $runRoot ($artifact.target.name + '-stderr.txt')
        $arguments = @('--test-threads=1')
        if ($Fixtures) { $arguments += '--include-ignored' }
        $testProcess = Start-Process -FilePath $executable -ArgumentList $arguments -WorkingDirectory $projectRoot -WindowStyle Hidden -Wait -PassThru -RedirectStandardOutput $outLog -RedirectStandardError $errLog
        $output = Get-Content -LiteralPath $outLog -Raw -Encoding UTF8
        Write-Output $output
        if ($testProcess.ExitCode -ne 0) {
            Get-Content -LiteralPath $errLog -Encoding UTF8 | Write-Output
            throw ('Windows tests failed: ' + $artifact.target.name)
        }
        $matched = [Regex]::Match($output, 'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored')
        if (!$matched.Success) { throw 'Missing native test summary.' }
        $summaries += [PSCustomObject]@{ target=$artifact.target.name; passed=[int]$matched.Groups[1].Value; failed=[int]$matched.Groups[2].Value; ignored=[int]$matched.Groups[3].Value }
    }
    [PSCustomObject]@{ tests=$summaries; fixtures_supplied=[bool]$Fixtures; method='native test executables; no computer use or OS input injection' } |
        ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $projectRoot 'docs\windows-native-tests.json') -Encoding UTF8
} finally {
    $env:TEMP = $previousTemp
    $env:TMP = $previousTmp
    $env:PAYMERGE_DATA_DIR = $previousData
    $env:PAYMERGE_TEST_FIXTURES = $previousFixtures
    if (Test-Path -LiteralPath $runRoot) {
        $resolvedRun = (Resolve-Path -LiteralPath $runRoot).Path
        if (!$resolvedRun.StartsWith($tempRoot + '\windows-tests-', [StringComparison]::OrdinalIgnoreCase)) {
            throw 'Refusing cleanup outside the isolated test folder.'
        }
        Remove-Item -LiteralPath $resolvedRun -Recurse -Force
    }
}
