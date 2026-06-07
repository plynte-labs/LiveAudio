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

def compile_launcher(app_dir, root_dir):
    print("\n--- Paso 8: Compilando lanzador nativo LiveAudio.exe ---")
    csc_path = r"C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe"
    if not os.path.exists(csc_path):
        print("[!] ADVERTENCIA: No se encontró el compilador de C# (csc.exe) de .NET Framework.")
        print("    No se pudo generar LiveAudio.exe. Los usuarios deberán usar LiveAudio.bat.")
        return

    launcher_cs = os.path.join(root_dir, "Launcher.cs")
    exe_path = os.path.join(app_dir, "LiveAudio.exe")
    icon_path = os.path.join(root_dir, "LiveAudio-Fran.ico")

    csharp_code = r"""using System;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Reflection;
using System.Threading;
using System.Windows.Forms;

class Launcher {
    [STAThread]
    static void Main() {
        Application.EnableVisualStyles();
        Application.SetCompatibleTextRenderingDefault(false);
        Application.Run(new SplashForm());
    }
}

class SplashForm : Form {
    private Thread workerThread;
    private Process pythonProcess;
    private System.Windows.Forms.Timer timeoutTimer;
    private System.Windows.Forms.Timer animTimer;
    private int animFrame = 0;

    protected override CreateParams CreateParams {
        get {
            const int CS_DROPSHADOW = 0x00020000;
            CreateParams cp = base.CreateParams;
            cp.ClassStyle |= CS_DROPSHADOW;
            return cp;
        }
    }

    public SplashForm() {
        this.FormBorderStyle = FormBorderStyle.None;
        this.StartPosition = FormStartPosition.CenterScreen;
        this.Size = new Size(420, 220);
        this.BackColor = Color.FromArgb(17, 27, 30); // #111b1e
        this.DoubleBuffered = true;

        try {
            this.Icon = Icon.ExtractAssociatedIcon(Assembly.GetExecutingAssembly().Location);
        } catch {
            this.ShowIcon = false;
        }

        // Borde fino gris oscuro y línea de progreso customizada
        this.Paint += (s, e) => {
            // Fondo / Borde
            using (Pen borderPen = new Pen(Color.FromArgb(31, 45, 51), 1)) { // #1f2d33
                e.Graphics.DrawRectangle(borderPen, 0, 0, this.Width - 1, this.Height - 1);
            }

            // Barra de progreso de fondo (gris muy oscuro)
            using (SolidBrush bgBrush = new SolidBrush(Color.FromArgb(10, 18, 20))) { // #0a1214
                e.Graphics.FillRectangle(bgBrush, 40, 150, this.Width - 80, 4);
            }

            // Indicador de progreso móvil (Verde Plynte)
            int progressWidth = 100;
            int trackWidth = this.Width - 80 - progressWidth;
            // Movimiento suave sinusoidal
            double phase = (animFrame % 180) * Math.PI / 180.0;
            int offset = (int)((Math.Sin(phase - Math.PI / 2) + 1.0) / 2.0 * trackWidth);
            int progressX = 40 + offset;

            using (SolidBrush barBrush = new SolidBrush(Color.FromArgb(60, 158, 102))) { // #3c9e66
                e.Graphics.FillRectangle(barBrush, progressX, 150, progressWidth, 4);
            }
        };

        // Textos del Splash
        Label lblTitle = new Label();
        lblTitle.Text = "LiveAudio";
        lblTitle.Font = new Font("Segoe UI", 28, FontStyle.Bold);
        lblTitle.ForeColor = Color.FromArgb(60, 158, 102); // #3c9e66
        lblTitle.AutoSize = false;
        lblTitle.Size = new Size(400, 55);
        lblTitle.Location = new Point(10, 40);
        lblTitle.TextAlign = ContentAlignment.MiddleCenter;
        this.Controls.Add(lblTitle);

        Label lblStatus = new Label();
        lblStatus.Text = "Iniciando servicios de audio e IA...";
        lblStatus.Font = new Font("Segoe UI Semibold", 10, FontStyle.Regular);
        lblStatus.ForeColor = Color.FromArgb(174, 184, 188); // #aeb8bc
        lblStatus.AutoSize = false;
        lblStatus.Size = new Size(400, 30);
        lblStatus.Location = new Point(10, 105);
        lblStatus.TextAlign = ContentAlignment.MiddleCenter;
        this.Controls.Add(lblStatus);

        this.Load += (s, e) => {
            // Hilos y Timers
            workerThread = new Thread(StartPythonProcess);
            workerThread.IsBackground = true;
            workerThread.Start();

            // Animación
            animTimer = new System.Windows.Forms.Timer();
            animTimer.Interval = 16; // ~60 FPS
            animTimer.Tick += (sender, args) => {
                animFrame += 2;
                this.Invalidate(new Rectangle(40, 150, this.Width - 80, 4)); // Redibujar solo la barra de progreso
            };
            animTimer.Start();

            // Timer de seguridad
            timeoutTimer = new System.Windows.Forms.Timer();
            timeoutTimer.Interval = 10000; // 10 segundos
            timeoutTimer.Tick += (sender, args) => {
                CleanAndClose();
            };
            timeoutTimer.Start();
        };
    }

    private void CleanAndClose() {
        if (animTimer != null) animTimer.Stop();
        if (timeoutTimer != null) timeoutTimer.Stop();
        this.Close();
    }

    private void StartPythonProcess() {
        string baseDir = AppDomain.CurrentDomain.BaseDirectory;
        string pythonw = Path.Combine(baseDir, "python", "pythonw.exe");
        string mainPy = Path.Combine(baseDir, "main.py");

        if (!File.Exists(pythonw)) {
            this.Invoke(new Action(() => {
                CleanAndClose();
                MessageBox.Show(
                    "No se encontró el intérprete de Python en:\n" + pythonw + "\n\nPor favor, reinstale la aplicación.",
                    "Error de Inicio - LiveAudio",
                    MessageBoxButtons.OK,
                    MessageBoxIcon.Error
                );
            }));
            return;
        }

        string torchLibPath = Path.Combine(baseDir, "python", "Lib", "site-packages", "torch", "lib");
        string currentPath = Environment.GetEnvironmentVariable("PATH") ?? "";
        string newPath = File.Exists(Path.Combine(torchLibPath, "cublas64_12.dll"))
            ? torchLibPath + ";" + currentPath
            : currentPath;

        ProcessStartInfo startInfo = new ProcessStartInfo();
        startInfo.FileName = pythonw;
        startInfo.Arguments = "\"" + mainPy + "\"";
        startInfo.WorkingDirectory = baseDir;
        startInfo.UseShellExecute = false;
        startInfo.CreateNoWindow = true;
        startInfo.EnvironmentVariables["PATH"] = newPath;

        try {
            pythonProcess = Process.Start(startInfo);
            if (pythonProcess != null) {
                // Espera a que la app esté lista
                pythonProcess.WaitForInputIdle(10000);
            }
        } catch (Exception ex) {
            this.Invoke(new Action(() => {
                CleanAndClose();
                MessageBox.Show(
                    "Error al iniciar LiveAudio:\n" + ex.Message,
                    "Error - LiveAudio",
                    MessageBoxButtons.OK,
                    MessageBoxIcon.Error
                );
            }));
        }

        this.BeginInvoke(new Action(() => {
            CleanAndClose();
        }));
    }
}
"""
    try:
        with open(launcher_cs, "w", encoding="utf-8") as f:
            f.write(csharp_code)

        cmd = [
            csc_path,
            "/target:winexe",
            f"/out:{exe_path}",
            "/r:System.Windows.Forms.dll",
            "/r:System.Drawing.dll"
        ]

        if os.path.exists(icon_path):
            cmd.append(f"/win32icon:{icon_path}")
        else:
            print("[!] Advertencia: No se encontró LiveAudio-Fran.ico para incrustar.")

        cmd.append(launcher_cs)

        result = subprocess.run(cmd, capture_output=True, text=True)
        if result.returncode == 0:
            print("[+] LiveAudio.exe compilado con éxito e ícono incrustado.")
        else:
            print("[!] Error al compilar el lanzador nativo:")
            print(result.stdout)
            print(result.stderr)
    except Exception as e:
        print(f"[!] Error inesperado al compilar el lanzador: {e}")
    finally:
        if os.path.exists(launcher_cs):
            try:
                os.remove(launcher_cs)
            except Exception:
                pass

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
            f.write("..\n")
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
        print("\n[INFO] Instalando PyTorch con soporte CUDA 12 (cu121)...")
        # cu121 bundlea cublas64_12.dll, requerido por ctranslate2 4.x
        subprocess.run([
            local_python_exe, "-m", "pip", "install",
            "torch>=2.1.0,<2.7.0", "torchaudio>=2.1.0,<2.7.0",
            "--index-url", "https://download.pytorch.org/whl/cu121"
        ], check=True)
    else:
        print("\n[INFO] Instalando PyTorch CPU-Only estándar...")
        subprocess.run([
            local_python_exe, "-m", "pip", "install", 
            "torch>=2.0.0,<2.7.0", "torchaudio>=2.0.0,<2.7.0"
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
    
    # Instalar tkinter-embed para habilitar soporte GUI en Python embebido
    print("\nInstalando soporte de GUI (tkinter-embed) para entorno portátil...")
    subprocess.run([
        local_python_exe, "-m", "pip", "install",
        "tkinter-embed",
        "--target", LOCAL_PYTHON_DIR
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
        f.write("SET PATH=%~dp0python\\Lib\\site-packages\\torch\\lib;%PATH%\n")
        f.write("echo Iniciando Plynte LiveAudio...\n")
        f.write("start \"\" \"%~dp0python\\pythonw.exe\" \"%~dp0main.py\"\n")
    print("Creado lanzador: LiveAudio.bat")
        
    # Lanzador de depuración (mantiene la ventana negra abierta para ver logs y crashes)
    bat_debug_path = os.path.join(APP_DIR, "LiveAudio_Debug.bat")
    with open(bat_debug_path, "w", encoding="utf-8") as f:
        f.write("@echo off\n")
        f.write("cd /d \"%~dp0\"\n")
        f.write("SET PATH=%~dp0python\\Lib\\site-packages\\torch\\lib;%PATH%\n")
        f.write("echo ====================================================\n")
        f.write("echo  Iniciando Plynte LiveAudio en MODO DEPURACION\n")
        f.write("echo ====================================================\n")
        f.write("\"%~dp0python\\python.exe\" \"%~dp0main.py\"\n")
        f.write("echo.\n")
        f.write("echo El proceso ha finalizado.\n")
        f.write("pause\n")
    print("Creado lanzador: LiveAudio_Debug.bat")

    # 8. Compilar el launcher nativo .exe
    compile_launcher(APP_DIR, ROOT_DIR)

    print_banner("¡PROCESO FINALIZADO CON ÉXITO!")
    print(f"Tu aplicación portátil completa está lista en: {APP_DIR}")
    print("\n¿Cómo distribuirla?")
    print("  1. Simplemente comprimí la carpeta 'LiveAudio' en un archivo .zip.")
    print("  2. El usuario final solo tiene que descomprimir el .zip y hacer doble clic en 'LiveAudio.exe'.")
    print("  3. ¡No requieren instalar Python, git, pip, ni dependencias nativas complejas!")
    print("=" * 60)

if __name__ == "__main__":
    main()
