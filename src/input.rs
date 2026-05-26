extern crate alloc;

use alloc::vec::Vec;
use uefi::proto::console::text::Key;
use uefi::{Handle, Status};

use crate::usb::{self, ControllerKind, UsbIo};

static mut DEBUG_LOGS: Option<alloc::vec::Vec<alloc::string::String>> = None;

pub fn add_debug_log(msg: alloc::string::String) {
    unsafe {
        let logs_ptr = core::ptr::addr_of_mut!(DEBUG_LOGS);
        if (*logs_ptr).is_none() {
            *logs_ptr = Some(alloc::vec::Vec::new());
        }
        if let Some(ref mut logs) = *logs_ptr {
            if logs.len() >= 18 {
                logs.remove(0);
            }
            logs.push(msg);
        }
    }
}

pub fn get_debug_logs() -> alloc::vec::Vec<alloc::string::String> {
    unsafe {
        let logs_ptr = core::ptr::addr_of_mut!(DEBUG_LOGS);
        (*logs_ptr).clone().unwrap_or_default()
    }
}

#[macro_export]
macro_rules! dbg_log {
    ($($arg:tt)*) => {
        $crate::input::add_debug_log(alloc::format!($($arg)*))
    };
}

const INPUT_UP: u8 = 1;
const INPUT_DOWN: u8 = 1 << 1;
const INPUT_SELECT: u8 = 1 << 2;
const INPUT_BACK: u8 = 1 << 3;
const RESCAN_INTERVAL_POLLS: u8 = 60;
const USB_POLL_TIMEOUT_MS: usize = 30;

const XBOXONE_POWER_ON: [u8; 5] = [0x05, 0x20, 0x00, 0x01, 0x00];
const XBOXONE_S_INIT: [u8; 5] = [0x05, 0x20, 0x00, 0x0f, 0x06];
/// GIP LED command: unknown=0x00, mode=0x01 (on), brightness=0x28 (max)
const XBOXONE_LED_ON: [u8; 7] = [0x0a, 0x20, 0x00, 0x03, 0x00, 0x01, 0x28];
/// GIP authentication done: tells controller auth succeeded
const XBOXONE_AUTH_DONE: [u8; 6] = [0x06, 0x20, 0x00, 0x02, 0x01, 0x00];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputSource {
    None,
    Keyboard,
    PlayStation,
    Xbox,
    Both,
}

pub enum InputEvent {
    Up,
    Down,
    Select,
    Back,
}

pub struct Input {
    controller: ControllerInput,
    source: InputSource,
}

impl Input {
    pub fn new() -> Self {
        Self {
            controller: ControllerInput::new(),
            source: InputSource::None,
        }
    }

    pub fn poll(&mut self) -> Option<InputEvent> {
        if let Some(event) = self.poll_keyboard() {
            self.source = InputSource::Keyboard;
            return Some(event);
        }

        let event = self.controller.poll();
        if event.is_some() {
            self.source = self.controller.source();
        } else if self.source != InputSource::Keyboard {
            self.source = self.controller.source();
        }
        event
    }

    pub fn source(&self) -> InputSource {
        if self.source == InputSource::Keyboard {
            InputSource::Keyboard
        } else {
            self.controller.source()
        }
    }

    fn poll_keyboard(&mut self) -> Option<InputEvent> {
        uefi::system::with_stdin(|stdin| {
            if let Ok(Some(key)) = stdin.read_key() {
                match key {
                    Key::Special(uefi::proto::console::text::ScanCode::UP) => {
                        return Some(InputEvent::Up)
                    }
                    Key::Special(uefi::proto::console::text::ScanCode::DOWN) => {
                        return Some(InputEvent::Down)
                    }
                    Key::Special(uefi::proto::console::text::ScanCode::ESCAPE) => {
                        return Some(InputEvent::Back)
                    }
                    Key::Printable(c) => match char::from(c).to_ascii_lowercase() {
                        '\r' | '\n' | ' ' | 'a' | 'x' => return Some(InputEvent::Select),
                        '\u{8}' => return Some(InputEvent::Back),
                        'w' | 'k' => return Some(InputEvent::Up),
                        's' | 'j' => return Some(InputEvent::Down),
                        _ => {}
                    },
                    _ => {}
                }
            }
            None
        })
    }
}

