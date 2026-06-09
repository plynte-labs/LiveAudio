@echo off
title Compilar LiveAudio con PyInstaller - Plynte
cd /d "%~dp0"
echo ====================================================
echo  Compilador PyInstaller para LiveAudio
echo ====================================================
echo.
echo Este script instalara PyInstaller y compilara LiveAudio
echo en una carpeta distribuible con un ejecutable (.exe).
echo.
echo Requisitos:
echo  - Python y pip instalados
echo  - Todas las dependencias de requirements.txt instaladas
echo.
pause
echo.
echo [1/2] Instalando PyInstaller...
pip install pyinstaller
echo.
echo [2/2] Compilando con PyInstaller (usando LiveAudio.spec)...
pyinstaller --clean LiveAudio.spec
echo.
echo Compilacion terminada. El ejecutable estara en la carpeta dist/LiveAudio/
pause
