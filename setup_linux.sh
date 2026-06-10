#!/bin/bash
# SPDX-License-Identifier: MIT

# Detener el script ante cualquier error
set -e

echo "============================================="
echo "  LiveAudio - Instalador para Ubuntu/Linux"
echo "============================================="

# 1. Instalar dependencias del sistema
echo -e "\n[1/4] Instalando dependencias del sistema (requiere sudo)..."
sudo apt-get update
sudo apt-get install -y \
    python3-pip \
    python3-venv \
    python3-tk \
    portaudio19-dev \
    python3-dev \
    build-essential

# 2. Crear entorno virtual
echo -e "\n[2/4] Creando entorno virtual de Python..."
if [ ! -d "venv" ]; then
    python3 -m venv venv
    echo "-> Entorno virtual 'venv' creado con éxito."
else
    echo "-> El entorno virtual 'venv' ya existe, saltando creación."
fi

# 3. Activar e instalar requerimientos
echo -e "\n[3/4] Activando entorno e instalando librerías de Python..."
source venv/bin/activate

echo "-> Actualizando pip..."
pip install --upgrade pip

echo "-> Instalando dependencias de requirements.txt..."
pip install -r requirements.txt

# 4. Finalizar
echo -e "\n[4/4] ¡Instalación completada con éxito!"
echo "============================================="
echo "Para ejecutar LiveAudio:"
echo "  1. Activá el entorno: source venv/bin/activate"
echo "  2. Iniciá la app:     python3 main.py"
echo "============================================="
