use alloc::{format, string::ToString, vec, vec::Vec};
use uefi::{
    cstr16, guid,
    prelude::{BootServices, RuntimeServices},
    proto::{
        device_path::{
            media::{HardDrive, PartitionSignature},
            text::DevicePathToText,
            DevicePath, DeviceSubType, DeviceType,
        },
        loaded_image::LoadedImage,
    },
    table::{
        runtime::{VariableAttributes, VariableVendor},
        Boot, SystemTable,
    },
    CStr16, Guid, Handle, Result,
};

use bitflags::bitflags;

/// Fetch the PARTUUID of a given disk
/// FIXME(security): UEFI makes no practical guarantee about the unicity of PARTUUID
/// This can become a problem when the threat model relies on PARTUUID unicity to
/// detect the correct disk to unlock.
/// See https://github.com/systemd/systemd/issues/28491 for an example.
fn disk_get_part_uuid(boot_services: &BootServices, disk_handle: Handle) -> Result<Guid> {
    let dp = boot_services.open_protocol_exclusive::<DevicePath>(disk_handle)?;

    for node in dp.node_iter() {
        if node.device_type() != DeviceType::MEDIA
            || node.sub_type() != DeviceSubType::MEDIA_HARD_DRIVE
        {
            continue;
        }

        if let Ok(hd_path) = <&HardDrive>::try_from(node) {
            if let PartitionSignature::Guid(guid) = hd_path.partition_signature() {
                return Ok(guid);
            }
        }
    }

    Err(uefi::Status::UNSUPPORTED.into())
}

/// systemd loader's GUID
/// != systemd's GUID
/// https://github.com/systemd/systemd/blob/main/src/boot/efi/util.h#L114-L121
/// https://systemd.io/BOOT_LOADER_INTERFACE/
pub const BOOT_LOADER_VENDOR_UUID: VariableVendor =
    VariableVendor(guid!("4a67b082-0a4c-41cf-b6c7-440b29bb8c4f"));

bitflags! {
    #[repr(transparent)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
    /// Feature flags as described in https://systemd.io/BOOT_LOADER_INTERFACE/
    pub struct EfiLoaderFeatures: u64 {
       const ConfigTimeout = 1 << 0;
       const ConfigTimeoutOneShot = 1 << 1;
       const EntryDefault = 1 << 2;
       const EntryOneshot = 1 << 3;
       const BootCounting = 1 << 4;
       const XBOOTLDR = 1 << 5;
       const RandomSeed = 1 << 6;
       const LoadDriver = 1 << 7;
       const SortKey = 1 << 8;
       const SavedEntry = 1 << 9;
       const DeviceTree = 1 << 10;
    }
}

/// Get the currently supported EFI features from the loader if they do exist
/// https://systemd.io/BOOT_LOADER_INTERFACE/
pub fn get_loader_features(runtime_services: &RuntimeServices) -> Result<EfiLoaderFeatures> {
    if let Ok(size) =
        runtime_services.get_variable_size(cstr16!("LoaderFeatures"), &BOOT_LOADER_VENDOR_UUID)
    {
        let mut buffer = vec![0; size].into_boxed_slice();
        runtime_services.get_variable(
            cstr16!("LoaderFeatures"),
            &BOOT_LOADER_VENDOR_UUID,
            &mut buffer,
        )?;

        return EfiLoaderFeatures::from_bits(u64::from_le_bytes(
            (*buffer)
                .try_into()
                .map_err(|_err| uefi::Status::BAD_BUFFER_SIZE)?,
        ))
        .ok_or_else(|| uefi::Status::INCOMPATIBLE_VERSION.into());
    }

    Ok(Default::default())
}

bitflags! {
    #[repr(transparent)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    /// Feature flags as described in https://www.freedesktop.org/software/systemd/man/systemd-stub.html
    pub struct EfiStubFeatures: u64 {
       /// Is `LoaderDevicePartUUID` loaded in UEFI variables?
       const ReportBootPartition = 1 << 0;
       /// Are credentials picked up from the boot partition?
       const PickUpCredentials = 1 << 1;
       /// Are system extensions picked up from the boot partition?
       const PickUpSysExts = 1 << 2;
       /// Are we able to measure kernel image, parameters and sysexts?
       const ThreePcrs = 1 << 3;
       /// Can we pass a random seed to the kernel?
       const RandomSeed = 1 << 4;
    }
}

// This won't work on a big endian system.
// But okay, we do not really care, do we?
#[cfg(target_endian = "little")]
pub fn from_u16(from: &[u16]) -> &[u8] {
    unsafe {
        core::slice::from_raw_parts(from.as_ptr() as *mut u8, from.len().checked_mul(2).unwrap())
    }
}

