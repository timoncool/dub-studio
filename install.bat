@echo off
chcp 65001 >nul
setlocal enabledelayedexpansion

echo ========================================
echo   Dub Studio - Install  (CUDA 12.8 build)
echo ========================================
echo   Target GPU: NVIDIA RTX 20xx / 30xx / 40xx / 50xx (CUDA 12.8).
echo   The engine stack (llama-cpp / triton / torch) is pinned to cu128 + Python 3.11.
echo.

set "SCRIPT_DIR=%~dp0"
cd /d "%SCRIPT_DIR%"
set "TEMP=%SCRIPT_DIR%temp"
set "TMP=%SCRIPT_DIR%temp"
for %%d in (downloads temp models cache voices) do if not exist "%%d" mkdir "%%d"

REM ============================================================
REM  1) Embeddable Python 3.11 (cp311 — required by the llama-cpp wheel)
REM ============================================================
if exist "python\python.exe" (
    echo [OK] Python already present
) else (
    echo [1/8] Downloading Python 3.11.9 embeddable...
    if not exist "python" mkdir python
    powershell -Command "[Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; Invoke-WebRequest 'https://www.python.org/ftp/python/3.11.9/python-3.11.9-embed-amd64.zip' -OutFile 'downloads\python.zip'"
    if not exist "downloads\python.zip" ( echo ERROR: Python download failed & pause & exit /b 1 )
    powershell -Command "Expand-Archive -Path 'downloads\python.zip' -DestinationPath 'python' -Force"
    cd python
    if exist "python311._pth" (
        echo python311.zip> python311._pth
        echo .>> python311._pth
        echo Lib\site-packages>> python311._pth
        echo ..\Lib\site-packages>> python311._pth
        echo import site>> python311._pth
    )
    cd ..
    echo [OK] Python 3.11.9 installed
)

REM ============================================================
REM  2) pip
REM ============================================================
if not exist "python\Scripts\pip.exe" (
    echo [2/8] Installing pip...
    powershell -Command "[Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; Invoke-WebRequest 'https://bootstrap.pypa.io/get-pip.py' -OutFile 'downloads\get-pip.py'"
    python\python.exe downloads\get-pip.py --no-warn-script-location
)
python\python.exe -m pip install --upgrade pip --no-warn-script-location

REM ============================================================
REM  3) PyTorch 2.8.0 + cu128 (proven)
REM ============================================================
echo [3/8] Installing PyTorch 2.8.0 (cu128)...
python\python.exe -m pip install torch==2.8.0 torchaudio==2.8.0 torchvision==0.23.0 --index-url https://download.pytorch.org/whl/cu128 --no-warn-script-location

REM ============================================================
REM  4) Engine main-env deps (proven pins)
REM ============================================================
echo [4/8] Installing engine deps...
python\python.exe -m pip install -r requirements-engine.txt --no-warn-script-location
REM backend (thin) deps
python\python.exe -m pip install -r requirements.txt --no-warn-script-location

REM ============================================================
REM  5) llama-cpp-python (JamePeng cu128 wheel) + Triton (+ Python headers)
REM ============================================================
echo [5/8] Installing llama-cpp-python (Gemma GGUF, cu128)...
python\python.exe -m pip install "https://github.com/JamePeng/llama-cpp-python/releases/download/v0.3.40-cu128-win-20260608/llama_cpp_python-0.3.40+cu128-cp311-cp311-win_amd64.whl" --no-warn-script-location
echo   Installing triton-windows (Qwen3-TTS kernels)...
python\python.exe -m pip install "triton-windows==3.7.0.post26" --no-warn-script-location
if not exist "python\Include\Python.h" (
    echo   Fetching Python headers for Triton...
    powershell -Command "[Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; Invoke-WebRequest 'https://www.python.org/ftp/python/3.11.9/amd64/dev.msi' -OutFile 'downloads\pydev.msi'"
    if exist "downloads\pydev.msi" (
        msiexec /a "downloads\pydev.msi" /qn TARGETDIR="%SCRIPT_DIR%downloads\pydev_extract"
        if not exist "python\Include" mkdir "python\Include"
        if not exist "python\libs" mkdir "python\libs"
        xcopy /E /Y "downloads\pydev_extract\include\*" "python\Include\" >nul 2>&1
        xcopy /E /Y "downloads\pydev_extract\libs\*" "python\libs\" >nul 2>&1
        rmdir /s /q "downloads\pydev_extract"
    )
)

