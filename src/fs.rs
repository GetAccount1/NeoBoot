extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use uefi::boot;
use uefi::fs::{FileSystem, Path, PathBuf};
use uefi::proto::loaded_image::LoadedImage;
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::{cstr16, CStr16, Handle, Result, Status};

use crate::config::{BootEntry, EntryType};

const MAX_SCAN_DEPTH: usize = 4;

#[derive(Clone, Copy)]
pub struct ScanProgress {
    pub completed_disks: usize,
    pub total_disks: usize,
}

pub fn read_file(image: Handle, path: &str) -> Result<Vec<u8>> {
    let mut fs = FileSystem::new(uefi::boot::get_image_file_system(image)?);
    let mut path16 = [0u16; 256];
    let len = path.chars().take(255).enumerate().fold(0, |_, (i, c)| {
        path16[i] = c as u16;
        i + 1
    });
    path16[len] = 0;
    let cstr = unsafe { CStr16::from_u16_with_nul_unchecked(&path16[..=len]) };
    fs.read(Path::new(cstr))
        .map_err(|_| Status::LOAD_ERROR.into())
}

pub fn image_device(image: Handle) -> Result<Handle> {
    let loaded_image = boot::open_protocol_exclusive::<LoadedImage>(image)?;
    loaded_image.device().ok_or(Status::NOT_FOUND.into())
}

pub fn scan_linux_entries() -> Vec<BootEntry> {
    scan_linux_entries_with_progress(|_| true).unwrap_or_default()
}

pub fn scan_firmware_boot_entries_with_progress<F>(mut on_progress: F) -> Option<Vec<BootEntry>>
where
    F: FnMut(ScanProgress) -> bool,
{
    let mut entries = Vec::new();
    let Ok(handles) = boot::find_handles::<SimpleFileSystem>() else {
        return Some(entries);
    };
    let total_disks = handles.len();

    if !on_progress(ScanProgress {
        completed_disks: 0,
        total_disks,
    }) {
        return None;
    }

    for (index, handle) in handles.into_iter().enumerate() {
        let Ok(protocol) = boot::open_protocol_exclusive::<SimpleFileSystem>(handle) else {
            if !on_progress(ScanProgress {
                completed_disks: index + 1,
                total_disks,
            }) {
                return None;
            }
            continue;
        };
        let mut fs = FileSystem::new(protocol);

        scan_boot_target_entries(&mut fs, handle, &mut entries);
        if let Some(entry) = detect_windows_boot(&mut fs, handle) {
            push_unique_entry(&mut entries, entry);
        }

        let mut linux_entries = Vec::new();
        scan_dir(
            &mut fs,
            PathBuf::from(cstr16!("\\")),
            handle,
            index + 1,
            0,
            &mut linux_entries,
        );
        for entry in linux_entries {
            push_unique_entry(&mut entries, make_firmware_volume_entry(entry));
        }

        if !on_progress(ScanProgress {
            completed_disks: index + 1,
            total_disks,
        }) {
            return None;
        }
    }

    Some(entries)
}

pub fn scan_linux_entries_with_progress<F>(mut on_progress: F) -> Option<Vec<BootEntry>>
where
    F: FnMut(ScanProgress) -> bool,
{
    let mut entries = Vec::new();
    let Ok(handles) = boot::find_handles::<SimpleFileSystem>() else {
        return Some(entries);
    };
    let total_disks = handles.len();

    if !on_progress(ScanProgress {
        completed_disks: 0,
        total_disks,
    }) {
        return None;
    }

    for (index, handle) in handles.into_iter().enumerate() {
        let Ok(protocol) = boot::open_protocol_exclusive::<SimpleFileSystem>(handle) else {
            if !on_progress(ScanProgress {
                completed_disks: index + 1,
                total_disks,
            }) {
                return None;
            }
            continue;
        };
        let mut fs = FileSystem::new(protocol);
        scan_dir(
            &mut fs,
            PathBuf::from(cstr16!("\\")),
            handle,
            index + 1,
            0,
            &mut entries,
        );

        if !on_progress(ScanProgress {
            completed_disks: index + 1,
            total_disks,
        }) {
            return None;
        }
    }

    Some(entries)
}

