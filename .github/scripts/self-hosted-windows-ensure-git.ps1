$ErrorActionPreference = 'Stop'

$candidates = @(
    'C:\Program Files\Git\cmd',
    'C:\Program Files\Git\bin',
    'C:\Program Files (x86)\Git\cmd'
)

foreach ($dir in $candidates) {
    $git = Join-Path $dir 'git.exe'
    if (Test-Path $git) {
        Write-Host "Using Git: $git"
        & $git --version
        Add-Content -Path $env:GITHUB_PATH -Value $dir
        exit 0
    }
}

Write-Error @"
Git 2.18+ not found on PATH.

Install Git for Windows on the runner host, then restart the actions-runner service:
  https://git-scm.com/download/win

Expected locations:
  C:\Program Files\Git\cmd\git.exe
"@
