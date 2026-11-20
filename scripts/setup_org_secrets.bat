@echo off
setlocal
set "SCRIPT_DIR=%~dp0"
set "KEY_FILE=%~1"
set "TARGET_ORG=%~2"

if "%KEY_FILE%"=="" set "KEY_FILE=keys.txt"
if "%TARGET_ORG%"=="" set "TARGET_ORG=rb-pkg-math"

powershell -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%setup_org_secrets.ps1" -KeyFile "%KEY_FILE%" -Orgs @("%TARGET_ORG%")