fn scan_dir(
    fs: &mut FileSystem<'_>,
    dir: PathBuf,
    device: Handle,
    disk_index: usize,
    depth: usize,
    out: &mut Vec<BootEntry>,
) {
    let Ok(read_dir) = fs.read_dir(&dir) else {
        return;
    };

    let mut dirs = Vec::<String>::new();
    let mut files = Vec::<String>::new();

    for file_info in read_dir.filter_map(|entry| entry.ok()) {
        let name = String::from(file_info.file_name());
        if name.is_empty() || name == "." || name == ".." {
            continue;
        }
        if file_info.is_directory() {
            dirs.push(name);
        } else {
            files.push(name);
        }
    }

    let initrd = detect_initrd(&files).map(|name| {
        let mut path = dir.clone();
        let cstr = string_to_cstr16(&name);
        path.push(cstr.as_ref());
        path.to_string()
    });

    for file_name in &files {
        if !is_kernel_candidate(file_name) {
            continue;
        }

        let mut kernel_path = dir.clone();
        let file_name_cstr = string_to_cstr16(file_name);
        kernel_path.push(file_name_cstr.as_ref());

        out.push(BootEntry {
            title: format!("Disk {disk_index}: {}", kernel_path),
            subtitle: None,
            boot_timeout: None,
            is_default: false,
            entry_type: EntryType::Linux,
            device: Some(device),
            kernel: Some(kernel_path.to_string()),
            initrd: initrd.clone(),
            rootfs: None,
            cmdline: None,
        });
    }

    if depth >= MAX_SCAN_DEPTH {
        return;
    }

    for dir_name in dirs {
        if !should_descend(&dir_name, depth) {
            continue;
        }

        let mut child = dir.clone();
        let child_name = string_to_cstr16(&dir_name);
        child.push(child_name.as_ref());
        scan_dir(fs, child, device, disk_index, depth + 1, out);
    }
}

fn detect_initrd(files: &[String]) -> Option<String> {
    let mut best: Option<&String> = None;
    for file_name in files {
        let lower = file_name.to_ascii_lowercase();
        if !is_initrd_candidate(&lower) {
            continue;
        }

        match best {
            None => best = Some(file_name),
            Some(current) => {
                if lower.contains("initrd") && !current.to_ascii_lowercase().contains("initrd") {
                    best = Some(file_name);
                }
            }
        }
    }
    best.cloned()
}

fn should_descend(dir_name: &str, depth: usize) -> bool {
    if depth == 0 {
        return true;
    }

    let lower = dir_name.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "efi" | "boot" | "loader" | "linux" | "kernels" | "neoboot"
    ) || depth < 2
}

fn is_kernel_candidate(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    lower.ends_with(".efi")
        && !matches!(
            lower.as_str(),
            "bootx64.efi" | "neodata.efi" | "neoboot.efi" | "grubx64.efi" | "shimaa64.efi"
        )
        && (lower.contains("vmlinuz")
            || lower.contains("bzimage")
            || lower.contains("linux")
            || lower.contains("kernel"))
}

fn is_initrd_candidate(file_name: &str) -> bool {
    file_name.contains("initrd")
        || file_name.contains("initramfs")
        || file_name.ends_with(".cpio")
        || file_name.contains("rootfs")
}

fn string_to_cstr16(text: &str) -> uefi::CString16 {
    uefi::CString16::try_from(text).expect("UEFI paths must be UCS-2 compatible")
}

fn detect_windows_boot(fs: &mut FileSystem<'_>, device: Handle) -> Option<BootEntry> {
    let path = cstr16!("\\EFI\\Microsoft\\Boot\\bootmgfw.efi");
    if fs.read(Path::new(path)).is_err() {
        return None;
    }

    Some(BootEntry {
        title: "Windows NT 10.0".into(),
        subtitle: Some("/EFI/Microsoft/Boot".into()),
        boot_timeout: None,
        is_default: false,
        entry_type: EntryType::Linux,
        device: Some(device),
        kernel: Some("\\EFI\\Microsoft\\Boot\\bootmgfw.efi".into()),
        initrd: None,
        rootfs: None,
        cmdline: None,
    })
}

fn make_firmware_volume_entry(mut entry: BootEntry) -> BootEntry {
    let kernel = entry.kernel.as_deref().unwrap_or_default();
    entry.title = guess_linux_volume_title(kernel);
    entry.subtitle = Some(display_path(kernel));
    entry
}

fn guess_linux_volume_title(kernel_path: &str) -> String {
    let lower = kernel_path.to_ascii_lowercase();
    if lower.contains("gentoo") {
        "Gentoo Linux (Custom kernel)".into()
    } else if lower.contains("ubuntu") {
        "Ubuntu Linux (Custom kernel)".into()
    } else if lower.contains("arch") {
        "Arch Linux (Custom kernel)".into()
    } else {
        "Linux (Custom kernel)".into()
    }
}

fn display_path(path: &str) -> String {
    let mut display = String::with_capacity(path.len());
    if !path.starts_with('\\') && !path.starts_with('/') {
        display.push('/');
    }
    for ch in path.chars() {
        display.push(if ch == '\\' { '/' } else { ch });
    }
    display
}

fn scan_boot_target_entries(fs: &mut FileSystem<'_>, device: Handle, out: &mut Vec<BootEntry>) {
    for root_dir in ["\\BOOT_TARGE", "\\BOOT_TARGET"] {
        let root_cstr = string_to_cstr16(root_dir);
        let Ok(target_dirs) = fs.read_dir(Path::new(root_cstr.as_ref())) else {
            continue;
        };

        for file_info in target_dirs.filter_map(|entry| entry.ok()) {
            if !file_info.is_directory() {
                continue;
            }

            let dir_name = String::from(file_info.file_name());
            if dir_name.is_empty() || dir_name == "." || dir_name == ".." {
                continue;
            }

            if let Some(entry) = read_boot_target_entry(fs, device, root_dir, &dir_name) {
                push_unique_entry(out, entry);
            }
        }
    }
}