struct ControllerInput {
    usb_handles: Vec<Handle>,
    active: Option<ActiveController>,
    rescan_counter: u8,
    last_state: u8,
    scan_attempts: u8,
}

struct ActiveController {
    usb_io: UsbIo,
    kind: ControllerKind,
    in_endpoint: u8,
    out_endpoint: Option<u8>,
    product_id: u16,
    initialized: bool,
    sequence: u8,
    buffer: [u8; 64],
    subclass: u8,
    interface_number: u8,
    /// Track consecutive interrupt transfer failures to trigger GET_REPORT fallback
    interrupt_failures: u8,
}

impl ControllerInput {
    fn new() -> Self {
        Self {
            usb_handles: Vec::new(),
            active: None,
            rescan_counter: 0,
            last_state: 0,
            scan_attempts: 0,
        }
    }

    fn poll(&mut self) -> Option<InputEvent> {
        self.ensure_controller();

        let Some(controller) = self.active.as_mut() else {
            return None;
        };

        if !controller.initialized && controller.initialize().is_err() {
            self.active = None;
            self.last_state = 0;
            return None;
        }

        match controller.read_state() {
            Ok(state) => self.emit_edge(state),
            Err(status) => {
                if status != Status::TIMEOUT
                    && status != Status::NOT_READY
                    && status != Status::DEVICE_ERROR
                {
                    self.active = None;
                    self.last_state = 0;
                }
                None
            }
        }
    }

