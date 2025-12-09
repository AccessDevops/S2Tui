@echo off
setlocal enabledelayedexpansion

echo.
echo ===============================================================
echo   S2Tui Windows Build Script
echo   Speech-to-Text with Vulkan GPU Acceleration
echo ===============================================================
echo.

:: Configuration
set "PROJECT_ROOT=%~dp0.."
set "SRC_TAURI=%PROJECT_ROOT%\src-tauri"
set "MODELS_DIR=%SRC_TAURI%\models"
set "RELEASE_DIR=%SRC_TAURI%\target\release"
set "PORTABLE_DIR=%RELEASE_DIR%\portable"

:: Parse arguments
set SKIP_BUILD=0
if "%1"=="--skip-build" set SKIP_BUILD=1
if "%1"=="-s" set SKIP_BUILD=1

:: ============================================================================
:: Prerequisites Check
:: ============================================================================

echo [1/5] Checking prerequisites...

:: Check Node.js
where node >nul 2>&1
if %ERRORLEVEL% neq 0 (
    echo ERROR: Node.js is not installed or not in PATH
    exit /b 1
)
for /f "tokens=*" %%i in ('node --version') do echo   - Node.js %%i

:: Check Rust
where rustc >nul 2>&1
if %ERRORLEVEL% neq 0 (
    echo ERROR: Rust is not installed or not in PATH
    exit /b 1
)
for /f "tokens=*" %%i in ('rustc --version') do echo   - %%i

:: Check Ninja
where ninja >nul 2>&1
if %ERRORLEVEL% neq 0 (
    echo ERROR: Ninja is not installed or not in PATH
    exit /b 1
)
for /f "tokens=*" %%i in ('ninja --version') do echo   - Ninja %%i

:: Check models
echo   - Checking Whisper models...
if not exist "%MODELS_DIR%\ggml-small.bin" (
    echo ERROR: Missing model: ggml-small.bin
    echo   Download with: curl -L -o src-tauri/models/ggml-small.bin https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin
    exit /b 1
)
echo     - ggml-small.bin [OK]

if not exist "%MODELS_DIR%\ggml-large-v3-turbo.bin" (
    echo ERROR: Missing model: ggml-large-v3-turbo.bin
    echo   Download with: curl -L -o src-tauri/models/ggml-large-v3-turbo.bin https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin
    exit /b 1
)
echo     - ggml-large-v3-turbo.bin [OK]

echo   All prerequisites satisfied
echo.

:: ============================================================================
:: Build
:: ============================================================================

if %SKIP_BUILD%==1 (
    echo [2/5] Skipping build ^(using existing artifacts^)...
    if not exist "%RELEASE_DIR%\S2Tui.exe" (
        echo ERROR: No existing build found. Run without --skip-build first.
        exit /b 1
    )
) else (
    echo [2/5] Building Tauri application...
    echo   Setting CMAKE_GENERATOR=Ninja
    set CMAKE_GENERATOR=Ninja

    pushd "%PROJECT_ROOT%"
    call npx tauri build --features gpu-vulkan --bundles msi
    if %ERRORLEVEL% neq 0 (
        echo ERROR: Build failed
        popd
        exit /b 1
    )
    popd
)

if not exist "%RELEASE_DIR%\S2Tui.exe" (
    echo ERROR: Build failed - S2Tui.exe not found
    exit /b 1
)
echo   Build complete
echo.

:: ============================================================================
:: Create Portable Package
:: ============================================================================

echo [3/5] Creating portable package...

:: Clean existing portable dir
if exist "%PORTABLE_DIR%" rmdir /s /q "%PORTABLE_DIR%"

:: Create directory structure
mkdir "%PORTABLE_DIR%\models"

:: Copy EXE
echo   - Copying S2Tui.exe
copy "%RELEASE_DIR%\S2Tui.exe" "%PORTABLE_DIR%\" >nul

:: Copy models
echo   - Copying ggml-small.bin
copy "%MODELS_DIR%\ggml-small.bin" "%PORTABLE_DIR%\models\" >nul
echo   - Copying ggml-large-v3-turbo.bin
copy "%MODELS_DIR%\ggml-large-v3-turbo.bin" "%PORTABLE_DIR%\models\" >nul

echo   Portable package created
echo.

:: ============================================================================
:: Create ZIP Archive
:: ============================================================================

echo [4/5] Creating ZIP archive...

:: Remove existing ZIP
if exist "%RELEASE_DIR%\S2Tui-portable-windows.zip" del "%RELEASE_DIR%\S2Tui-portable-windows.zip"

:: Create ZIP using PowerShell
powershell -NoProfile -Command "Compress-Archive -Path '%PORTABLE_DIR%\*' -DestinationPath '%RELEASE_DIR%\S2Tui-portable-windows.zip' -CompressionLevel Optimal"

if %ERRORLEVEL% neq 0 (
    echo ERROR: Failed to create ZIP archive
    exit /b 1
)
echo   ZIP archive created
echo.

:: ============================================================================
:: Summary
:: ============================================================================

echo [5/5] Build Summary
echo.
echo   Output files:
echo.

:: Show EXE
for %%F in ("%RELEASE_DIR%\S2Tui.exe") do (
    set "SIZE=%%~zF"
    setlocal enabledelayedexpansion
    set /a "SIZE_MB=!SIZE!/1048576"
    echo   [EXE] S2Tui.exe ^(!SIZE_MB! MB^)
    endlocal
)
echo         %RELEASE_DIR%\S2Tui.exe

:: Show MSI
for %%F in ("%RELEASE_DIR%\bundle\msi\*.msi") do (
    set "SIZE=%%~zF"
    setlocal enabledelayedexpansion
    set /a "SIZE_MB=!SIZE!/1048576"
    echo.
    echo   [MSI] %%~nxF ^(!SIZE_MB! MB^)
    echo         %%F
    endlocal
)

:: Show ZIP
for %%F in ("%RELEASE_DIR%\S2Tui-portable-windows.zip") do (
    set "SIZE=%%~zF"
    setlocal enabledelayedexpansion
    set /a "SIZE_MB=!SIZE!/1048576"
    echo.
    echo   [ZIP] S2Tui-portable-windows.zip ^(!SIZE_MB! MB^)
    echo         %RELEASE_DIR%\S2Tui-portable-windows.zip
    endlocal
)

echo.
echo   Portable package contents:
echo     portable\
echo       S2Tui.exe
echo       models\
echo         ggml-small.bin
echo         ggml-large-v3-turbo.bin
echo.
echo ===============================================================
echo   Build completed successfully!
echo ===============================================================
echo.

endlocal
