@echo off
chcp 65001 >nul
setlocal

echo ========================================
echo   Dub Studio
echo ========================================

set "SCRIPT_DIR=%~dp0"
cd /d "%SCRIPT_DIR%"

REM === First run? Auto-install so a single double-click does everything ===
if not exist "python\python.exe" goto :firstrun
if not exist "frontend\dist\index.html" goto :firstrun
goto :ready
:firstrun
echo First run detected - installing one-time: Python, CUDA wheels, engine, UI build...
echo.
call "%SCRIPT_DIR%install.bat"
cd /d "%SCRIPT_DIR%"
if not exist "python\python.exe" ( echo. & echo Install did not complete - see messages above. & pause & exit /b 1 )
:ready
set "PYTHON=%SCRIPT_DIR%python\python.exe"

REM === Environment isolation (everything stays in the app folder) ===
set "TEMP=%SCRIPT_DIR%temp"
set "TMP=%SCRIPT_DIR%temp"
if not exist "%TEMP%" mkdir "%TEMP%"

set "HF_HOME=%SCRIPT_DIR%models"
set "DUBENGINE_MODELS_ROOT=%SCRIPT_DIR%models"
set "HUGGINGFACE_HUB_CACHE=%SCRIPT_DIR%models"
if exist "%SCRIPT_DIR%python\Lib\site-packages\hf_transfer" set "HF_HUB_ENABLE_HF_TRANSFER=1"
set "TORCH_HOME=%SCRIPT_DIR%models\torch"
set "XDG_CACHE_HOME=%SCRIPT_DIR%cache"
if not exist "%HF_HOME%" mkdir "%HF_HOME%"
if not exist "%TORCH_HOME%" mkdir "%TORCH_HOME%"
if not exist "%XDG_CACHE_HOME%" mkdir "%XDG_CACHE_HOME%"

REM === Engine hooks (voice packs + Sortformer NeMo sub-venv, both bundled in the folder) ===
if exist "%SCRIPT_DIR%voices" set "DUBENGINE_VOICES=%SCRIPT_DIR%voices"
if exist "%SCRIPT_DIR%.venv-sortformer\Scripts\python.exe" set "DUBENGINE_SORTFORMER_PY=%SCRIPT_DIR%.venv-sortformer\Scripts\python.exe"

REM === ffmpeg/NVENC on PATH (bundled) ===
if exist "%SCRIPT_DIR%ffmpeg\ffmpeg.exe" set "PATH=%SCRIPT_DIR%ffmpeg;%PATH%"

REM === MUST be set before torch/llama_cpp import ===
set "KMP_DUPLICATE_LIB_OK=TRUE"
set "PYTHONUTF8=1"
set "PYTHONIOENCODING=utf-8"
set "PYTHONUNBUFFERED=1"
set "PYTORCH_CUDA_ALLOC_CONF=expandable_segments:True"

set "DUB_PORT=8765"

echo.
echo   UI:  http://127.0.0.1:%DUB_PORT%
echo   Close this window to stop.
echo.

REM === Open the browser once the server is up (delayed; uvicorn blocks below) ===
start "" powershell -NoProfile -Command "for($i=0;$i -lt 120;$i++){try{(New-Object Net.Sockets.TcpClient).Connect('127.0.0.1',%DUB_PORT%);Start-Process 'http://127.0.0.1:%DUB_PORT%/';break}catch{Start-Sleep 1}}"

REM === Launch the single-worker FastAPI backend (serves the SPA same-origin) ===
"%PYTHON%" -m uvicorn backend.app:app --host 127.0.0.1 --port %DUB_PORT%

if errorlevel 1 (
    echo.
    echo ERROR starting server. If the port is busy, change DUB_PORT in run.bat.
    pause
    exit /b 1
)
pause