    fn ensure_controller(&mut self) {
        if self.active.is_some() {
            return;
        }

        if self.rescan_counter > 0 {
            self.rescan_counter -= 1;
            return;
        }
        // Progressive back-off: scan quickly at first, then slow down
        self.rescan_counter = if self.scan_attempts < 5 {
            15
        } else {
            RESCAN_INTERVAL_POLLS
        };
        self.scan_attempts = self.scan_attempts.saturating_add(1);

        // If we haven't found handles before, re-run connect_all_controllers
        // to give USB bus drivers another chance to enumerate
        if self.usb_handles.is_empty() && self.scan_attempts <= 5 {
            dbg_log!(
                "Re-connecting controllers (attempt {})...",
                self.scan_attempts
            );
            usb::connect_all_controllers();
        }

        dbg_log!("Rescanning USB devices...");
        let handles = match usb::find_usb_io_handles() {
            Ok(h) => {
                dbg_log!("Found {} USB handle(s)", h.len());
                h
            }
            Err(e) => {
                dbg_log!("USB handle search failed: {:?}", e.status());
                Vec::new()
            }
        };
        self.usb_handles = handles;

        for &handle in &self.usb_handles {
            // --- Phase 1: Probe the device to classify it ---
            dbg_log!("Probing handle {:?}", handle);
            let usb_io = match unsafe { UsbIo::from_open_protocol(handle) } {
                Ok(io) => io,
                Err(e) => {
                    dbg_log!("  Failed to open protocol: {:?}", e.status());
                    continue;
                }
            };
            let desc = match usb_io.get_device_descriptor() {
                Ok(d) => d,
                Err(e) => {
                    dbg_log!("  Failed to get device desc: {:?}", e.status());
                    continue;
                }
            };

            dbg_log!(
                "  Device: VID={:04x} PID={:04x}",
                desc.vendor_id,
                desc.product_id
            );

            let mut subclass = 0u8;
            let mut interface_number = 0u8;
            let mut interface_class = 0u8;
            let mut interface_protocol = 0u8;

            if let Ok(interface) = usb_io.get_interface_descriptor() {
                subclass = interface.interface_subclass;
                interface_number = interface.interface_number;
                interface_class = interface.interface_class;
                interface_protocol = interface.interface_protocol;
                dbg_log!(
                    "  Interface: num={} class={:02x} sub={:02x} proto={:02x}",
                    interface_number,
                    interface_class,
                    subclass,
                    interface_protocol
                );
            } else {
                dbg_log!("  Failed to get interface descriptor");
            }

            // Classify with class, subclass, and protocol checks
            let kind = usb::classify_controller(
                desc.vendor_id,
                desc.product_id,
                interface_class,
                subclass,
                interface_protocol,
            );

            if kind == ControllerKind::Unknown {
                dbg_log!("  Skipping non-gamepad interface");
                continue;
            }

            dbg_log!("  Classified as {:?}", kind);

            // Must have an interrupt IN endpoint
            let in_endpoint = match usb_io.interrupt_in_endpoint() {
                Ok(Some(ep)) => ep,
                Ok(None) => {
                    dbg_log!("  No Interrupt IN endpoint found");
                    continue;
                }
                Err(e) => {
                    dbg_log!("  Failed seeking IN endpoint: {:?}", e.status());
                    continue;
                }
            };
            let out_endpoint = usb_io.interrupt_out_endpoint().ok().flatten();
            dbg_log!("  Endpoints: IN={:02x} OUT={:?}", in_endpoint, out_endpoint);

            // --- Phase 2: Evict UEFI drivers and claim the device ---
            // Drop the old UsbIo reference (it's just a raw pointer, no cleanup)
            drop(usb_io);

            // Disconnect ALL existing UEFI drivers (HID, keyboard, etc.)
            // from this handle. This releases any async interrupt transfers
            // that lock the interrupt pipe.
            dbg_log!("  Evicting existing UEFI drivers on handle {:?}", handle);
            match usb::disconnect_existing_drivers(handle) {
                Ok(_) => dbg_log!("  Eviction returned SUCCESS"),
                Err(e) => dbg_log!("  Eviction returned: {:?}", e.status()),
            }

            // Small delay for driver cleanup to complete
            uefi::boot::stall(10_000); // 10ms

            // --- Phase 3: Re-open protocol with a fresh pointer exclusively ---
            dbg_log!("  Re-opening protocol exclusively...");
            let usb_io = match unsafe { UsbIo::from_open_protocol_exclusive(handle) } {
                Ok(io) => io,
                Err(e) => {
                    dbg_log!("  Re-open failed: {:?}", e.status());
                    continue;
                }
            };

            // Cancel any lingering async interrupt transfers on ALL endpoints
            dbg_log!("  Canceling async transfers...");
            if let Err(e) = usb_io.cancel_async_interrupt(in_endpoint) {
                dbg_log!("  Cancel IN transfer returned: {:?}", e.status());
            }
            if let Some(out_ep) = out_endpoint {
                if let Err(e) = usb_io.cancel_async_interrupt(out_ep) {
                    dbg_log!("  Cancel OUT transfer returned: {:?}", e.status());
                }
            }

            // --- Phase 4: SET_CONFIGURATION to re-activate the device ---
            // After disconnecting UEFI drivers, the device may revert to the Address state.
            // We must send SET_CONFIGURATION to put it back into the Configured state
            // so that endpoints become active and the device starts streaming data.
            // DualSense streams immediately after SET_CONFIGURATION.
            // Xbox controllers need SET_CONFIGURATION before GIP activation packets work.
            dbg_log!("  Sending SET_CONFIGURATION...");
            match usb_io.get_config_descriptor() {
                Ok(config) => {
                    let config_val = config.configuration_value;
                    dbg_log!(
                        "  Config value={}, num_interfaces={}",
                        config_val,
                        config.num_interfaces
                    );
                    match usb_io.set_configuration(config_val) {
                        Ok(_) => dbg_log!("  SET_CONFIGURATION({}) OK", config_val),
                        Err(e) => dbg_log!("  SET_CONFIGURATION failed: {:?}", e.status()),
                    }
                }
                Err(e) => {
                    dbg_log!(
                        "  get_config_descriptor failed: {:?}. Trying config=1...",
                        e.status()
                    );
                    match usb_io.set_configuration(1) {
                        Ok(_) => dbg_log!("  SET_CONFIGURATION(1) OK"),
                        Err(e2) => dbg_log!("  SET_CONFIGURATION(1) failed: {:?}", e2.status()),
                    }
                }
            }

            // Small delay for configuration to take effect
            uefi::boot::stall(50_000); // 50ms

            dbg_log!("  Device CLAIMED successfully!");
            self.active = Some(ActiveController {
                usb_io,
                kind,
                in_endpoint,
                out_endpoint,
                product_id: desc.product_id,
                initialized: false,
                sequence: 0,
                buffer: [0; 64],
                subclass,
                interface_number,
                interrupt_failures: 0,
            });
            break;
        }
    }