// Remove me when https://github.com/rust-osdev/uefi-rs/pull/788 lands
pub fn cstr16_to_bytes(s: &CStr16) -> &[u8] {
    from_u16(s.to_u16_slice_with_nul())
}

/// Ensures that an UEFI variable is set or set it with a fallback value
/// computed in a lazy way.
pub fn ensure_efi_variable<F>(
    runtime_services: &RuntimeServices,
    name: &CStr16,
    vendor: &VariableVendor,
    attributes: VariableAttributes,
    get_fallback_value: F,
) -> uefi::Result
where
    F: FnOnce() -> uefi::Result<Vec<u8>>,
{
    // If we get a variable size, a variable already exist.
    if runtime_services.get_variable_size(name, vendor).is_err() {
        runtime_services.set_variable(name, vendor, attributes, &get_fallback_value()?)?;
    }

    Ok(())
}

/// Exports systemd-stub style EFI variables
pub fn export_efi_variables(stub_info_name: &str, system_table: &SystemTable<Boot>) -> Result<()> {
    let boot_services = system_table.boot_services();
    let runtime_services = system_table.runtime_services();

    let stub_features: EfiStubFeatures = EfiStubFeatures::ReportBootPartition;

    let loaded_image =
        boot_services.open_protocol_exclusive::<LoadedImage>(boot_services.image_handle())?;

    let default_attributes =
        VariableAttributes::BOOTSERVICE_ACCESS | VariableAttributes::RUNTIME_ACCESS;

    #[allow(unused_must_use)]
    // LoaderDevicePartUUID
    ensure_efi_variable(
        runtime_services,
        cstr16!("LoaderDevicePartUUID"),
        &BOOT_LOADER_VENDOR_UUID,
        default_attributes,
        || {
            disk_get_part_uuid(boot_services, loaded_image.device()).map(|guid| {
                guid.to_string()
                    .encode_utf16()
                    .flat_map(|c| c.to_le_bytes())
                    .collect::<Vec<u8>>()
            })
        },
    )
    .ok();
    // LoaderImageIdentifier
    ensure_efi_variable(
        runtime_services,
        cstr16!("LoaderImageIdentifier"),
        &BOOT_LOADER_VENDOR_UUID,
        default_attributes,
        || {
            if let Some(dp) = loaded_image.file_path() {
                let dp_protocol = boot_services.open_protocol_exclusive::<DevicePathToText>(
                    boot_services.get_handle_for_protocol::<DevicePathToText>()?,
                )?;
                dp_protocol
                    .convert_device_path_to_text(
                        boot_services,
                        dp,
                        uefi::proto::device_path::text::DisplayOnly(false),
                        uefi::proto::device_path::text::AllowShortcuts(false),
                    )
                    .map(|ps| cstr16_to_bytes(&ps).to_vec())
            } else {
                // If we cannot retrieve the filepath of the loaded image
                // Then, we cannot set `LoaderImageIdentifier`.
                Err(uefi::Status::UNSUPPORTED.into())
            }
        },
    )
    .ok();
    // LoaderFirmwareInfo
    ensure_efi_variable(
        runtime_services,
        cstr16!("LoaderFirmwareInfo"),
        &BOOT_LOADER_VENDOR_UUID,
        default_attributes,
        || {
            Ok(format!(
                "{} {}.{:02}",
                system_table.firmware_vendor(),
                system_table.firmware_revision() >> 16,
                system_table.firmware_revision() & 0xFFFFF
            )
            .encode_utf16()
            .flat_map(|c| c.to_le_bytes())
            .collect::<Vec<u8>>())
        },
    )
    .ok();
    // LoaderFirmwareType
    ensure_efi_variable(
        runtime_services,
        cstr16!("LoaderFirmwareType"),
        &BOOT_LOADER_VENDOR_UUID,
        default_attributes,
        || {
            Ok(format!("UEFI {:02}", system_table.uefi_revision())
                .encode_utf16()
                .flat_map(|c| c.to_le_bytes())
                .collect::<Vec<u8>>())
        },
    )
    .ok();
    // StubInfo
    // FIXME: ideally, no one should be able to overwrite `StubInfo`, but that would require
    // constructing an EFI authenticated variable payload. This seems overcomplicated for now.
    runtime_services
        .set_variable(
            cstr16!("StubInfo"),
            &BOOT_LOADER_VENDOR_UUID,
            default_attributes,
            &stub_info_name
                .encode_utf16()
                .flat_map(|c| c.to_le_bytes())
                .collect::<Vec<u8>>(),
        )
        .ok();

    // StubFeatures
    runtime_services
        .set_variable(
            cstr16!("StubFeatures"),
            &BOOT_LOADER_VENDOR_UUID,
            default_attributes,
            &stub_features.bits().to_le_bytes(),
        )
        .ok();

    Ok(())
}
