@echo off
chcp 65001 >nul
setlocal enabledelayedexpansion

echo ========================================
echo   Dub Studio - Install
echo ========================================

set "SCRIPT_DIR=%~dp0"
cd /d "%SCRIPT_DIR%"
set "TEMP=%SCRIPT_DIR%temp"
set "TMP=%SCRIPT_DIR%temp"
for %%d in (downloads temp models cache voices) do if not exist "%%d" mkdir "%%d"

REM ============================================================
REM  Step 1: GPU selection (drives the torch CUDA wheel AND the matching
REM  JamePeng llama-cpp GGUF wheel; saved to cuda_version.txt for run.bat).
REM  NOTE: the engine's Gemma-GGUF wheel (JamePeng llama-cpp 0.3.40) ships for
REM  cu124/cu126/cu128/cu130/cu131 ONLY - there is no cu118 build, so GTX 10xx
REM  maps to cu126 (Pascal, experimental), not cu118.
REM ============================================================
echo.
echo Select your GPU:
echo.
echo   1. NVIDIA GTX 10xx (Pascal)        - cu126 (experimental)
echo   2. NVIDIA RTX 20xx (Turing)        - cu126
echo   3. NVIDIA RTX 30xx (Ampere)        - cu128
echo   4. NVIDIA RTX 40xx (Ada Lovelace)  - cu128
echo   5. NVIDIA RTX 50xx (Blackwell)     - cu128
echo   6. CPU only (no GPU, experimental / very slow)
echo.
set /p GPU_CHOICE="Enter number (1-6): "

if "%GPU_CHOICE%"=="1" goto :gpu_10xx
if "%GPU_CHOICE%"=="2" goto :gpu_20xx
if "%GPU_CHOICE%"=="3" goto :gpu_30xx
if "%GPU_CHOICE%"=="4" goto :gpu_40xx
if "%GPU_CHOICE%"=="5" goto :gpu_50xx
if "%GPU_CHOICE%"=="6" goto :gpu_cpu
echo Invalid choice!
pause
exit /b 1

:gpu_10xx
set "CUDA_VERSION=cu126"
set "LLAMA_CUDA=cu126"
set "CUDA_NAME=CUDA 12.6 (GTX 10xx, experimental)"
goto :gpu_done
:gpu_20xx
set "CUDA_VERSION=cu126"
set "LLAMA_CUDA=cu126"
set "CUDA_NAME=CUDA 12.6 (RTX 20xx)"
goto :gpu_done
:gpu_30xx
set "CUDA_VERSION=cu128"
set "LLAMA_CUDA=cu128"
set "CUDA_NAME=CUDA 12.8 (RTX 30xx)"
goto :gpu_done
:gpu_40xx
set "CUDA_VERSION=cu128"
set "LLAMA_CUDA=cu128"
set "CUDA_NAME=CUDA 12.8 (RTX 40xx)"
goto :gpu_done
:gpu_50xx
set "CUDA_VERSION=cu128"
set "LLAMA_CUDA=cu128"
set "CUDA_NAME=CUDA 12.8 (RTX 50xx)"
goto :gpu_done
:gpu_cpu
set "CUDA_VERSION=cpu"
set "LLAMA_CUDA=cpu"
set "CUDA_NAME=CPU only (experimental)"
goto :gpu_done

:gpu_done
echo.
echo Selected: %CUDA_NAME%
echo.

REM ============================================================
REM  2) Embeddable Python 3.11 (cp311 — required by the llama-cpp wheel)
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
REM  3) pip
REM ============================================================
if not exist "python\Scripts\pip.exe" (
    echo [2/8] Installing pip...
    powershell -Command "[Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; Invoke-WebRequest 'https://bootstrap.pypa.io/get-pip.py' -OutFile 'downloads\get-pip.py'"
    python\python.exe downloads\get-pip.py --no-warn-script-location
)
python\python.exe -m pip install --upgrade pip --no-warn-script-location

REM ============================================================
REM  4) PyTorch 2.8.0 (CUDA wheel per the GPU choice above)
REM ============================================================
echo [3/8] Installing PyTorch 2.8.0 (%CUDA_NAME%)...
if "%CUDA_VERSION%"=="cpu" (
    python\python.exe -m pip install torch==2.8.0 torchaudio==2.8.0 torchvision==0.23.0 --no-warn-script-location
) else (
    python\python.exe -m pip install torch==2.8.0 torchaudio==2.8.0 torchvision==0.23.0 --index-url https://download.pytorch.org/whl/%CUDA_VERSION% --no-warn-script-location
)