    fn emit_edge(&mut self, state: u8) -> Option<InputEvent> {
        let pressed = state & !self.last_state;
        self.last_state = state;
        if pressed & INPUT_UP != 0 {
            return Some(InputEvent::Up);
        }
        if pressed & INPUT_DOWN != 0 {
            return Some(InputEvent::Down);
        }
        if pressed & INPUT_SELECT != 0 {
            return Some(InputEvent::Select);
        }
        if pressed & INPUT_BACK != 0 {
            return Some(InputEvent::Back);
        }
        None
    }

    fn source(&self) -> InputSource {
        let Some(controller) = self.active.as_ref() else {
            return InputSource::None;
        };

        match controller.kind {
            ControllerKind::DualSense | ControllerKind::DualShock4 => InputSource::PlayStation,
            ControllerKind::Xbox => InputSource::Xbox,
            ControllerKind::Unknown => InputSource::None,
        }
    }
}

impl ActiveController {
    fn initialize(&mut self) -> Result<(), Status> {
        if self.initialized {
            return Ok(());
        }

        dbg_log!("Initializing {:?} controller...", self.kind);

        match self.kind {
            ControllerKind::Xbox => {
                if self.subclass == 0x47 || self.subclass == 0x5D {
                    if let Some(endpoint) = self.out_endpoint {
                        dbg_log!("  Sending Xbox GIP activation sequence...");
                        // Step 1: Power on the controller
                        let _ = self.send_xbox_packet(endpoint, &XBOXONE_POWER_ON);
                        let _ = self.send_xbox_packet(endpoint, &XBOXONE_S_INIT);
                        // Step 2: Wait for controller to wake up
                        uefi::boot::stall(50_000); // 50ms

                        // Step 3: Auth done FIRST — controller must see auth before LED
                        // Without this, 3rd party controllers show red (auth failed) indicator
                        let _ = self.send_xbox_packet(endpoint, &XBOXONE_AUTH_DONE);
                        uefi::boot::stall(20_000); // 20ms for auth to register

                        // Step 4: Set LED to on (max brightness)
                        let _ = self.send_xbox_packet(endpoint, &XBOXONE_LED_ON);
                        uefi::boot::stall(20_000); // 20ms

                        // Step 5: Send auth done again — some 3rd party controllers
                        // need a second auth ack after LED to clear the red indicator
                        let _ = self.send_xbox_packet(endpoint, &XBOXONE_AUTH_DONE);

                        // Give the controller time to finalize
                        uefi::boot::stall(50_000); // 50ms
                    } else {
                        dbg_log!("  Error: Xbox GIP has no OUT endpoint for activation");
                    }
                }
            }
            ControllerKind::DualSense => {
                dbg_log!("  Sending DualSense LED init (SET_REPORT 0x02)...");
                // DualSense USB output report 0x02
                // Must be at least 51 bytes to include RGB at offsets 48-50
                let mut report = [0u8; 63];
                report[0] = 0x02; // Report ID

                // Byte 1: valid_flag0 — enable motor/haptic updates (host takeover)
                report[1] = 0x01 | 0x02;

                // Byte 2: valid_flag2 — enable LED updates
                // 0x04 = lightbar color, 0x02 = lightbar setup, 0x01 = player LED
                report[2] = 0x04 | 0x02 | 0x01;

                // Bytes 3-43: motor/audio/feature (leave at 0 = no rumble)

                // Byte 44: mute LED mode (0 = off)
                report[44] = 0x00;

                // Byte 45: lightbar setup — 0x02 = disable fade, take host control
                report[45] = 0x02;

                // Byte 46: player LED brightness (0 = bright)
                report[46] = 0x00;

                // Byte 47: player LED pattern (0x04 = center LED)
                report[47] = 0x04;

                // Bytes 48-50: RGB lightbar color — Blue
                report[48] = 0x00; // R
                report[49] = 0x40; // G (slight cyan tint)
                report[50] = 0xFF; // B

                match self.usb_io.hid_set_report(
                    0x02, // Report ID
                    self.interface_number,
                    &mut report,
                ) {
                    Ok(_) => dbg_log!("  DualSense LED set to BLUE!"),
                    Err(e) => dbg_log!("  DualSense SET_REPORT failed: {:?}", e.status()),
                }

                uefi::boot::stall(50_000); // 50ms
            }
            ControllerKind::DualShock4 => {
                dbg_log!("  Sending DualShock 4 LED init (SET_REPORT 0x05)...");
                // DS4 USB output report 0x05 — 32 bytes
                // Sets lightbar color to blue
                let mut report = [0u8; 32];
                report[0] = 0x05; // Report ID
                report[1] = 0xFF; // Transaction flags

                // Byte 6: R, Byte 7: G, Byte 8: B
                report[6] = 0x00; // R
                report[7] = 0x40; // G (slight cyan tint)
                report[8] = 0xFF; // B

                match self.usb_io.hid_set_report(
                    0x05, // Report ID
                    self.interface_number,
                    &mut report,
                ) {
                    Ok(_) => dbg_log!("  DualShock 4 LED set to BLUE!"),
                    Err(e) => dbg_log!("  DS4 SET_REPORT failed: {:?}", e.status()),
                }

                uefi::boot::stall(50_000); // 50ms
            }
            ControllerKind::Unknown => {}
        }

        self.initialized = true;
        dbg_log!("  Initialization complete!");
        Ok(())
    }

