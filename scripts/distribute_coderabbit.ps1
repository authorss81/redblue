# scripts/distribute_coderabbit.ps1
# Distributes .coderabbit.yaml across all ecosystem repositories.

$TargetRepos = @(
    "authorss81/rb-pkg",
    "rb-pkg-math/packages", "rb-pkg-math/ops",
    "authorss81/rbos", "authorss81/rbos-ops",
    "rb-apps-org/apps", "rb-apps-org/ops",
    "authorss81/rb-ide", "authorss81/rb-ide-ops",
    "rb-pkg-sys/packages", "rb-pkg-sys/ops",
    "rb-pkg-web/packages", "rb-pkg-web/ops",
    "rb-pkg-net/packages", "rb-pkg-net/ops",
    "rb-pkg-data/packages", "rb-pkg-data/ops",
    "rb-pkg-ai/packages", "rb-pkg-ai/ops",
    "rb-pkg-crypto/packages", "rb-pkg-crypto/ops",
    "rb-pkg-media/packages", "rb-pkg-media/ops",
    "rb-pkg-cli/packages", "rb-pkg-cli/ops"
)

$SourceFile = "C:\Users\USER\3D Objects\wsl\redblue\.coderabbit.yaml"
$TempDir = "$env:TEMP\cr_dist"
if (Test-Path $TempDir) { Remove-Item -Recurse -Force $TempDir }
New-Item -ItemType Directory -Force $TempDir | Out-Null

Write-Host "=== Distributing .coderabbit.yaml across Ecosystem ===" -ForegroundColor Cyan

foreach ($Repo in $TargetRepos) {
    $ClonePath = "$TempDir\$($Repo.Replace('/', '_'))"
    git clone --quiet "https://github.com/$Repo.git" $ClonePath
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Could not clone $Repo, skipping." -ForegroundColor Yellow
        continue
    }

    Copy-Item -Force $SourceFile "$ClonePath\.coderabbit.yaml"
    Push-Location $ClonePath
    try {
        git add .coderabbit.yaml
        $Status = git status --porcelain
        if ($Status) {
            git commit -m "ci: add CodeRabbit AI review configuration (.coderabbit.yaml)" --quiet
            git push origin main --quiet
            Write-Host "Added .coderabbit.yaml to $Repo" -ForegroundColor Green
        } else {
            Write-Host "$Repo already has .coderabbit.yaml" -ForegroundColor Gray
        }
    } finally {
        Pop-Location
    }
}

Remove-Item -Recurse -Force $TempDir
Write-Host "`n=== CodeRabbit distribution complete! ===" -ForegroundColor Cyan
