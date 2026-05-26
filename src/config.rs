extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use uefi::{Handle, Result, Status};

use crate::fs;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ControllerStyle {
    Auto,
    DualSense,
    Xbox,
    Both,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EntryType {
    Linux,
    FirmwareVolumes,
    RebootMenu,
    InstallKernel,
    ScanDisk,
    Reboot,
    Shutdown,
}

#[derive(Clone)]
pub struct BootEntry {
    pub title: String,
    pub subtitle: Option<String>,
    pub boot_timeout: Option<usize>,
    pub is_default: bool,
    pub entry_type: EntryType,
    pub device: Option<Handle>,
    pub kernel: Option<String>,
    pub initrd: Option<String>,
    pub rootfs: Option<String>,
    pub cmdline: Option<String>,
}

pub struct Config {
    pub default: usize,
    pub timeout: usize,
    pub font: Option<String>,
    pub entries: Vec<BootEntry>,
    pub show_timeout: bool,
    pub controller_style: ControllerStyle,
}

impl Config {
    pub fn load(image: Handle) -> Result<Self> {
        let bytes = fs::read_file(image, "boot.cfg")?;
        let text = core::str::from_utf8(&bytes).map_err(|_| Status::LOAD_ERROR)?;
        Ok(Self::parse(text))
    }

    pub fn parse(text: &str) -> Self {
        let mut config = Self {
            default: 0,
            timeout: 0,
            font: None,
            entries: Vec::new(),
            show_timeout: true,
            controller_style: ControllerStyle::Auto,
        };
        let mut current: Option<BootEntry> = None;

        for raw_line in text.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if line == "[global]" {
                if let Some(entry) = current.take() {
                    config.entries.push(entry);
                }
                continue;
            }

            if let Some(title) = line
                .strip_prefix("[entry \"")
                .and_then(|s| s.strip_suffix("\"]"))
            {
                if let Some(entry) = current.take() {
                    config.entries.push(entry);
                }
                current = Some(BootEntry {
                    title: title.to_string(),
                    subtitle: None,
                    boot_timeout: None,
                    is_default: false,
                    entry_type: EntryType::Linux,
                    device: None,
                    kernel: None,
                    initrd: None,
                    rootfs: None,
                    cmdline: None,
                });
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();

            if let Some(entry) = current.as_mut() {
                match key {
                    "type" => entry.entry_type = parse_entry_type(value),
                    "kernel" => entry.kernel = Some(value.to_string()),
                    "initrd" => entry.initrd = Some(value.to_string()),
                    "root" | "rootfs" => entry.rootfs = Some(value.to_string()),
                    "cmdline" => entry.cmdline = Some(value.to_string()),
                    _ => {}
                }
            } else {
                match key {
                    "default" => config.default = parse_usize(value),
                    "timeout" => config.timeout = parse_usize(value),
                    "show_timeout" => config.show_timeout = value == "true" || value == "1",
                    "font" => config.font = Some(value.to_string()),
                    "controller_style" => config.controller_style = parse_controller_style(value),
                    _ => {}
                }
            }
        }

        if let Some(entry) = current.take() {
            config.entries.push(entry);
        }
        if config.entries.is_empty() {
            return Self::default();
        }
        if config.default >= config.entries.len() {
            config.default = 0;
        }
        config
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default: 0,
            timeout: 0,
            font: Some("assets/NotoSans-Regular.ttf".to_string()),
            show_timeout: true,
            controller_style: ControllerStyle::Auto,
            entries: Vec::from([
                BootEntry {
                    title: "Restart to Other OS Boot".to_string(),
                    subtitle: None,
                    boot_timeout: None,
                    is_default: false,
                    entry_type: EntryType::FirmwareVolumes,
                    device: None,
                    kernel: None,
                    initrd: None,
                    rootfs: None,
                    cmdline: None,
                },
                BootEntry {
                    title: "Restart to Select Boot Options".to_string(),
                    subtitle: None,
                    boot_timeout: None,
                    is_default: false,
                    entry_type: EntryType::RebootMenu,
                    device: None,
                    kernel: None,
                    initrd: None,
                    rootfs: None,
                    cmdline: None,
                },
                BootEntry {
                    title: "Install kernel".to_string(),
                    subtitle: None,
                    boot_timeout: None,
                    is_default: false,
                    entry_type: EntryType::InstallKernel,
                    device: None,
                    kernel: None,
                    initrd: None,
                    rootfs: None,
                    cmdline: None,
                },
                BootEntry {
                    title: "Scanning disk".to_string(),
                    subtitle: None,
                    boot_timeout: None,
                    is_default: false,
                    entry_type: EntryType::ScanDisk,
                    device: None,
                    kernel: None,
                    initrd: None,
                    rootfs: None,
                    cmdline: None,
                },
                BootEntry {
                    title: "Restart system".to_string(),
                    subtitle: None,
                    boot_timeout: None,
                    is_default: false,
                    entry_type: EntryType::Reboot,
                    device: None,
                    kernel: None,
                    initrd: None,
                    rootfs: None,
                    cmdline: None,
                },
                BootEntry {
                    title: "Shutdown device".to_string(),
                    subtitle: None,
                    boot_timeout: None,
                    is_default: false,
                    entry_type: EntryType::Shutdown,
                    device: None,
                    kernel: None,
                    initrd: None,
                    rootfs: None,
                    cmdline: None,
                },
            ]),
        }
    }
}

fn parse_entry_type(value: &str) -> EntryType {
    match value {
        "firmware-volumes" => EntryType::FirmwareVolumes,
        "reboot-menu" => EntryType::RebootMenu,
        "install-kernel" => EntryType::InstallKernel,
        "scan-disk" => EntryType::ScanDisk,
        "reboot" => EntryType::Reboot,
        "shutdown" => EntryType::Shutdown,
        _ => EntryType::Linux,
    }
}

fn parse_controller_style(value: &str) -> ControllerStyle {
    match value {
        "dualsense" | "playstation" | "dualshock4" => ControllerStyle::DualSense,
        "xbox" | "vader4" | "vader4pro" | "vaper4pro" => ControllerStyle::Xbox,
        "both" => ControllerStyle::Both,
        _ => ControllerStyle::Auto,
    }
}

fn parse_usize(value: &str) -> usize {
    let mut n = 0;
    for b in value.bytes() {
        if !b.is_ascii_digit() {
            break;
        }
        n = n * 10 + usize::from(b - b'0');
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_boot_cfg_entries() {
        let cfg = Config::parse(include_str!("../boot.cfg"));
        assert_eq!(cfg.entries.len(), 6);
        assert_eq!(cfg.entries[0].title, "Restart to Other OS Boot");
        assert_eq!(cfg.entries[0].entry_type, EntryType::FirmwareVolumes);
        assert_eq!(cfg.entries[5].entry_type, EntryType::Shutdown);
    }
}