    fn read_state(&mut self) -> Result<u8, Status> {
        // Strategy: Try interrupt transfer first for ALL controllers.
        // If it keeps failing for PlayStation controllers, fall back to
        // HID GET_REPORT via USB control transfer.

        let use_get_report = matches!(
            self.kind,
            ControllerKind::DualSense | ControllerKind::DualShock4
        ) && self.interrupt_failures >= 3;

        if use_get_report {
            // Fallback: HID GET_REPORT via control pipe (endpoint 0)
            return self.read_state_get_report();
        }

        // Primary: sync interrupt transfer
        match self.usb_io.sync_interrupt_transfer(
            self.in_endpoint,
            &mut self.buffer,
            USB_POLL_TIMEOUT_MS,
        ) {
            Ok(len) => {
                if self.interrupt_failures > 0 {
                    dbg_log!("  Sync interrupt transfer recovered! (len={})", len);
                }
                self.interrupt_failures = 0; // Reset on success
                if len == 0 {
                    return Ok(0);
                }
                let state = match self.kind {
                    ControllerKind::DualSense => parse_dualsense_state(&self.buffer[..len]),
                    ControllerKind::DualShock4 => parse_dualshock4_state(&self.buffer[..len]),
                    ControllerKind::Xbox => parse_xbox_state(&self.buffer[..len]),
                    ControllerKind::Unknown => 0,
                };
                if state != 0 {
                    dbg_log!("  Button pressed: {:02x}", state);
                }
                Ok(state)
            }
            Err(err) => {
                let status = err.status();
                if status != Status::TIMEOUT && status != Status::NOT_READY {
                    dbg_log!("  Sync interrupt transfer err: {:?}", status);
                }
                // Count failures for PlayStation to trigger GET_REPORT fallback
                if matches!(
                    self.kind,
                    ControllerKind::DualSense | ControllerKind::DualShock4
                ) && (status == Status::TIMEOUT
                    || status == Status::DEVICE_ERROR
                    || status == Status::NOT_READY)
                {
                    self.interrupt_failures = self.interrupt_failures.saturating_add(1);
                    // Try GET_REPORT immediately on the transition
                    if self.interrupt_failures >= 3 {
                        dbg_log!("  Sync interrupt failed 3x. Falling back to GET_REPORT");
                        return self.read_state_get_report();
                    }
                }
                Err(status)
            }
        }
    }

