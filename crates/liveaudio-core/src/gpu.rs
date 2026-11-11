// SPDX-License-Identifier: MIT
//! GPU / CUDA driver detection and ASR backend selection.

/// Minimum CUDA driver API version (`cuDriverGetVersion`) for the CUDA 12.8 runtime.
pub const MIN_CUDA_DRIVER_VERSION: i32 = 12080;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Other(u32),
}

impl GpuVendor {
    pub fn from_pci_vendor_id(id: u32) -> Self {
        match id {
            0x10DE => Self::Nvidia,
            0x1002 => Self::Amd,
            0x8086 => Self::Intel,
            other => Self::Other(other),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuAdapter {
    pub vendor: GpuVendor,
    pub name: String,
    pub dedicated_vram_mb: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsrBackendPref {
    Auto,
    Ort,
    Ct2Cuda,
}

/// Concrete worker backend (distinct from the `asr::AsrBackend` trait).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsrBackendKind {
    Ort,
    Ct2Cuda,
}

pub fn select_asr_backend(
    pref: AsrBackendPref,
    adapters: &[GpuAdapter],
    cuda_driver_version: Option<i32>,
    ct2_runtime_ready: bool,
) -> (AsrBackendKind, &'static str) {
    use AsrBackendKind::{Ct2Cuda, Ort};
    if pref == AsrBackendPref::Ort {
        return (Ort, "user_selected_ort");
    }
    if !adapters.iter().any(|a| a.vendor == GpuVendor::Nvidia) {
        return (Ort, "no_nvidia_gpu");
    }
    match cuda_driver_version {
        None => (Ort, "cuda_driver_missing"),
        Some(v) if v < MIN_CUDA_DRIVER_VERSION => (Ort, "cuda_driver_too_old"),
        Some(_) if !ct2_runtime_ready => (Ort, "nvidia_runtime_not_installed"),
        Some(_) => (Ct2Cuda, "nvidia_cuda_ready"),
    }
}

/// Enumerates hardware GPU adapters (software adapters skipped). Empty on non-Windows or on failure.
#[cfg(windows)]
pub fn detect_gpu_adapters() -> Vec<GpuAdapter> {
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
    };
    let Ok(factory) = (unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for i in 0.. {
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(i) }) else { break };
        let Ok(desc) = (unsafe { adapter.GetDesc1() }) else { continue };
        let end = desc.Description.iter().position(|&c| c == 0).unwrap_or(desc.Description.len());
        let name = String::from_utf16_lossy(&desc.Description[..end]);
        if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0
            || name.contains("Microsoft Basic Render")
        {
            continue;
        }
        out.push(GpuAdapter {
            vendor: GpuVendor::from_pci_vendor_id(desc.VendorId),
            name,
            dedicated_vram_mb: (desc.DedicatedVideoMemory / (1024 * 1024)) as u64,
        });
    }
    out
}

#[cfg(not(windows))]
pub fn detect_gpu_adapters() -> Vec<GpuAdapter> {
    Vec::new()
}

/// CUDA driver API version from `nvcuda.dll` (e.g. 12080). `None` when absent.
/// `cuDriverGetVersion` is callable without `cuInit`.
#[cfg(windows)]
pub fn cuda_driver_version() -> Option<i32> {
    use windows::core::{s, w};
    use windows::Win32::Foundation::FreeLibrary;
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    unsafe {
        let lib = LoadLibraryW(w!("nvcuda.dll")).ok()?;
        let version = GetProcAddress(lib, s!("cuDriverGetVersion")).and_then(|f| {
            let f: unsafe extern "system" fn(*mut i32) -> i32 = std::mem::transmute(f);
            let mut v = 0;
            (f(&mut v) == 0).then_some(v)
        });
        let _ = FreeLibrary(lib);
        version
    }
}

#[cfg(not(windows))]
pub fn cuda_driver_version() -> Option<i32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(vendor: GpuVendor) -> GpuAdapter {
        GpuAdapter { vendor, name: "gpu".into(), dedicated_vram_mb: 1024 }
    }

    #[test]
    fn vendor_from_pci_id() {
        assert_eq!(GpuVendor::from_pci_vendor_id(0x10DE), GpuVendor::Nvidia);
        assert_eq!(GpuVendor::from_pci_vendor_id(0x1002), GpuVendor::Amd);
        assert_eq!(GpuVendor::from_pci_vendor_id(0x8086), GpuVendor::Intel);
        assert_eq!(GpuVendor::from_pci_vendor_id(0x1234), GpuVendor::Other(0x1234));
    }

    #[test]
    fn select_backend_table() {
        use AsrBackendKind::{Ct2Cuda, Ort};
        use AsrBackendPref::{Auto, Ct2Cuda as PrefCt2, Ort as PrefOrt};
        let nv = [gpu(GpuVendor::Nvidia)];
        let amd = [gpu(GpuVendor::Amd), gpu(GpuVendor::Intel)];
        let none: [GpuAdapter; 0] = [];
        let cases: &[(AsrBackendPref, &[GpuAdapter], Option<i32>, bool, AsrBackendKind, &str)] = &[
            (PrefOrt, &nv, Some(12080), true, Ort, "user_selected_ort"),
            (Auto, &nv, Some(12080), true, Ct2Cuda, "nvidia_cuda_ready"),
            (PrefCt2, &nv, Some(13000), true, Ct2Cuda, "nvidia_cuda_ready"),
            (Auto, &none, Some(12080), true, Ort, "no_nvidia_gpu"),
            (Auto, &amd, Some(12080), true, Ort, "no_nvidia_gpu"),
            (PrefCt2, &amd, None, false, Ort, "no_nvidia_gpu"),
            (Auto, &nv, None, true, Ort, "cuda_driver_missing"),
            (Auto, &nv, Some(12079), true, Ort, "cuda_driver_too_old"),
            (Auto, &nv, Some(12080), false, Ort, "nvidia_runtime_not_installed"),
            (PrefCt2, &nv, Some(11000), false, Ort, "cuda_driver_too_old"),
        ];
        for (pref, ad, drv, ready, want, reason) in cases {
            assert_eq!(
                select_asr_backend(*pref, ad, *drv, *ready),
                (*want, *reason),
                "{pref:?} {drv:?} {ready}"
            );
        }
    }

    #[test]
    #[ignore = "probes real hardware"]
    fn smoke_print_adapters() {
        let adapters = detect_gpu_adapters();
        println!("adapters: {adapters:#?}");
        println!("cuda driver: {:?}", cuda_driver_version());
    }
}
