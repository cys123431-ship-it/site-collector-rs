@echo off
chcp 65001 >nul
setlocal

echo ========================================
echo  사이트 수집기 릴리스 빌드
echo ========================================
echo.

echo [1/4] 테스트 실행...
cargo test
if errorlevel 1 (
    echo.
    echo [실패] 테스트가 실패했습니다.
    exit /b 1
)

echo.
echo [2/4] 릴리스 빌드...
cargo build --release
if errorlevel 1 (
    echo.
    echo [실패] 빌드가 실패했습니다.
    exit /b 1
)

echo.
echo [3/4] 배포 폴더 생성...
if not exist dist mkdir dist
copy /y target\release\site-collector.exe dist\site-collector.exe >nul
copy /y README.md dist\README.md >nul
copy /y LICENSE dist\LICENSE >nul
if not exist dist\config.json copy /y config.example.json dist\config.json >nul
if not exist dist\.env copy /y .env.example dist\.env >nul
if not exist dist\js_engine mkdir dist\js_engine
copy /y js_engine\README.md dist\js_engine\README.md >nul
copy /y js_engine\render.py.example dist\js_engine\render.py.example >nul
copy /y js_engine\render.js.example dist\js_engine\render.js.example >nul

echo.
echo [4/4] 결과
for %%A in (dist\site-collector.exe) do echo    exe 크기: %%~zA bytes
echo    폴더: %~dp0dist
echo.
echo 완료! dist 폴더를 배포하면 됩니다.
pause