    /// Read controller state via HID GET_REPORT control transfer.
    /// This uses the default control pipe (endpoint 0) which is NEVER
    /// locked by UEFI's HID driver async interrupt transfers.
    fn read_state_get_report(&mut self) -> Result<u8, Status> {
        match self
            .usb_io
            .hid_get_report(0x01, self.interface_number, &mut self.buffer)
        {
            Ok(len) => {
                let state = match self.kind {
                    ControllerKind::DualSense => parse_dualsense_state(&self.buffer[..len]),
                    ControllerKind::DualShock4 => parse_dualshock4_state(&self.buffer[..len]),
                    _ => 0,
                };
                if state != 0 {
                    dbg_log!("  GET_REPORT button pressed: {:02x}", state);
                }
                Ok(state)
            }
            Err(err) => {
                let status = err.status();
                dbg_log!("  GET_REPORT failed: {:?}", status);
                Err(status)
            }
        }
    }

    fn send_xbox_packet(&mut self, endpoint: u8, template: &[u8]) -> Result<(), Status> {
        let mut packet = [0u8; 32];
        let len = template.len();
        packet[..len].copy_from_slice(template);
        if len > 2 {
            packet[2] = self.sequence;
            self.sequence = self.sequence.wrapping_add(1);
        }
        match self
            .usb_io
            .sync_interrupt_transfer(endpoint, &mut packet[..len], 100)
        {
            Ok(_) => Ok(()),
            Err(e) => {
                let status = e.status();
                dbg_log!("  Xbox packet send err: {:?}", status);
                Err(status)
            }
        }
    }
}

// ─── Report Parsers ─────────────────────────────────────────────────────────

/// DualSense USB Report ID 0x01 (64 bytes):
///   Byte 0:   Report ID (0x01)
///   Byte 1-2: Left Stick X, Y
///   Byte 3-4: Right Stick X, Y
///   Byte 5-6: L2, R2 triggers
///   Byte 7:   Sequence counter
///   Byte 8:   D-pad (bits 0-3) + action buttons (bits 4-7)
///             Bits 0-3: hat switch (0=N, 1=NE, 2=E, 3=SE, 4=S, 5=SW, 6=W, 7=NW, 8=neutral)
///             Bit 4: Square, Bit 5: Cross, Bit 6: Circle, Bit 7: Triangle
///   Byte 9:   L1, R1, L2btn, R2btn, Create, Options, L3, R3
fn parse_dualsense_state(report: &[u8]) -> u8 {
    if report.len() < 10 || report[0] != 0x01 {
        return 0;
    }

    let hat = report[8] & 0x0f;
    let buttons = report[8];
    let mut state = 0;

    // D-pad: 0=N, 7=NW, 1=NE -> up; 3=SE, 4=S, 5=SW -> down
    if hat <= 7 {
        if matches!(hat, 7 | 0 | 1) {
            state |= INPUT_UP;
        }
        if matches!(hat, 3 | 4 | 5) {
            state |= INPUT_DOWN;
        }
    }
    // Cross (bit 5) = Select
    if buttons & (1 << 5) != 0 {
        state |= INPUT_SELECT;
    }
    // Circle (bit 6) = Back
    if buttons & (1 << 6) != 0 {
        state |= INPUT_BACK;
    }

    // Also map Options (byte 9 bit 5) as Select, Create (byte 9 bit 4) as Back
    if report.len() > 9 {
        if report[9] & (1 << 5) != 0 {
            state |= INPUT_SELECT; // Options
        }
        if report[9] & (1 << 4) != 0 {
            state |= INPUT_BACK; // Create
        }
    }

    state
}