fn read_boot_target_entry(
    fs: &mut FileSystem<'_>,
    device: Handle,
    root_dir: &str,
    dir_name: &str,
) -> Option<BootEntry> {
    let config = read_boot_target_config(fs, root_dir, dir_name)?;
    let kernel = config.kernel_path?;

    let mut cmdline_parts = Vec::<String>::new();
    if let Some(init_path) = config.init_path.as_deref() {
        cmdline_parts.push(format!("init={init_path}"));
    }
    if let Some(boot_path) = config.boot_path.as_deref() {
        cmdline_parts.push(format!("BOOT_PATH={boot_path}"));
    }
    if let Some(disk_size) = config.disk_size.as_deref() {
        cmdline_parts.push(format!("DISK_SIZE={disk_size}"));
    }
    if let Some(log_level) = config.log_level.as_deref() {
        cmdline_parts.push(format!("loglevel={log_level}"));
    }
    if let Some(other_parameters) = config.other_parameters.as_deref() {
        cmdline_parts.push(other_parameters.to_string());
    }

    let boot_path_display = config
        .boot_path
        .as_deref()
        .map(display_path)
        .or_else(|| config.initrd_path.as_deref().map(display_path))
        .or_else(|| Some(display_path(&kernel)));

    let title = match config.title {
        Some(title) if !title.is_empty() => title,
        _ => boot_path_display
            .clone()
            .unwrap_or_else(|| display_path(&kernel)),
    };

    Some(BootEntry {
        title,
        subtitle: boot_path_display,
        boot_timeout: config.timeout,
        is_default: config.default_selected,
        entry_type: EntryType::Linux,
        device: Some(device),
        kernel: Some(kernel),
        initrd: config.initrd_path,
        rootfs: None,
        cmdline: if cmdline_parts.is_empty() {
            None
        } else {
            Some(cmdline_parts.join(" "))
        },
    })
}

fn read_boot_target_config(
    fs: &mut FileSystem<'_>,
    root_dir: &str,
    dir_name: &str,
) -> Option<BootTargetConfig> {
    for file_name in ["boot-prop.conf", "boot-prop.conf.example"] {
        let root_cstr = string_to_cstr16(root_dir);
        let mut path = PathBuf::from(root_cstr.as_ref());
        let dir_cstr = string_to_cstr16(dir_name);
        let file_cstr = string_to_cstr16(file_name);
        path.push(dir_cstr.as_ref());
        path.push(file_cstr.as_ref());

        let path_string = path.to_string();
        let path_cstr = string_to_cstr16(&path_string);
        let Ok(bytes) = fs.read(Path::new(path_cstr.as_ref())) else {
            continue;
        };
        let Ok(text) = core::str::from_utf8(&bytes) else {
            continue;
        };
        return Some(parse_boot_target_config(text));
    }

    None
}

fn parse_boot_target_config(text: &str) -> BootTargetConfig {
    let mut config = BootTargetConfig::default();

    for raw_line in text.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if value.is_empty() {
            continue;
        }

        match key {
            "BOOT_PATH" => config.boot_path = Some(value.to_string()),
            "INIT_PATH" => config.init_path = Some(value.to_string()),
            "KERNEL_PATH" => config.kernel_path = Some(value.to_string()),
            "TITLE" => config.title = Some(value.to_string()),
            "DISK_SIZE" => config.disk_size = Some(value.to_string()),
            "TIMEOUT" => config.timeout = Some(parse_usize(value)),
            "DEFAULT" => config.default_selected = value.eq_ignore_ascii_case("true"),
            "INITRD_PATH" => config.initrd_path = Some(value.to_string()),
            "LOG_LEVEL" => config.log_level = Some(value.to_string()),
            "OTHER_PARAMETERS" => config.other_parameters = Some(value.to_string()),
            _ => {}
        }
    }

    config
}

fn push_unique_entry(entries: &mut Vec<BootEntry>, entry: BootEntry) {
    let exists = entries
        .iter()
        .any(|existing| existing.device == entry.device && existing.kernel == entry.kernel);
    if !exists {
        entries.push(entry);
    }
}

fn parse_usize(value: &str) -> usize {
    let mut n = 0usize;
    for b in value.bytes() {
        if !b.is_ascii_digit() {
            break;
        }
        n = n * 10 + usize::from(b - b'0');
    }
    n
}

#[derive(Default)]
struct BootTargetConfig {
    boot_path: Option<String>,
    init_path: Option<String>,
    kernel_path: Option<String>,
    title: Option<String>,
    disk_size: Option<String>,
    timeout: Option<usize>,
    default_selected: bool,
    initrd_path: Option<String>,
    log_level: Option<String>,
    other_parameters: Option<String>,
}
