# Create all paired ecosystem repositories
$pkgOrgs = @(
    "rb-pkg-web",
    "rb-pkg-net",
    "rb-pkg-data",
    "rb-pkg-ai",
    "rb-pkg-crypto",
    "rb-pkg-media",
    "rb-pkg-cli"
)

foreach ($org in $pkgOrgs) {
    Write-Host "Creating paired repos in '$org'..." -ForegroundColor Cyan
    gh repo create "$org/packages" --public --description "Redblue Official Packages Registry ($org)"
    gh repo create "$org/ops" --public --description "Autonomous Ops Orchestrator for $org"
}

Write-Host "Creating paired repos in 'rb-apps-org'..." -ForegroundColor Cyan
gh repo create "rb-apps-org/apps" --public --description "Redblue Userland Applications Catalog"
gh repo create "rb-apps-org/ops" --public --description "Autonomous Ops Orchestrator for rb-apps-org"

Write-Host "Creating paired OS repos in 'authorss81'..." -ForegroundColor Cyan
gh repo create "authorss81/rbos" --public --description "Redblue Sovereign Operating System Kernel & Desktop Workspace"
gh repo create "authorss81/rbos-ops" --public --description "Autonomous Pipeline & QEMU Runner for rbos"

Write-Host "All ecosystem repositories successfully created!" -ForegroundColor Green