/// DualShock 4 USB Report ID 0x01 (64 bytes):
///   Byte 0:   Report ID (0x01)
///   Byte 1-2: Left Stick X, Y
///   Byte 3-4: Right Stick X, Y
///   Byte 5:   D-pad (bits 0-3) + action buttons (bits 4-7)
///             Bits 0-3: hat switch (same encoding as DualSense)
///             Bit 4: Square, Bit 5: Cross, Bit 6: Circle, Bit 7: Triangle
///   Byte 6:   L1, R1, L2, R2, Share, Options, L3, R3
fn parse_dualshock4_state(report: &[u8]) -> u8 {
    if report.len() < 7 || report[0] != 0x01 {
        return 0;
    }

    let hat = report[5] & 0x0f;
    let buttons = report[5];
    let mut state = 0;

    if hat <= 7 {
        if matches!(hat, 7 | 0 | 1) {
            state |= INPUT_UP;
        }
        if matches!(hat, 3 | 4 | 5) {
            state |= INPUT_DOWN;
        }
    }
    // Cross (bit 5) = Select
    if buttons & (1 << 5) != 0 {
        state |= INPUT_SELECT;
    }
    // Circle (bit 6) = Back
    if buttons & (1 << 6) != 0 {
        state |= INPUT_BACK;
    }

    // Also map Options (byte 6 bit 5) as Select, Share (byte 6 bit 4) as Back
    if report.len() > 6 {
        if report[6] & (1 << 5) != 0 {
            state |= INPUT_SELECT; // Options
        }
        if report[6] & (1 << 4) != 0 {
            state |= INPUT_BACK; // Share
        }
    }

    state
}

fn parse_xbox_state(report: &[u8]) -> u8 {
    if report.len() < 4 {
        return 0;
    }

    match report[0] {
        // Xbox 360 wired: 20-byte report starting with 0x00
        0x00 if report.len() >= 20 => parse_xbox360_state(report),
        // Xbox One GIP: message type 0x20
        0x20 if report.len() >= 6 => parse_xboxone_state(report),
        // Generic fallback: any Xbox report with buttons in bytes 2 and 3
        // (covers 30-byte XInput reports and other variants)
        _ if report.len() >= 4 => parse_xbox_generic_state(report),
        _ => 0,
    }
}

/// Xbox 360 wired controller USB report (20 bytes):
///   Byte 0:   Report type (0x00)
///   Byte 1:   Report size (0x14 = 20)
///   Byte 2:   Digital buttons:
///             Bit 0: D-pad Up, Bit 1: Down, Bit 2: Left, Bit 3: Right
///             Bit 4: Start, Bit 5: Back, Bit 6: L3, Bit 7: R3
///   Byte 3:   Bit 0: LB, Bit 1: RB, Bit 2: Guide
fn parse_xbox360_state(report: &[u8]) -> u8 {
    let buttons = report[2];
    let mut state = 0;

    // D-pad
    if buttons & (1 << 0) != 0 {
        state |= INPUT_UP;
    }
    if buttons & (1 << 1) != 0 {
        state |= INPUT_DOWN;
    }
    // Start (bit 4) = Select
    if buttons & (1 << 4) != 0 {
        state |= INPUT_SELECT;
    }
    // Back (bit 5) = Back
    if buttons & (1 << 5) != 0 {
        state |= INPUT_BACK;
    }

    // Buttons high byte contains A (bit 4) and B (bit 5)
    if report.len() > 3 {
        let buttons_hi = report[3];
        // A button (byte 3, bit 4) = Select
        if buttons_hi & (1 << 4) != 0 {
            state |= INPUT_SELECT;
        }
        // B button (byte 3, bit 5) = Back
        if buttons_hi & (1 << 5) != 0 {
            state |= INPUT_BACK;
        }
    }
    state
}

