@echo off
chcp 65001 >nul
setlocal

set EXE=%~dp0target\release\site-collector.exe
if not exist "%EXE%" (
    echo [오류] release 빌드가 없습니다.
    echo   cargo build --release
    pause
    exit /b 1
)

if "%~1"=="" (
    start "" "%EXE%"
) else (
    "%EXE%" %*
    pause
)