REM ============================================================
REM  5) Engine main-env deps (proven pins)
REM ============================================================
echo [4/8] Installing engine deps...
python\python.exe -m pip install -r requirements-engine.txt --no-warn-script-location
REM backend (thin) deps
python\python.exe -m pip install -r requirements.txt --no-warn-script-location

REM ============================================================
REM  6) llama-cpp-python (JamePeng GGUF wheel, matching CUDA) + Triton (+ headers)
REM ============================================================
echo [5/8] Installing llama-cpp-python (Gemma GGUF, %LLAMA_CUDA%)...
if "%CUDA_VERSION%"=="cpu" goto :llama_cpu
python\python.exe -m pip install "https://github.com/JamePeng/llama-cpp-python/releases/download/v0.3.40-%LLAMA_CUDA%-win-20260608/llama_cpp_python-0.3.40+%LLAMA_CUDA%-cp311-cp311-win_amd64.whl" --no-warn-script-location
goto :after_llama
:llama_cpu
python\python.exe -m pip install llama-cpp-python --only-binary=:all: --extra-index-url https://abetlen.github.io/llama-cpp-python/whl/cpu --no-warn-script-location
:after_llama

REM Triton (Qwen3-TTS kernels) — NVIDIA only (Pascal+); needs Python headers from dev.msi
if "%CUDA_VERSION%"=="cpu" goto :after_triton
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
:after_triton

REM ============================================================
REM  7) dub-engine (bundled in the archive) — editable install
REM ============================================================
echo [6/8] Installing dub-engine...
if exist "dub-engine\pyproject.toml" (
    python\python.exe -m pip install -e dub-engine --no-deps --no-warn-script-location
) else (
    echo   WARNING: dub-engine\ not found next to install.bat. Place the engine there, then re-run.
)

REM ============================================================
REM  8) Sortformer diarization sub-venv (NeMo) — OPTIONAL (multi-speaker, NVIDIA only)
REM     The core pipeline runs WITHOUT it (falls back to single-speaker).
REM     NeMo on Windows is finicky; if this step fails, Dub Studio still works.
REM ============================================================
echo [7/8] Sortformer sub-venv (optional, multi-speaker diarization)...
if "%CUDA_VERSION%"=="cpu" goto :sf_done
set "SF_PY=%SCRIPT_DIR%.venv-sortformer\Scripts\python.exe"
if exist "%SF_PY%" goto :sf_done
python\python.exe -m pip install virtualenv --no-warn-script-location
python\python.exe -m virtualenv ".venv-sortformer"
if not exist "%SF_PY%" goto :sf_skip
"%SF_PY%" -m pip install torch==2.8.0 torchaudio==2.8.0 --index-url https://download.pytorch.org/whl/%CUDA_VERSION%
"%SF_PY%" -m pip install "nemo_toolkit[asr]" "cuda-python>=12.3"
echo [OK] Sortformer sub-venv ready
goto :sf_done
:sf_skip
echo   SKIP: sub-venv not created; multi-speaker diarization off ^(single-speaker fallback^).
:sf_done

REM ============================================================
REM  9) ffmpeg (NVENC) + Node + build the SPA
REM ============================================================
echo [8/8] FFmpeg + frontend build...
if not exist "ffmpeg\ffmpeg.exe" (
    powershell -Command "[Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; Invoke-WebRequest 'https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip' -OutFile 'downloads\ffmpeg.zip'"
    if not exist "ffmpeg" mkdir ffmpeg
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

REM ============================================================
REM  Base voice pack — preset reference voices, pulled as a zip from HF
REM ============================================================
echo [+] Downloading the base voice pack...
if exist "voices\*.mp3" (
    echo   [OK] voices already present
) else (
    curl -L -o downloads\voice-pack.zip https://huggingface.co/datasets/nerualdreming/VibeVoice/resolve/main/voice-pack.zip
    if exist "downloads\voice-pack.zip" (
        powershell -Command "Expand-Archive -Path 'downloads\voice-pack.zip' -DestinationPath 'downloads\vp' -Force"
        if exist "downloads\vp\voice-pack" (
            xcopy /E /Y /Q "downloads\vp\voice-pack\*" "voices\" >nul
        ) else (
            xcopy /E /Y /Q "downloads\vp\*" "voices\" >nul
        )
        rmdir /s /q "downloads\vp"
        echo   [OK] voice pack installed
    )
)

REM Save the chosen CUDA build so run.bat (and a re-install) can show/reuse it
echo %CUDA_VERSION%> cuda_version.txt

echo.
echo ========================================
echo   Done (%CUDA_NAME%). Start with run.bat
echo   Models download on first run (Gemma GGUF + mmproj, Parakeet, Qwen3-TTS, Sortformer).
echo ========================================
pause
