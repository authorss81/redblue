<#
.SYNOPSIS
    Securely uploads and distributes OpenCode API keys across GitHub Organizations.
.DESCRIPTION
    Reads API keys from a local text file (e.g., keys.txt) and uploads them as organization-level
    secrets using the GitHub CLI (`gh secret set --org`). Secrets are piped via standard input to
    ensure they are never printed to the screen, never logged, and never exposed.
.PARAMETER KeyFile
    Path to the text file containing API keys (one per line). Default: keys.txt
.PARAMETER Orgs
    Array of organization names. If omitted, uses standard 10 Redblue ecosystem organizations.
#>
[CmdletBinding()]
param (
    [string]$KeyFile = "keys.txt",
    [string[]]$Orgs = @(
        "rb-pkg-sys",
        "rb-pkg-web",
        "rb-pkg-net",
        "rb-pkg-data",
        "rb-pkg-math",
        "rb-pkg-ai",
        "rb-pkg-crypto",
        "rb-pkg-media",
        "rb-pkg-cli",
        "rb-apps-org"
    )
)

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host " Redblue Ecosystem Organization Secret Provisioner" -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

# 1. Verify GitHub CLI authentication
try {
    $null = gh auth status 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Error "GitHub CLI ('gh') is not logged in. Please run 'gh auth login' first."
        exit 1
    }
} catch {
    Write-Error "GitHub CLI ('gh') is not installed or not in PATH. Please install from https://cli.github.com"
    exit 1
}

# 2. Check key file existence
if (-not (Test-Path -Path $KeyFile)) {
    Write-Error "Key file '$KeyFile' not found! Create '$KeyFile' with one API key per line."
    exit 1
}

# 3. Read keys securely (strip whitespace and empty lines)
$allLines = Get-Content -Path $KeyFile | ForEach-Object { $_.Trim() } | Where-Object { $_.Length -gt 0 -and -not $_.StartsWith("#") }
$totalKeys = $allLines.Count

if ($totalKeys -eq 0) {
    Write-Error "Key file '$KeyFile' contains 0 valid keys."
    exit 1
}

Write-Host "[OK] Loaded $totalKeys API keys from '$KeyFile'." -ForegroundColor Green
Write-Host "[OK] Target Organizations: $($Orgs.Count) organizations." -ForegroundColor Green
Write-Host "----------------------------------------------------------"

# 4. Distribute keys round-robin across organizations
$orgKeyBuckets = @{}
foreach ($org in $Orgs) {
    $orgKeyBuckets[$org] = [System.Collections.Generic.List[string]]::new()
}

$orgIndex = 0
for ($i = 0; $i -lt $totalKeys; $i++) {
    $targetOrg = $Orgs[$orgIndex % $Orgs.Count]
    $orgKeyBuckets[$targetOrg].Add($allLines[$i])
    $orgIndex++
}

# 5. Securely upload secrets per organization
$totalSuccess = 0
$totalFailed = 0

foreach ($org in $Orgs) {
    $keysForOrg = $orgKeyBuckets[$org]
    Write-Host "`n--> Provisioning Organization: '$org' ($($keysForOrg.Count) keys)" -ForegroundColor Yellow
    
    for ($k = 0; $k -lt $keysForOrg.Count; $k++) {
        $keyNum = $k + 1
        $secretName = "OPENCODE_KEY_$keyNum"
        $keyValue = $keysForOrg[$k]
        
        Write-Host "    Setting secret $secretName in $org... " -NoNewline
        
        # Pipe directly via stdin so key is NEVER visible in process lists or logs
        $processInfo = New-Object System.Diagnostics.ProcessStartInfo
        $processInfo.FileName = "gh"
        $processInfo.Arguments = "secret set $secretName --org $org --visibility all"
        $processInfo.RedirectStandardInput = $true
        $processInfo.RedirectStandardOutput = $true
        $processInfo.RedirectStandardError = $true
        $processInfo.UseShellExecute = $false
        $processInfo.CreateNoWindow = $true

        $process = [System.Diagnostics.Process]::Start($processInfo)
        $process.StandardInput.WriteLine($keyValue)
        $process.StandardInput.Close()
        $process.WaitForExit()

        if ($process.ExitCode -eq 0) {
            Write-Host "[OK]" -ForegroundColor Green
            $totalSuccess++
        } else {
            $err = $process.StandardError.ReadToEnd()
            Write-Host "[FAILED]" -ForegroundColor Red
            Write-Warning "    Error: $err"
            $totalFailed++
        }
    }
}

Write-Host "`n==========================================================" -ForegroundColor Cyan
Write-Host " Provisioning Complete!" -ForegroundColor Cyan
Write-Host " Successfully set: $totalSuccess secrets" -ForegroundColor Green
if ($totalFailed -gt 0) {
    Write-Host " Failed: $totalFailed secrets (ensure organizations exist on GitHub)" -ForegroundColor Red
}
Write-Host "==========================================================" -ForegroundColor Cyan
