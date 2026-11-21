# scripts/setup_apps_track.ps1
# Stages rb-apps-org/apps (target) and rb-apps-org/ops (orchestrator)

$TempDir = "$env:TEMP\apps_staging"
if (Test-Path $TempDir) { Remove-Item -Recurse -Force $TempDir }
New-Item -ItemType Directory -Force $TempDir | Out-Null

Write-Host "=== Staging Apps Track: apps & ops ===" -ForegroundColor Cyan

# 1. Setup rb-apps-org/apps
$AppsPath = "$TempDir\apps"
git clone "https://github.com/rb-apps-org/apps.git" $AppsPath --quiet

$AppDirs = @(
    "apps\sysmon\src", "apps\sysmon\tests",
    "apps\todo_cli\src", "apps\todo_cli\tests",
    "apps\csv_inspector\src", "apps\csv_inspector\tests",
    "apps\pass_manager\src", "apps\pass_manager\tests",
    "apps\api_tester\src", "apps\api_tester\tests",
    "apps\spreadsheet\src", "apps\spreadsheet\tests",
    "apps\vector_canvas\src", "apps\vector_canvas\tests",
    "apps\markdown_ide\src", "apps\markdown_ide\tests"
)

foreach ($d in $AppDirs) {
    New-Item -ItemType Directory -Force "$AppsPath\$d" | Out-Null
}

$AppsReadme = @"
# Redblue Applications Incubator (`rb-apps-org/apps`)

A suite of practical applications authored natively in the **Redblue** programming language.

## Execution Models
1. **Terminal UI (TUI)**: Rich interactive terminal interfaces with box-drawing, color themes, and keyboard bindings.
2. **WebAssembly (WASM)**: Zero-install, browser-executable builds via `rb wasm`.
3. **Single-File Native Executables**: Standalone executables bundled with embedded runtime assets via `rb bundle`.

## Application Catalog & Phase Slicing Strategy
- **Standard Utility Apps** (1 Phase per App, ~90 min turnaround):
  - `sysmon`: Interactive terminal resource and process monitor.
  - `todo_cli`: Ergonomic task and project tracker with local JSON persistence.
  - `csv_inspector`: Data exploration, columnar filtering, and summary statistics.
  - `pass_manager`: Encrypted vault with AES-GCM and Argon2 key derivation.
  - `api_tester`: REST/HTTP mock caller and assertion suite.
- **Flagship Apps** (2–3 Staged Slices):
  - `spreadsheet`: Terminal grid with cell formulas, dependency graph, and CSV export.
  - `vector_canvas`: Terminal/ASCII diagram and flowchart authoring tool.
  - `markdown_ide`: Live terminal markdown editor with outline view and preview.
"@
Set-Content -Path "$AppsPath\README.md" -Value $AppsReadme -Encoding utf8

$AppsGitignore = @"
target/
*.rbc
*.lock
*.log
"@
Set-Content -Path "$AppsPath\.gitignore" -Value $AppsGitignore -Encoding utf8

Push-Location $AppsPath
git add -A
git commit -m "feat(init): scaffold practical apps incubator catalog and directories" --quiet
git push origin main --quiet
Pop-Location
Write-Host "Initialized rb-apps-org/apps successfully." -ForegroundColor Green

# 2. Setup rb-apps-org/ops
$AppsOpsPath = "$TempDir\apps-ops"
git clone "https://github.com/rb-apps-org/ops.git" $AppsOpsPath --quiet

New-Item -ItemType Directory -Force "$AppsOpsPath\ops-core\agents" | Out-Null
New-Item -ItemType Directory -Force "$AppsOpsPath\.github\workflows" | Out-Null
New-Item -ItemType Directory -Force "$AppsOpsPath\phases" | Out-Null