REM ============================================================
REM  6) dub-engine (bundled in the archive) — editable install
REM ============================================================
echo [6/8] Installing dub-engine...
if exist "dub-engine\pyproject.toml" (
    python\python.exe -m pip install -e dub-engine --no-deps --no-warn-script-location
) else (
    echo   WARNING: dub-engine\ not found next to install.bat. Place the engine there, then re-run.
)

REM ============================================================
REM  7) Sortformer diarization sub-venv (NeMo) — OPTIONAL (multi-speaker)
REM     The core pipeline runs WITHOUT it (falls back to single-speaker).
REM     NeMo on Windows is finicky; if this step fails, Dub Studio still works.
REM ============================================================
echo [7/8] Sortformer sub-venv (optional, multi-speaker diarization)...
if not exist ".venv-sortformer\Scripts\python.exe" (
    python\python.exe -m pip install virtualenv --no-warn-script-location
    python\python.exe -m virtualenv ".venv-sortformer" 2>nul
    if exist ".venv-sortformer\Scripts\python.exe" (
        .venv-sortformer\Scripts\python.exe -m pip install torch==2.8.0 torchaudio==2.8.0 --index-url https://download.pytorch.org/whl/cu128
        .venv-sortformer\Scripts\python.exe -m pip install "nemo_toolkit[asr]" cuda-python>=12.3
        echo [OK] Sortformer sub-venv ready
    ) else (
        echo   SKIP: could not create sub-venv; multi-speaker diarization disabled (single-speaker fallback).
    )
) else ( echo [OK] Sortformer sub-venv present )

REM ============================================================
REM  8) ffmpeg (NVENC) + Node + build the SPA
REM ============================================================
echo [8/8] FFmpeg + frontend build...
if not exist "ffmpeg\ffmpeg.exe" (
    powershell -Command "[Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; Invoke-WebRequest 'https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip' -OutFile 'downloads\ffmpeg.zip'"
    if exist "downloads\ffmpeg.zip" (
        powershell -Command "Expand-Archive -Path 'downloads\ffmpeg.zip' -DestinationPath 'downloads\ff' -Force"
        powershell -Command "Get-ChildItem 'downloads\ff\ffmpeg-*\bin\ffmpeg.exe' | Copy-Item -Destination 'ffmpeg\ffmpeg.exe' -Force"
        powershell -Command "Get-ChildItem 'downloads\ff\ffmpeg-*\bin\ffprobe.exe' | Copy-Item -Destination 'ffmpeg\ffprobe.exe' -Force"
        rmdir /s /q "downloads\ff"
    )
)
if not exist "node\node.exe" (
    powershell -Command "[Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; Invoke-WebRequest 'https://nodejs.org/dist/v22.18.0/node-v22.18.0-win-x64.zip' -OutFile 'downloads\node.zip'"
    powershell -Command "Expand-Archive -Path 'downloads\node.zip' -DestinationPath 'downloads\node-x' -Force"
    if not exist "node" mkdir node
    powershell -Command "Get-ChildItem 'downloads\node-x\node-*\*' | Move-Item -Destination 'node' -Force"
    rmdir /s /q "downloads\node-x"
)
set "PATH=%SCRIPT_DIR%node;%PATH%"
cd frontend
call "%SCRIPT_DIR%node\npm.cmd" install
call "%SCRIPT_DIR%node\npm.cmd" run build
cd "%SCRIPT_DIR%"

echo.
echo ========================================
echo   Done. Start with run.bat
echo   Models download on first run (Gemma GGUF + mmproj, Parakeet, Qwen3-TTS, Sortformer).
echo ========================================
pause
