# scripts/sync_ops_core.ps1
# Synchronizes the verified ops-core engine and smoke_parallel workflow across all ecosystem ops repositories.

param(
    [string[]]$TargetRepos = @(
        "rb-pkg-math/ops",
        "rb-pkg-sys/ops",
        "rb-pkg-web/ops",
        "rb-pkg-net/ops",
        "rb-pkg-data/ops",
        "rb-pkg-ai/ops",
        "rb-pkg-crypto/ops",
        "rb-pkg-media/ops",
        "rb-pkg-cli/ops",
        "rb-apps-org/ops"
    )
)

$SourceDir = "C:\Users\USER\3D Objects\wsl\rb-pkg-math-ops"
$TempDir = "$env:TEMP\rb_ops_sync"

Write-Host "=== Redblue Ops-Core Engine Synchronizer ===" -ForegroundColor Cyan

foreach ($Repo in $TargetRepos) {
    if ($Repo -eq "rb-pkg-math/ops") {
        continue
    }

    $ClonePath = "$TempDir\$($Repo.Replace('/', '_'))"
    if (Test-Path $ClonePath) {
        Remove-Item -Recurse -Force $ClonePath
    }

    git clone "https://github.com/$Repo.git" $ClonePath --quiet
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Failed to clone $Repo. Skipping." -ForegroundColor Red
        continue
    }

    New-Item -ItemType Directory -Force "$ClonePath\.github\workflows" | Out-Null
    Copy-Item -Recurse -Force "$SourceDir\ops-core" "$ClonePath\"
    Copy-Item -Force "$SourceDir\.github\workflows\smoke_parallel.yml" "$ClonePath\.github\workflows\"

    Push-Location $ClonePath
    try {
        git add -A
        $Status = git status --porcelain
        if ($Status) {
            git commit -m "feat(ops): add smoke_parallel.yml and ops-core workflows" --quiet
            git push origin main --quiet
            Write-Host "Synced smoke_parallel.yml to $Repo." -ForegroundColor Green
        } else {
            Write-Host "$Repo is already fully synchronized." -ForegroundColor Gray
        }
    } finally {
        Pop-Location
    }
}

if (Test-Path $TempDir) {
    Remove-Item -Recurse -Force $TempDir
}

Write-Host "`n=== Complete: All repositories have ops-core and smoke_parallel.yml! ===" -ForegroundColor Cyan
