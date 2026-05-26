#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;
use alloc::vec::Vec;
use uefi::prelude::*;
use uefi::proto::console::gop::GraphicsOutput;

const VERSION: &str = "0.1.0-beta";
const INPUT_PROMPT: &str =
    "Please connect an input device, such as a controller or keyboard, to enter the boot selection menu.";

mod config;
mod font;
mod fs;
mod icons;
mod input;
mod launcher;
mod ui;
pub mod usb;

use config::{Config, EntryType};
use font::FontRenderer;
use input::{Input, InputEvent, InputSource};
use ui::UI;

#[entry]
fn main(image: Handle, _st: SystemTable<Boot>) -> Status {
    uefi::helpers::init().unwrap();

    // Force enumeration of all lazy-loaded USB/PCI controllers and devices
    usb::connect_all_controllers();

    // Load config
    let config = Config::load(image).unwrap_or_else(|_| Config::default());

    // Initialize graphics
    let gop_handle = uefi::boot::get_handle_for_protocol::<GraphicsOutput>().unwrap();
    let mut gop = uefi::boot::open_protocol_exclusive::<GraphicsOutput>(gop_handle).unwrap();

    let mode = gop.current_mode_info();
    let (width, height) = mode.resolution();
    let mut framebuffer = gop.frame_buffer();

    let mut ui = UI::new(width, height, framebuffer.as_mut_ptr());
    let mut input = Input::new();
    let mut base_entries = config.entries.clone();
    if let Ok(device) = fs::image_device(image) {
        bind_entries_to_device(&mut base_entries, device);
    }
    let mut entries = base_entries.clone();
    let mut selected = config.default.min(entries.len().saturating_sub(1));
    let mut dirty = true;
    let mut countdown = config.timeout;
    let mut ticks = 0usize;
    let mut waiting_for_input = true;
    let mut footer_source = resolve_footer_source(config.controller_style, input.source());

    // Load font
    let font_path = config
        .font
        .as_deref()
        .unwrap_or("assets/NotoSans-Regular.ttf");
    let font = fs::read_file(image, font_path)
        .ok()
        .or_else(|| {
            Some(Vec::from(
                include_bytes!("../assets/NotoSans-Regular.ttf").as_slice(),
            ))
        })
        .and_then(FontRenderer::new);

    loop {
        let next_footer_source = resolve_footer_source(config.controller_style, input.source());
        if next_footer_source != footer_source {
            footer_source = next_footer_source;
            dirty = true;
        }

        if dirty || waiting_for_input {
            ui.draw_background();
            if waiting_for_input {
                ui.draw_title("NeoBoot", font.as_ref());
                ui.draw_status(INPUT_PROMPT, 180, font.as_ref());
                ui.draw_status(
                    "Press any key or controller button to continue.",
                    220,
                    font.as_ref(),
                );

                // Render diagnostic logs in real-time
                let logs = input::get_debug_logs();
                let mut y_log = 260;
                for log_line in &logs {
                    ui.draw_status(log_line, y_log, font.as_ref());
                    y_log += 22;
                }

                ui.flush();
                dirty = false;
            } else {
                ui.draw_title("Select Boot Options", font.as_ref());
                if config.show_timeout && countdown > 0 {
                    ui.draw_status(
                        &format!("NeoBoot Beta {VERSION} - Auto boot in {countdown}s"),
                        120,
                        font.as_ref(),
                    );
                } else {
                    ui.draw_status(&format!("NeoBoot Beta {VERSION}"), 120, font.as_ref());
                }
                ui.draw_menu(&entries, selected, font.as_ref());
                ui.draw_footer(font.as_ref(), footer_source);
                ui.flush();
                dirty = false;
            }
        }

        match input.poll() {
            Some(InputEvent::Up) => {
                if waiting_for_input {
                    waiting_for_input = false;
                    dirty = true;
                } else {
                    countdown = 0;
                    if selected > 0 {
                        selected -= 1;
                        dirty = true;
                    }
                }
            }
            Some(InputEvent::Down) => {
                if waiting_for_input {
                    waiting_for_input = false;
                    dirty = true;
                } else {
                    countdown = 0;
                    if selected + 1 < entries.len() {
                        selected += 1;
                        dirty = true;
                    }
                }
            }
            Some(InputEvent::Select) => {
                if waiting_for_input {
                    waiting_for_input = false;
                    dirty = true;
                } else {
                    countdown = 0;
                    let entry = &entries[selected];
                    match entry.entry_type {
                        EntryType::Linux => {
                            if let Err(err) = launcher::boot_linux(image, entry) {
                                log::error!("failed to boot {:?}: {:?}", entry.title, err.status());
                            }
                        }
                        EntryType::FirmwareVolumes => {
                            if let Some(entry) = browse_firmware_volumes(
                                &mut ui,
                                &mut input,
                                font.as_ref(),
                                config.controller_style,
                            ) {
                                if let Err(err) = launcher::boot_linux(image, &entry) {
                                    log::error!(
                                        "failed to boot {:?}: {:?}",
                                        entry.title,
                                        err.status()
                                    );
                                }
                            }
                            dirty = true;
                        }
                        EntryType::ScanDisk => {
                            if let Some(scanned) = scan_disk_with_ui(
                                &mut ui,
                                &mut input,
                                font.as_ref(),
                                config.controller_style,
                            ) {
                                let (next_entries, next_selected) =
                                    merge_scanned_entries(&base_entries, scanned);
                                entries = next_entries;
                                selected = next_selected.min(entries.len().saturating_sub(1));
                            }
                            dirty = true;
                        }
                        EntryType::Reboot => {
                            uefi::runtime::reset(
                                uefi::table::runtime::ResetType::COLD,
                                Status::SUCCESS,
                                None,
                            );
                        }
                        EntryType::Shutdown => {
                            uefi::runtime::reset(
                                uefi::table::runtime::ResetType::SHUTDOWN,
                                Status::SUCCESS,
                                None,
                            );
                        }
                        _ => {}
                    }
                }
            }
            Some(InputEvent::Back) => {
                if waiting_for_input {
                    waiting_for_input = false;
                    dirty = true;
                } else {
                    countdown = 0;
                }
            }
            None => {}
        }

        ticks += 1;
        if !waiting_for_input && countdown > 0 && ticks >= 63 {
            ticks = 0;
            countdown -= 1;
            dirty = true;
            if countdown == 0 {
                let entry = &entries[selected];
                if entry.entry_type == EntryType::Linux {
                    if let Err(err) = launcher::boot_linux(image, entry) {
                        log::error!("failed to boot {:?}: {:?}", entry.title, err.status());
                    }
                }
            }
        }

        uefi::boot::stall(16_000); // ~60fps
    }
}

