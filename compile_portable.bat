@echo off
title Compilar LiveAudio Portatil - Plynte
cd /d "%~dp0"
echo ====================================================
echo  Constructor de LiveAudio Portatil para Windows
echo ====================================================
echo.
echo Este script ejecutara 'build_portable.py' para descargar
echo un entorno limpio de Python, instalar PyTorch/Whisper,
echo copiar el codigo fuente y generar los lanzadores .bat.
echo.
echo Requisitos:
echo  - Python instalado en el sistema
echo  - Conexion a Internet (para descargar dependencias)
echo.
pause
echo.
python build_portable.py
echo.
echo Proceso finalizado.
pause
