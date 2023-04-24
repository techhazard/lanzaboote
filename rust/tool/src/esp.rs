use std::array::IntoIter;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::generation::Generation;

/// Supported system
#[allow(dead_code)]
#[non_exhaustive]
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Architecture {
    X86,
    AArch64,
}

impl System {
    pub fn systemd_filename(&self) -> &Path {
        Path::new(match self {
            Self::X86 => "systemd-bootx64.efi",
            Self::AArch64 => "systemd-bootaa64.efi"
        })
    }

    pub fn efi_fallback_filename(&self) -> &Path {
        Path::new(match self {
            Self::X86 => "BOOTX64.EFI",
            Self::AArch64 => "BOOTAA64.EFI",
        })
    }
}

impl Architecture {
    /// Converts from a NixOS system double to a supported system
    pub fn from_nixos_system(system_double: &str) -> Result<Self> {
        Ok(match system_double {
            "x86_64-linux" => Self::X86,
            "aarch64-linux" => Self::AArch64,
            _ => bail!("Unsupported NixOS system double: {}, please open an issue or a PR if you think this should be supported.", system_double)
        })
    }
}

/// Paths to the boot files that are not specific to a generation.
pub struct EspPaths {
    pub esp: PathBuf,
    pub efi: PathBuf,
    pub nixos: PathBuf,
    pub linux: PathBuf,
    pub efi_fallback_dir: PathBuf,
    pub systemd: PathBuf,
    pub loader: PathBuf,
    pub systemd_boot_loader_config: PathBuf,
}

impl EspPaths {
    pub fn new(esp: impl AsRef<Path>) -> Self {
        let esp = esp.as_ref();
        let efi = esp.join("EFI");
        let efi_nixos = efi.join("nixos");
        let efi_linux = efi.join("Linux");
        let efi_systemd = efi.join("systemd");
        let efi_efi_fallback_dir = efi.join("BOOT");
        let loader = esp.join("loader");
        let systemd_boot_loader_config = loader.join("loader.conf");

        Self {
            esp: esp.to_path_buf(),
            efi,
            nixos: efi_nixos,
            linux: efi_linux,
            efi_fallback_dir: efi_efi_fallback_dir,
            systemd: efi_systemd,
            loader,
            systemd_boot_loader_config,
        }
    }

    /// Return the used file paths to store as garbage collection roots.
    pub fn to_iter(&self) -> IntoIter<&PathBuf, 8> {
        [
            &self.esp,
            &self.efi,
            &self.nixos,
            &self.linux,
            &self.efi_fallback_dir,
            &self.systemd,
            &self.loader,
            &self.systemd_boot_loader_config,
        ]
        .into_iter()
    }
}

/// Paths to the boot files of a specific generation.
pub struct EspGenerationPaths {
    pub kernel: PathBuf,
    pub initrd: PathBuf,
    pub lanzaboote_image: PathBuf,
}

impl EspGenerationPaths {
    pub fn new(esp_paths: &EspPaths, generation: &Generation) -> Result<Self> {
        let bootspec = &generation.spec.bootspec.bootspec;
        let bootspec_system: Architecture = Architecture::from_nixos_system(&bootspec.system)?;

        assert_eq!(system, bootspec_system, "Bootspec's system differs from provided target system, unsupported usecase!");

        Ok(Self {
            kernel: esp_paths
                .nixos
                .join(nixos_path(&bootspec.kernel, "bzImage")?),
            initrd: esp_paths.nixos.join(nixos_path(
                bootspec
                    .initrd
                    .as_ref()
                    .context("Lanzaboote does not support missing initrd yet")?,
                "initrd",
            )?),
            lanzaboote_image: esp_paths.linux.join(generation_path(generation)),
        })
    }

    /// Return the used file paths to store as garbage collection roots.
    pub fn to_iter(&self) -> IntoIter<&PathBuf, 3> {
        [&self.kernel, &self.initrd, &self.lanzaboote_image].into_iter()
    }
}

fn nixos_path(path: impl AsRef<Path>, name: &str) -> Result<PathBuf> {
    let resolved = path
        .as_ref()
        .read_link()
        .unwrap_or_else(|_| path.as_ref().into());

    let parent_final_component = resolved
        .parent()
        .and_then(|x| x.file_name())
        .and_then(|x| x.to_str())
        .with_context(|| format!("Failed to extract final component from: {:?}", resolved))?;

    let nixos_filename = format!("{}-{}.efi", parent_final_component, name);

    Ok(PathBuf::from(nixos_filename))
}

fn generation_path(generation: &Generation) -> PathBuf {
    if let Some(specialisation_name) = generation.is_specialised() {
        PathBuf::from(format!(
            "nixos-generation-{}-specialisation-{}.efi",
            generation, specialisation_name
        ))
    } else {
        PathBuf::from(format!("nixos-generation-{}.efi", generation))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nixos_path_creates_correct_filename_from_nix_store_path() -> Result<()> {
        let path =
            Path::new("/nix/store/xqplddjjjy1lhzyzbcv4dza11ccpcfds-initrd-linux-6.1.1/initrd");

        let generated_filename = nixos_path(path, "initrd")?;

        let expected_filename =
            PathBuf::from("xqplddjjjy1lhzyzbcv4dza11ccpcfds-initrd-linux-6.1.1-initrd.efi");

        assert_eq!(generated_filename, expected_filename);
        Ok(())
    }
}
