# SPDX-License-Identifier: MIT
import sys
import traceback
import webbrowser
import urllib.parse
import customtkinter as ctk
from tkinter import messagebox

GITHUB_ISSUES_URL = "https://github.com/plynte-labs/LiveAudio/issues/new"

def show_crash_dialog(error_type, error_value, tb):
    """
    Muestra un diálogo de error premium que permite al usuario copiar el log
    de error y abrir un issue en GitHub de forma segura.
    """
    tb_lines = traceback.format_exception(error_type, error_value, tb)
    tb_text = "".join(tb_lines)
    
    # Intentar inicializar un diálogo CTk
    try:
        app = ctk.CTk()
        app.title("LiveAudio - Error Inesperado")
        app.geometry("700x500")
        app.resizable(False, False)
        ctk.set_appearance_mode("dark")
        ctk.set_default_color_theme("green")
        
        # Centrar la ventana
        app.update_idletasks()
        width = app.winfo_width()
        height = app.winfo_height()
        x = (app.winfo_screenwidth() // 2) - (width // 2)
        y = (app.winfo_screenheight() // 2) - (height // 2)
        app.geometry(f"+{x}+{y}")
        
        # Estilo Chihuahua / Plynte
        lbl_title = ctk.CTkLabel(
            app,
            text="⚠️ LiveAudio ha detectado un error y debió cerrarse",
            font=ctk.CTkFont(size=18, weight="bold", family="Outfit"),
            text_color="#F44336"
        )
        lbl_title.pack(pady=(20, 10))
        
        lbl_desc = ctk.CTkLabel(
            app,
            text="Para ayudarnos a solucionar esto, por favor copia el reporte y repórtalo en nuestro GitHub.",
            font=ctk.CTkFont(size=12, family="Segoe UI"),
            text_color="#AEB8BC",
            wraplength=600
        )
        lbl_desc.pack(pady=(0, 15))
        
        # Textbox con el Traceback
        tb_box = ctk.CTkTextbox(
            app,
            font=ctk.CTkFont(family="Consolas", size=11),
            width=640,
            height=280
        )
        tb_box.pack(padx=30, pady=(0, 20))
        tb_box.insert("0.0", tb_text)
        tb_box.configure(state="disabled")
        
        # Botones de Acción
        btn_frame = ctk.CTkFrame(app, fg_color="transparent")
        btn_frame.pack(fill="x", padx=30)
        
        def copy_log():
            app.clipboard_clear()
            app.clipboard_append(tb_text)
            app.update()
            messagebox.showinfo("Copiado", "El reporte de error fue copiado al portapapeles.")
            
        def report_github():
            body = (
                "### Descripción del problema\n"
                "<!-- Por favor describe qué estabas haciendo antes del crash -->\n\n"
                "### Logs / Traceback\n"
                "```python\n" + tb_text + "\n```"
            )
            # URLencode del issue template
            params = {
                "title": f"Crash Report: {error_value}",
                "body": body
            }
            url = f"{GITHUB_ISSUES_URL}?{urllib.parse.urlencode(params)}"
            webbrowser.open_new(url)
            
        btn_copy = ctk.CTkButton(
            btn_frame,
            text="Copiar Reporte",
            font=ctk.CTkFont(size=12, weight="bold"),
            fg_color="#1a2730",
            hover_color="#2d7a4d",
            command=copy_log
        )
        btn_copy.pack(side="left", padx=(0, 10))
        
        btn_report = ctk.CTkButton(
            btn_frame,
            text="Reportar en GitHub",
            font=ctk.CTkFont(size=12, weight="bold"),
            fg_color="#2d7a4d",
            hover_color="#3c9e66",
            command=report_github
        )
        btn_report.pack(side="left")
        
        btn_close = ctk.CTkButton(
            btn_frame,
            text="Cerrar",
            font=ctk.CTkFont(size=12),
            fg_color="transparent",
            border_width=1,
            command=app.destroy
        )
        btn_close.pack(side="right")
        
        app.mainloop()
    except Exception as e:
        # Fallback si falla customtkinter
        print(f"Error al dibujar la ventana de crash: {e}", file=sys.stderr)
        messagebox.showerror(
            "LiveAudio - Error Inesperado",
            f"La aplicación ha crasheado debido a:\n\n{error_value}\n\nTraceback:\n{tb_text}"
        )

def install_crash_handler():
    """Registra el crash handler global en sys.excepthook."""
    sys.excepthook = show_crash_dialog