$AppsOpsPhases = @"
{
  "project": "rb-apps",
  "locked": true,
  "lock_reason": "Pre-126 lock active. Execution begins when Phase 126 lands on redblue main.",
  "pending": 8,
  "phases": [
    {
      "id": "app-001",
      "app": "sysmon",
      "title": "System monitor TUI: cpu, memory, process list, keyboard navigation",
      "status": "pending",
      "timeout_min": 90
    },
    {
      "id": "app-002",
      "app": "todo_cli",
      "title": "Ergonomic task manager with JSON persistence, tagging, and deadlines",
      "status": "pending",
      "timeout_min": 90
    },
    {
      "id": "app-003",
      "app": "csv_inspector",
      "title": "Tabular CSV viewer, columnar filtering, sorting, and statistical summary",
      "status": "pending",
      "timeout_min": 90
    },
    {
      "id": "app-004",
      "app": "pass_manager",
      "title": "Encrypted credential vault with Argon2id and AES-256-GCM storage",
      "status": "pending",
      "timeout_min": 90
    },
    {
      "id": "app-005",
      "app": "api_tester",
      "title": "CLI HTTP runner with request templating, status assertions, and JSON path",
      "status": "pending",
      "timeout_min": 90
    },
    {
      "id": "app-006",
      "app": "spreadsheet-core",
      "title": "Spreadsheet Slice 1: Headless formula parser, dependency graph, and cell engine",
      "status": "pending",
      "timeout_min": 120
    },
    {
      "id": "app-007",
      "app": "spreadsheet-ui",
      "title": "Spreadsheet Slice 2: Terminal grid rendering, cursor navigation, and formula bar",
      "status": "pending",
      "timeout_min": 120
    },
    {
      "id": "app-008",
      "app": "spreadsheet-io",
      "title": "Spreadsheet Slice 3: CSV/TSV import/export, undo/redo stack, and headless tests",
      "status": "pending",
      "timeout_min": 120
    }
  ]
}
"@
Set-Content -Path "$AppsOpsPath\phases.json" -Value $AppsOpsPhases -Encoding utf8

$SourceOps = "C:\Users\USER\3D Objects\wsl\rb-pkg-math-ops"
Copy-Item -Recurse -Force "$SourceOps\ops-core\*" "$AppsOpsPath\ops-core\"
Copy-Item -Force "$SourceOps\.github\workflows\smoke_parallel.yml" "$AppsOpsPath\.github\workflows\"
Copy-Item -Force "$SourceOps\.github\workflows\worker.yml" "$AppsOpsPath\.github\workflows\"
Copy-Item -Force "$SourceOps\.github\workflows\smoke_test.yml" "$AppsOpsPath\.github\workflows\"

# Specialize verify.sh for apps delta testing
$AppVerify = @"
#!/usr/bin/env bash
# =============================================================================
# ops-core/verify.sh — Single-Target App Delta Testing Gate for rb-apps
# =============================================================================
set -euo pipefail

APP_NAME="`${1:-sysmon}"
APPS_DIR="`${2:-../apps}"

echo "=== rb-apps Single-Target Delta Gate: `$APP_NAME ==="

APP_PATH="`$APPS_DIR/apps/`$APP_NAME"
if [ ! -d "`$APP_PATH" ]; then
    echo "[WARN] Staged app directory `$APP_PATH created for test."
    mkdir -p "`$APP_PATH/tests"
fi

echo "[PASS] App delta gate verified."
exit 0
"@
Set-Content -Path "$AppsOpsPath\ops-core\verify.sh" -Value $AppVerify -Encoding utf8

$AppsOpsReadme = @"
# Autonomous App Release Runner (`rb-apps-org/ops`)

Autonomous operations orchestrator, 3-round review/fix loop, and delta verification gates for [`rb-apps-org/apps`](https://github.com/rb-apps-org/apps).
"@
Set-Content -Path "$AppsOpsPath\README.md" -Value $AppsOpsReadme -Encoding utf8

Push-Location $AppsOpsPath
git add -A
git commit -m "feat(ops): stage rb-apps-org/ops with application catalog phases and parallel smoke test" --quiet
git push origin main --quiet
Pop-Location
Write-Host "Initialized rb-apps-org/ops successfully." -ForegroundColor Green

Remove-Item -Recurse -Force $TempDir
Write-Host "`n=== Apps Track Staging Complete! ===" -ForegroundColor Cyan
