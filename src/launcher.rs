extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::convert::TryFrom;
use uefi::boot::{self, LoadImageSource};
use uefi::proto::device_path::build::{media, DevicePathBuilder};
use uefi::proto::device_path::DevicePath;
use uefi::proto::loaded_image::LoadedImage;
use uefi::proto::BootPolicy;
use uefi::{CString16, Handle, Result, Status};

use crate::config::BootEntry;

pub fn boot_linux(image: Handle, entry: &BootEntry) -> Result {
    let mut full_device_path_storage = Vec::new();
    let full_device_path = build_kernel_device_path(image, entry, &mut full_device_path_storage)?;
    let child = boot::load_image(
        image,
        LoadImageSource::FromDevicePath {
            device_path: full_device_path,
            boot_policy: BootPolicy::ExactMatch,
        },
    )?;

    let load_options = build_linux_load_options(entry)?;
    if let Some(load_options) = load_options {
        let mut child_image = boot::open_protocol_exclusive::<LoadedImage>(child)?;
        let bytes = load_options.as_slice_with_nul();
        let size = u32::try_from(core::mem::size_of_val(bytes)).map_err(|_| Status::LOAD_ERROR)?;
        unsafe {
            child_image.set_load_options(load_options.as_ptr().cast::<u8>(), size);
        }
    }

    match boot::start_image(child) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = boot::unload_image(child);
            Err(err)
        }
    }
}

fn build_kernel_device_path<'a>(
    image: Handle,
    entry: &BootEntry,
    storage: &'a mut Vec<u8>,
) -> Result<&'a DevicePath> {
    let device_handle = if let Some(device) = entry.device {
        device
    } else {
        let loaded_image = boot::open_protocol_exclusive::<LoadedImage>(image)?;
        loaded_image.device().ok_or(Status::NOT_FOUND)?
    };
    let device_path = boot::open_protocol_exclusive::<DevicePath>(device_handle)?;
    let kernel_path = CString16::try_from(
        normalize_efi_path(
            entry.kernel.as_deref().ok_or(Status::INVALID_PARAMETER)?,
            true,
        )
        .as_str(),
    )
    .map_err(|_| Status::INVALID_PARAMETER)?;

    let mut builder = DevicePathBuilder::with_vec(storage);
    for node in device_path.node_iter() {
        builder = builder.push(&node).map_err(|_| Status::LOAD_ERROR)?;
    }
    builder
        .push(&media::FilePath {
            path_name: kernel_path.as_ref(),
        })
        .map_err(|_| Status::LOAD_ERROR)?
        .finalize()
        .map_err(|_| Status::LOAD_ERROR.into())
}

fn build_linux_load_options(entry: &BootEntry) -> Result<Option<CString16>> {
    let mut parts = Vec::<String>::new();
    let cmdline = entry.cmdline.as_deref().unwrap_or_default();

    if let Some(initrd) = entry.initrd.as_deref() {
        let initrd_path = normalize_efi_path(initrd, true);
        parts.push(format_arg("initrd", &initrd_path));
    }

    if let Some(rootfs) = entry.rootfs.as_deref() {
        if !cmdline
            .split_ascii_whitespace()
            .any(|arg| arg.starts_with("root="))
        {
            parts.push(format_arg("root", rootfs));
        }
    }

    let trimmed = cmdline.trim();
    if !trimmed.is_empty() {
        parts.push(trimmed.into());
    }

    if parts.is_empty() {
        return Ok(None);
    }

    CString16::try_from(parts.join(" ").as_str())
        .map(Some)
        .map_err(|_| Status::INVALID_PARAMETER.into())
}

fn format_arg(key: &str, value: &str) -> String {
    let mut arg = String::with_capacity(key.len() + value.len() + 1);
    arg.push_str(key);
    arg.push('=');
    arg.push_str(value);
    arg
}

fn normalize_efi_path(path: &str, absolute: bool) -> String {
    let mut normalized = String::with_capacity(path.len() + 1);
    if absolute && !matches!(path.as_bytes().first(), Some(b'\\' | b'/')) {
        normalized.push('\\');
    }
    for ch in path.chars() {
        if ch == '/' {
            normalized.push('\\');
        } else {
            normalized.push(ch);
        }
    }
    normalized
}