/// Xbox One GIP report (0x20 message type):
///   Byte 0:   Message type (0x20)
///   Byte 1:   Flags
///   Byte 2:   Sequence ID
///   Byte 3:   Payload length
///   Byte 4:   Buttons byte 1:
///             Bit 2: Menu, Bit 3: View
///             Bit 4: A, Bit 5: B, Bit 6: X, Bit 7: Y
///   Byte 5:   Buttons byte 2:
///             Bit 0: D-pad Up, Bit 1: Down, Bit 2: Left, Bit 3: Right
///             Bit 4: LB, Bit 5: RB
fn parse_xboxone_state(report: &[u8]) -> u8 {
    let buttons1 = report[4];
    let buttons2 = report[5];
    let mut state = 0;

    // D-pad (byte 5)
    if buttons2 & (1 << 0) != 0 {
        state |= INPUT_UP;
    }
    if buttons2 & (1 << 1) != 0 {
        state |= INPUT_DOWN;
    }
    // A button (byte 4, bit 4) = Select
    if buttons1 & (1 << 4) != 0 {
        state |= INPUT_SELECT;
    }
    // B button (byte 4, bit 5) = Back
    if buttons1 & (1 << 5) != 0 {
        state |= INPUT_BACK;
    }
    // Also map Menu (bit 2) as Select, View (bit 3) as Back
    if buttons1 & (1 << 2) != 0 {
        state |= INPUT_SELECT;
    }
    if buttons1 & (1 << 3) != 0 {
        state |= INPUT_BACK;
    }
    state
}

/// Generic Xbox / XInput fallback report parser.
/// The screenshot spec says: "Once activated, it spits a 30-byte packet.
/// Buttons are simple bitmasks in bytes 2 and 3."
/// This handles any Xbox report format that doesn't match the known 0x00 or 0x20 headers.
fn parse_xbox_generic_state(report: &[u8]) -> u8 {
    let buttons_lo = report[2]; // Byte 2: D-pad + Start/Back/L3/R3
    let buttons_hi = report[3]; // Byte 3: LB/RB/Guide + A/B/X/Y
    let mut state = 0;

    // Byte 2 bitmask (same layout as Xbox 360):
    // Bit 0: D-pad Up, Bit 1: Down, Bit 2: Left, Bit 3: Right
    // Bit 4: Start, Bit 5: Back, Bit 6: L3, Bit 7: R3
    if buttons_lo & (1 << 0) != 0 {
        state |= INPUT_UP;
    }
    if buttons_lo & (1 << 1) != 0 {
        state |= INPUT_DOWN;
    }
    if buttons_lo & (1 << 4) != 0 {
        state |= INPUT_SELECT; // Start
    }
    if buttons_lo & (1 << 5) != 0 {
        state |= INPUT_BACK; // Back
    }

    // Byte 3 bitmask:
    // Bit 4: A, Bit 5: B, Bit 6: X, Bit 7: Y
    if buttons_hi & (1 << 4) != 0 {
        state |= INPUT_SELECT; // A button
    }
    if buttons_hi & (1 << 5) != 0 {
        state |= INPUT_BACK; // B button
    }

    state
}
