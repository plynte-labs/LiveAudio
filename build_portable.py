# SPDX-License-Identifier: MIT
import os
import sys
import shutil
import urllib.request
import zipfile
import subprocess

# Configuración básica
PYTHON_VERSION = "3.10.11"
PYTHON_ZIP_URL = f"https://www.python.org/ftp/python/{PYTHON_VERSION}/python-{PYTHON_VERSION}-embed-amd64.zip"
GET_PIP_URL = "https://bootstrap.pypa.io/get-pip.py"

ROOT_DIR = os.path.abspath(os.path.dirname(__file__))
DIST_DIR = os.path.join(ROOT_DIR, "dist")
APP_DIR = os.path.join(DIST_DIR, "LiveAudio")
LOCAL_PYTHON_DIR = os.path.join(APP_DIR, "python")
CACHE_DIR = os.path.join(ROOT_DIR, ".build_cache")

def print_banner(text):
    print("\n" + "=" * 60)
    print(f" {text}")
    print("=" * 60)

def download_file(url, dest_path):
    print(f"Descargando: {url}")
    print(f"Destino: {dest_path}")
    
    # Reportar progreso
    def reporthook(block_num, block_size, total_size):
        read_so_far = block_num * block_size
        if total_size > 0:
            percent = min(100, (read_so_far * 100) // total_size)
            sys.stdout.write(f"\rProgreso: {percent}% ({read_so_far // (1024*1024)}MB / {total_size // (1024*1024)}MB)")
        else:
            sys.stdout.write(f"\rDescargados: {read_so_far // 1024} KB")
        sys.stdout.flush()
        
    urllib.request.urlretrieve(url, dest_path, reporthook)
    print("\n¡Descarga completada!")

def main():
    if sys.platform != "win32":
        print("ERROR: Este script de empaquetado portátil está diseñado exclusivamente para Windows.")
        sys.exit(1)

    print_banner("Plynte LiveAudio - Constructor de Distribución Portátil")
    print(f"Este script generará una carpeta autocontenida de LiveAudio en: {APP_DIR}")
    
    # Crear directorios
    os.makedirs(CACHE_DIR, exist_ok=True)
    
    if os.path.exists(APP_DIR):
        print(f"\n[!] ATENCIÓN: La carpeta de destino {APP_DIR} ya existe.")
        res = input("¿Desea eliminarla y realizar una instalación limpia? (s/n): ").strip().lower()
        if res == 's':
            print("Eliminando instalación anterior...")
            shutil.rmtree(APP_DIR)
        else:
            print("Abortando proceso para no sobreescribir datos.")
            sys.exit(0)
            
    os.makedirs(APP_DIR, exist_ok=True)
    os.makedirs(LOCAL_PYTHON_DIR, exist_ok=True)

    # 1. Obtener Python Embeddable
    zip_cache_path = os.path.join(CACHE_DIR, f"python-{PYTHON_VERSION}-embed-amd64.zip")
    if not os.path.exists(zip_cache_path):
        print("\n--- Paso 1: Descargando intérprete de Python embebido oficial ---")
        download_file(PYTHON_ZIP_URL, zip_cache_path)
    else:
        print("\n--- Paso 1: Usando Python embebido desde la caché local ---")

    # Extraer Python
    print("Extrayendo entorno de Python...")
    with zipfile.ZipFile(zip_cache_path, 'r') as zip_ref:
        zip_ref.extractall(LOCAL_PYTHON_DIR)
    print("Intérprete extraído con éxito.")

    # 2. Configurar el archivo ._pth para habilitar pip y site-packages
    print("\n--- Paso 2: Configurando sistema de rutas de Python (site-packages) ---")
    pth_file = os.path.join(LOCAL_PYTHON_DIR, "python310._pth")
    if os.path.exists(pth_file):
        # Reescribimos el archivo para garantizar que 'import site' esté activo
        with open(pth_file, "w") as f:
            f.write("python310.zip\n")
            f.write(".\n")
            f.write("\n")
            f.write("# Habilitar site-packages para soportar pip y librerías externas\n")
            f.write("import site\n")
        print("Archivo python310._pth modificado correctamente.")
    else:
        print("ERROR: No se encontró el archivo python310._pth. Es posible que la versión de Python haya cambiado.")
        sys.exit(1)

    # 3. Descargar e instalar pip
    print("\n--- Paso 3: Instalando administrador de paquetes (pip) ---")
    get_pip_path = os.path.join(CACHE_DIR, "get-pip.py")
    if not os.path.exists(get_pip_path):
        download_file(GET_PIP_URL, get_pip_path)
        
    local_python_exe = os.path.join(LOCAL_PYTHON_DIR, "python.exe")
    print("Ejecutando instalación de pip local...")
    subprocess.run([local_python_exe, get_pip_path], check=True)
    print("pip instalado correctamente en el entorno portátil.")

    # 4. Elegir modo de Hardware (GPU vs CPU)
    print("\n--- Paso 4: Selección de Arquitectura de Hardware para IA ---")
    print("LiveAudio utiliza PyTorch y Whisper para la transcripción en vivo.")
    print("  [1] GPU NVIDIA (Recomendado): Instala PyTorch con soporte CUDA. Súper rápido, ideal para streaming.")
    print("  [2] CPU-Only (Estándar): Menor tamaño de descarga. Adecuado para pruebas o PCs sin gráfica NVIDIA.")
    
    hw_choice = ""
    while hw_choice not in ["1", "2"]:
        hw_choice = input("Seleccione una opción (1 o 2): ").strip()
        
    # 5. Instalar dependencias
    print("\n--- Paso 5: Instalando dependencias de LiveAudio ---")
    
    # Actualizar pip local de seguridad
    subprocess.run([local_python_exe, "-m", "pip", "install", "--upgrade", "pip"], check=True)
    
    if hw_choice == "1":
        print("\n[INFO] Instalando PyTorch con soporte CUDA (cu118)...")
        # cu118 es súper estable y ligera con faster-whisper en Windows
        subprocess.run([
            local_python_exe, "-m", "pip", "install", 
            "torch>=2.0.0,<2.7.0", 
            "--index-url", "https://download.pytorch.org/whl/cu118"
        ], check=True)
    else:
        print("\n[INFO] Instalando PyTorch CPU-Only estándar...")
        subprocess.run([
            local_python_exe, "-m", "pip", "install", 
            "torch>=2.0.0,<2.7.0"
        ], check=True)
        
    # Instalar las demás dependencias declaradas
    print("\nInstalando librerías secundarias (faster-whisper, sounddevice, customtkinter, Pillow, websockets, numpy)...")
    subprocess.run([
        local_python_exe, "-m", "pip", "install",
        "faster-whisper>=1.0.0,<2.0.0",
        "sounddevice>=0.4.6,<0.5.0",
        "numpy>=1.24.0,<2.1.0",
        "customtkinter>=5.2.0,<6.0.0",
        "Pillow>=10.0.0,<12.0.0",
        "websockets>=14.0,<17.0"
    ], check=True)
    
    print("¡Todas las dependencias fueron instaladas con éxito!")

    # 6. Copiar código fuente
    print("\n--- Paso 6: Copiando el código fuente de LiveAudio ---")
    
    # Lista de archivos y carpetas a copiar
    items_to_copy = [
        ("main.py", False),
        ("LiveAudio-Fran.png", False),
        ("logger.py", False),
        ("config.json", False),
        ("opencode.json", False),
        ("subtitulos_obs.html", False),
        ("core", True),
        ("utils", True),
    ]
    
    for item, is_dir in items_to_copy:
        src = os.path.join(ROOT_DIR, item)
        dst = os.path.join(APP_DIR, item)
        
        if not os.path.exists(src):
            print(f"Advertencia: No se encontró {item}, omitiendo.")
            continue
            
        if is_dir:
            print(f"Copiando directorio: {item} -> LiveAudio/")
            # Evitar copiar __pycache__
            shutil.copytree(src, dst, ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
        else:
            print(f"Copiando archivo: {item} -> LiveAudio/")
            shutil.copy2(src, dst)

    # 7. Crear los lanzadores independientes (.bat)
    print("\n--- Paso 7: Creando los lanzadores de un solo clic ---")
    
    # Lanzador normal (sin ventana de consola negra, usa pythonw)
    bat_normal_path = os.path.join(APP_DIR, "LiveAudio.bat")
    with open(bat_normal_path, "w", encoding="utf-8") as f:
        f.write("@echo off\n")
        f.write("cd /d \"%~dp0\"\n")
        f.write("echo Iniciando Plynte LiveAudio...\n")
        f.write("start \"\" \"%~dp0python\\pythonw.exe\" \"%~dp0main.py\"\n")
    print("Creado lanzador: LiveAudio.bat")
        
    # Lanzador de depuración (mantiene la ventana negra abierta para ver logs y crashes)
    bat_debug_path = os.path.join(APP_DIR, "LiveAudio_Debug.bat")
    with open(bat_debug_path, "w", encoding="utf-8") as f:
        f.write("@echo off\n")
        f.write("cd /d \"%~dp0\"\n")
        f.write("echo ====================================================\n")
        f.write("echo  Iniciando Plynte LiveAudio en MODO DEPURACION\n")
        f.write("echo ====================================================\n")
        f.write("\"%~dp0python\\python.exe\" \"%~dp0main.py\"\n")
        f.write("echo.\n")
        f.write("echo El proceso ha finalizado.\n")
        f.write("pause\n")
    print("Creado lanzador: LiveAudio_Debug.bat")

    print_banner("¡PROCESO FINALIZADO CON ÉXITO!")
    print(f"Tu aplicación portátil completa está lista en: {APP_DIR}")
    print("\n¿Cómo distribuirla?")
    print("  1. Simplemente comprimí la carpeta 'LiveAudio' en un archivo .zip.")
    print("  2. El usuario final solo tiene que descomprimir el .zip y hacer doble clic en 'LiveAudio.bat'.")
    print("  3. ¡No requieren instalar Python, git, pip, ni dependencias nativas complejas!")
    print("=" * 60)

if __name__ == "__main__":
    main()