fn bind_entries_to_device(entries: &mut [config::BootEntry], device: Handle) {
    for entry in entries {
        if entry.entry_type == EntryType::Linux && entry.device.is_none() {
            entry.device = Some(device);
        }
    }
}

fn scan_disk_with_ui(
    ui: &mut UI,
    input: &mut Input,
    font: Option<&FontRenderer>,
    controller_style: config::ControllerStyle,
) -> Option<Vec<config::BootEntry>> {
    fs::scan_linux_entries_with_progress(|progress| {
        while let Some(event) = input.poll() {
            if matches!(event, InputEvent::Back) {
                return false;
            }
        }

        let footer_source = resolve_footer_source(controller_style, input.source());
        ui.draw_background();
        ui.draw_title("Select Boot Options", font);
        ui.draw_scan_screen(
            font,
            progress.completed_disks,
            progress.total_disks,
            footer_source,
        );
        ui.flush();
        true
    })
}

fn browse_firmware_volumes(
    ui: &mut UI,
    input: &mut Input,
    font: Option<&FontRenderer>,
    controller_style: config::ControllerStyle,
) -> Option<config::BootEntry> {
    let entries = fs::scan_firmware_boot_entries_with_progress(|progress| {
        while let Some(event) = input.poll() {
            if matches!(event, InputEvent::Back) {
                return false;
            }
        }

        let footer_source = resolve_footer_source(controller_style, input.source());
        ui.draw_background();
        ui.draw_title("Select Boot Options", font);
        ui.draw_scan_screen(
            font,
            progress.completed_disks,
            progress.total_disks,
            footer_source,
        );
        ui.flush();
        true
    })?;

    let mut selected = entries
        .iter()
        .position(|entry| entry.is_default)
        .unwrap_or(0);
    if entries
        .get(selected)
        .is_some_and(|entry| entry.is_default && entry.boot_timeout == Some(0))
    {
        return entries.get(selected).cloned();
    }
    let mut countdown = entries
        .get(selected)
        .and_then(|entry| entry.boot_timeout)
        .unwrap_or(0);
    let mut ticks = 0usize;
    let mut dirty = true;

    loop {
        if dirty {
            let footer_source = resolve_footer_source(controller_style, input.source());
            ui.draw_background();
            ui.draw_title("Select Boot Options", font);
            ui.draw_volume_browser(&entries, selected, font, footer_source);
            ui.flush();
            dirty = false;
        }

        match input.poll() {
            Some(InputEvent::Up) => {
                if selected > 0 {
                    selected -= 1;
                    countdown = 0;
                    dirty = true;
                }
            }
            Some(InputEvent::Down) => {
                if selected + 1 < entries.len() {
                    selected += 1;
                    countdown = 0;
                    dirty = true;
                }
            }
            Some(InputEvent::Select) => return entries.get(selected).cloned(),
            Some(InputEvent::Back) => return None,
            None => {}
        }

        ticks += 1;
        if countdown > 0 && ticks >= 63 {
            ticks = 0;
            countdown -= 1;
            if countdown == 0 {
                return entries.get(selected).cloned();
            }
        }

        uefi::boot::stall(16_000);
    }
}

fn merge_scanned_entries(
    base_entries: &[config::BootEntry],
    scanned_entries: Vec<config::BootEntry>,
) -> (Vec<config::BootEntry>, usize) {
    let split_index = base_entries
        .iter()
        .position(|entry| entry.entry_type != EntryType::Linux)
        .unwrap_or(base_entries.len());

    let mut entries = Vec::with_capacity(base_entries.len() + scanned_entries.len());
    entries.extend_from_slice(&base_entries[..split_index]);

    let scanned_start = entries.len();
    for entry in scanned_entries {
        if !entries
            .iter()
            .any(|existing| same_boot_target(existing, &entry))
        {
            entries.push(entry);
        }
    }
    let has_scanned = entries.len() > scanned_start;
    entries.extend_from_slice(&base_entries[split_index..]);

    (
        entries,
        if has_scanned {
            scanned_start
        } else {
            split_index.min(base_entries.len().saturating_sub(1))
        },
    )
}

fn same_boot_target(left: &config::BootEntry, right: &config::BootEntry) -> bool {
    left.entry_type == EntryType::Linux
        && right.entry_type == EntryType::Linux
        && left.device == right.device
        && left.kernel == right.kernel
}

fn resolve_footer_source(
    _controller_style: config::ControllerStyle,
    input_source: InputSource,
) -> InputSource {
    input_source
}
