@echo off
chcp 65001 >nul
setlocal

set "SCRIPT_DIR=%~dp0"
cd /d "%SCRIPT_DIR%"

echo ========================================
echo   Dub Studio - Update
echo ========================================

where git >nul 2>&1
if errorlevel 1 goto :nogit
if not exist "python\python.exe" goto :noinstall

REM update the studio wrapper
if exist ".git" (
    echo Updating Dub Studio...
    git pull
)

REM update + reinstall the engine (bundled next to the app)
if exist "dub-engine\.git" (
    echo Updating dub-engine...
    cd dub-engine
    git pull
    cd /d "%SCRIPT_DIR%"
)
if exist "dub-engine\pyproject.toml" python\python.exe -m pip install -e dub-engine --no-deps --no-warn-script-location

REM rebuild the SPA (UI source may have changed)
if exist "node\node.exe" (
    set "PATH=%SCRIPT_DIR%node;%PATH%"
    cd frontend
    call "%SCRIPT_DIR%node\npm.cmd" install
    call "%SCRIPT_DIR%node\npm.cmd" run build
    cd /d "%SCRIPT_DIR%"
)

echo.
echo Update complete. Start with run.bat
pause
exit /b 0

:nogit
echo ERROR: Git not found. Install Git, or just re-download the latest portable zip.
pause
exit /b 1
:noinstall
echo ERROR: not installed yet. Run install.bat first.
pause
exit /b 1
