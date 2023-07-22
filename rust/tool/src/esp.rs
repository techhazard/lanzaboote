use std::{path::{PathBuf, Path}, array::IntoIter};

use anyhow::{Context, Result};

use crate::generation::Generation;

/// Generic ESP paths which can be specific to a bootloader
// The const generics is necessary because:
// (a) we cannot return Trait objects in trait definitions, see https://rust-lang.github.io/impl-trait-initiative/explainer/rpit_trait.html
// (b) we cannot return std::slice::Iter<'_, PathBuf> because PathBuf is owned
// (c) we cannot return std::slice::Iter<'_, &PathBuf> because then Item = &&PathBuf
// (d) we cannot return std::slice::Iter<'_, Path> because Path is unsized
// (e) we cannot return std::slice::Iter<'_, &Path> because then Item = &&Path
// (f) we can bring an associated type `BorrowedIterator` and then because we want to return
// borrows, Rust needs a lifetime and the struct cannot hold the borrow so you need to bring a
// PhantomData<'a ()> inside the structure to keep the lifetime, this is needlessly complicated
// when this solution ensures proper monomorphisation at a small complexity cost.
pub trait EspPaths<const N: usize> {
    /// Build an ESP path structure out of the ESP root directory
    fn new(esp: impl AsRef<Path>) -> Self;
    /// Return the used file paths to store as garbage collection roots.
    fn iter(&self) -> std::array::IntoIter<&PathBuf, N>;
    /// Returns the path containing NixOS EFI binaries
    fn nixos_path(&self) -> &Path;
    /// Returns the path containing Linux EFI binaries
    fn linux_path(&self) -> &Path;
}

/// Paths to the boot files of a specific generation.
pub struct EspGenerationPaths {
    pub kernel: PathBuf,
    pub initrd: PathBuf,
    pub lanzaboote_image: PathBuf,
}

impl EspGenerationPaths {
    pub fn new<const N: usize, P: EspPaths<N>>(esp_paths: &P, generation: &Generation) -> Result<Self> {
        let bootspec = &generation.spec.bootspec.bootspec;

        Ok(Self {
            kernel: esp_paths
                .nixos_path()
                .join(nixos_path(&bootspec.kernel, "bzImage")?),
            initrd: esp_paths.nixos_path().join(nixos_path(
                bootspec
                    .initrd
                    .as_ref()
                    .context("Lanzaboote does not support missing initrd yet")?,
                "initrd",
            )?),
            lanzaboote_image: esp_paths.linux_path().join(generation_path(generation)),
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
