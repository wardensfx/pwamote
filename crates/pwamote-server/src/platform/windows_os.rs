//! Version de Windows, lue dans le registre (pour les signalements).

use windows::core::{w, PCWSTR};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};

fn read(name: PCWSTR) -> Option<String> {
    let mut buf = [0u16; 128];
    let mut size = (buf.len() * 2) as u32;
    // SAFETY : tampon et taille cohérents, chaîne terminée par un zéro garantie par RRF_RT_REG_SZ.
    unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion"),
            name,
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    }
    .ok()
    .ok()?;
    let len = (size as usize / 2).saturating_sub(1).min(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

/// Numéro de build (22631) et version affichée (« 23H2 ») s'ils sont lisibles.
pub fn os_version() -> Option<(u32, Option<String>)> {
    let build = read(w!("CurrentBuildNumber"))?.trim().parse().ok()?;
    Some((build, read(w!("DisplayVersion"))))
}
