use alloc::vec::Vec;
use core::mem::MaybeUninit;
use core::ptr::NonNull;
use uefi::boot;
use uefi::proto::unsafe_protocol;
use uefi::{Guid, Handle, Result, Status, StatusExt};

/// Official EFI_USB_IO_PROTOCOL_GUID:
/// { 0x2B2F68D6, 0x0CD2, 0x44CF, { 0x8E, 0x8B, 0xBB, 0xA2, 0x0B, 0x1B, 0x5B, 0x75 } }
/// String: "2b2f68d6-0cd2-44cf-8e8b-bba20b1b5b75"
///
/// Guid::new takes the first three fields as LITTLE-ENDIAN byte arrays:
///   time_low  = 0x2b2f68d6 → LE bytes [0xd6, 0x68, 0x2f, 0x2b]
///   time_mid  = 0x0cd2     → LE bytes [0xd2, 0x0c]
///   time_high = 0x44cf     → LE bytes [0xcf, 0x44]
pub const EFI_USB_IO_PROTOCOL_GUID: Guid = Guid::new(
    [0xd6, 0x68, 0x2f, 0x2b],
    [0xd2, 0x0c],
    [0xcf, 0x44],
    0x8e,
    0x8b,
    [0xbb, 0xa2, 0x0b, 0x1b, 0x5b, 0x75],
);

pub const EFI_USB2_HC_PROTOCOL_GUID: Guid = Guid::new(
    [0x26, 0x52, 0x74, 0x3e],
    [0x18, 0x98],
    [0xb6, 0x45],
    0xa2,
    0x26,
    [0xf0, 0xd5, 0x1f, 0x78, 0xa7, 0x4b],
);

pub const EFI_USB_HC_PROTOCOL_GUID: Guid = Guid::new(
    [0x80, 0x15, 0x87, 0xf1],
    [0x9c, 0xbc],
    [0xd2, 0x11],
    0x9a,
    0x2c,
    [0x00, 0x90, 0x27, 0x3f, 0xc1, 0x4d],
);

pub const EFI_PCI_IO_PROTOCOL_GUID: Guid = Guid::new(
    [0x00, 0xb2, 0xf5, 0x4c],
    [0xb8, 0x68],
    [0xa5, 0x4c],
    0x9e,
    0xec,
    [0xb2, 0x3e, 0x3f, 0x50, 0x02, 0x9a],
);

#[unsafe_protocol("4cf5b200-68b8-4ca5-9eec-b23e3f50029a")]
#[repr(C)]
pub struct RawPciIoProtocol {
    pub poll_mem: usize,
    pub poll_io: usize,
    pub mem_read: usize,
    pub mem_write: usize,
    pub io_read: usize,
    pub io_write: usize,
    pub pci_read: extern "efiapi" fn(
        *mut RawPciIoProtocol,
        u32,     // Width
        u32,     // Offset
        usize,   // Count
        *mut u8, // Buffer
    ) -> Status,
    pub pci_write: usize,
}

#[unsafe_protocol("2b2f68d6-0cd2-44cf-8e8b-bba20b1b5b75")]
#[repr(C)]
pub struct RawUsbIoProtocol {
    // 1. EFI_USB_IO_CONTROL_TRANSFER
    pub usb_control_transfer: extern "efiapi" fn(
        *mut RawUsbIoProtocol,
        *mut EfiUsbDeviceRequest,
        EfiUsbDataDirection,
        u32,      // Timeout (ms)
        *mut u8,  // Data buffer
        usize,    // DataLength
        *mut u32, // USB transfer status
    ) -> Status,
    // 2. EFI_USB_IO_BULK_TRANSFER (placeholder — not called, same size as fn ptr)
    pub usb_bulk_transfer: usize,
    // 3. EFI_USB_IO_ASYNC_INTERRUPT_TRANSFER
    pub usb_async_interrupt_transfer: extern "efiapi" fn(
        *mut RawUsbIoProtocol,
        u8,    // DeviceEndpoint
        u8,    // IsNewTransfer (BOOLEAN)
        usize, // PollingInterval OPTIONAL
        usize, // DataLength OPTIONAL
        usize, // InterruptCallback OPTIONAL (fn pointer, 0 for cancel)
        usize, // Context OPTIONAL
    ) -> Status,
    // 4. EFI_USB_IO_SYNC_INTERRUPT_TRANSFER
    pub usb_sync_interrupt_transfer: extern "efiapi" fn(
        *mut RawUsbIoProtocol,
        u8,
        *mut u8,
        *mut usize,
        usize,
        *mut u32,
    ) -> Status,
    // 5. EFI_USB_IO_ISOCHRONOUS_TRANSFER (placeholder)
    pub usb_isochronous_transfer: usize,
    // 6. EFI_USB_IO_ASYNC_ISOCHRONOUS_TRANSFER (placeholder)
    pub usb_async_isochronous_transfer: usize,
    // 7. EFI_USB_IO_GET_DEVICE_DESCRIPTOR
    pub usb_get_device_descriptor:
        extern "efiapi" fn(*mut RawUsbIoProtocol, *mut EfiUsbDeviceDescriptor) -> Status,
    // 8. EFI_USB_IO_GET_CONFIG_DESCRIPTOR
    pub usb_get_config_descriptor:
        extern "efiapi" fn(*mut RawUsbIoProtocol, *mut EfiUsbConfigDescriptor) -> Status,
    // 9. EFI_USB_IO_GET_INTERFACE_DESCRIPTOR
    pub usb_get_interface_descriptor:
        extern "efiapi" fn(*mut RawUsbIoProtocol, *mut EfiUsbInterfaceDescriptor) -> Status,
    // 10. EFI_USB_IO_GET_ENDPOINT_DESCRIPTOR
    pub usb_get_endpoint_descriptor:
        extern "efiapi" fn(*mut RawUsbIoProtocol, u8, *mut EfiUsbEndpointDescriptor) -> Status,
    // 11. EFI_USB_IO_GET_STRING_DESCRIPTOR (placeholder)
    pub usb_get_string_descriptor: usize,
    // 12. EFI_USB_IO_GET_SUPPORTED_LANGUAGES (placeholder)
    pub usb_get_supported_languages: usize,
    // 13. EFI_USB_IO_PORT_RESET
    pub usb_port_reset: extern "efiapi" fn(*mut RawUsbIoProtocol) -> Status,
}

#[repr(C)]
pub struct EfiUsbDeviceRequest {
    pub request_type: u8,
    pub request: u8,
    pub value: u16,
    pub index: u16,
    pub length: u16,
}

/// USB data direction for control transfers.
/// Values MUST match the UEFI spec (EDK2 UsbIo.h):
///   EfiUsbDataIn  = 0  (device → host)
///   EfiUsbDataOut = 1  (host → device)
///   EfiUsbNoData  = 2
#[repr(u32)]
pub enum EfiUsbDataDirection {
    In = 0,
    Out = 1,
    NoData = 2,
}

#[repr(C)]
pub struct EfiUsbDeviceDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub bcd_usb: u16,
    pub device_class: u8,
    pub device_subclass: u8,
    pub device_protocol: u8,
    pub max_packet_size0: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub device_release: u16,
    pub manufacturer_string: u8,
    pub product_string: u8,
    pub serial_number_string: u8,
    pub num_configurations: u8,
}

#[repr(C)]
pub struct EfiUsbConfigDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub total_length: u16,
    pub num_interfaces: u8,
    pub configuration_value: u8,
    pub configuration: u8,
    pub attributes: u8,
    pub max_power: u8,
}

#[repr(C)]
pub struct EfiUsbInterfaceDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub interface_number: u8,
    pub alternate_setting: u8,
    pub num_endpoints: u8,
    pub interface_class: u8,
    pub interface_subclass: u8,
    pub interface_protocol: u8,
    pub interface_string: u8,
}

#[repr(C)]
pub struct EfiUsbEndpointDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub endpoint_address: u8,
    pub attributes: u8,
    pub max_packet_size: u16,
    pub interval: u8,
}

pub struct UsbIo {
    inner: NonNull<RawUsbIoProtocol>,
}

impl UsbIo {
    pub unsafe fn from_raw(ptr: *mut RawUsbIoProtocol) -> Option<Self> {
        NonNull::new(ptr).map(|inner| Self { inner })
    }

    pub unsafe fn from_open_protocol(handle: Handle) -> Result<Self> {
        let params = boot::OpenProtocolParams {
            handle,
            agent: boot::image_handle(),
            controller: None,
        };
        let mut protocol = boot::open_protocol::<RawUsbIoProtocol>(
            params,
            boot::OpenProtocolAttributes::GetProtocol,
        )?;
        let raw_ptr: *mut RawUsbIoProtocol = &mut *protocol as *mut _;
        let inner = NonNull::new(raw_ptr).ok_or(Status::LOAD_ERROR)?;
        core::mem::forget(protocol);
        Ok(Self { inner })
    }

    pub unsafe fn from_open_protocol_exclusive(handle: Handle) -> Result<Self> {
        // Try Exclusive first to prevent UEFI HID driver from reclaiming the device
        match boot::open_protocol::<RawUsbIoProtocol>(
            boot::OpenProtocolParams {
                handle,
                agent: boot::image_handle(),
                controller: None,
            },
            boot::OpenProtocolAttributes::Exclusive,
        ) {
            Ok(mut protocol) => {
                let raw_ptr: *mut RawUsbIoProtocol = &mut *protocol as *mut _;
                let inner = NonNull::new(raw_ptr).ok_or(Status::LOAD_ERROR)?;
                core::mem::forget(protocol);
                crate::dbg_log!("  Opened protocol exclusively!");
                Ok(Self { inner })
            }
            Err(e) => {
                crate::dbg_log!(
                    "  Exclusive open failed: {:?}. Trying GetProtocol fallback...",
                    e.status()
                );
                let mut protocol = boot::open_protocol::<RawUsbIoProtocol>(
                    boot::OpenProtocolParams {
                        handle,
                        agent: boot::image_handle(),
                        controller: None,
                    },
                    boot::OpenProtocolAttributes::GetProtocol,
                )?;
                let raw_ptr: *mut RawUsbIoProtocol = &mut *protocol as *mut _;
                let inner = NonNull::new(raw_ptr).ok_or(Status::LOAD_ERROR)?;
                core::mem::forget(protocol);
                Ok(Self { inner })
            }
        }
    }

    fn as_ptr(&self) -> *mut RawUsbIoProtocol {
        self.inner.as_ptr()
    }

    pub fn get_device_descriptor(&self) -> Result<EfiUsbDeviceDescriptor> {
        let mut desc = MaybeUninit::<EfiUsbDeviceDescriptor>::uninit();
        unsafe {
            ((self.as_ptr().as_mut().unwrap()).usb_get_device_descriptor)(
                self.as_ptr(),
                desc.as_mut_ptr(),
            )
        }
        .to_result_with_val(|| unsafe { desc.assume_init() })
    }

    pub fn get_interface_descriptor(&self) -> Result<EfiUsbInterfaceDescriptor> {
        let mut desc = MaybeUninit::<EfiUsbInterfaceDescriptor>::uninit();
        unsafe {
            ((self.as_ptr().as_mut().unwrap()).usb_get_interface_descriptor)(
                self.as_ptr(),
                desc.as_mut_ptr(),
            )
        }
        .to_result_with_val(|| unsafe { desc.assume_init() })
    }

    pub fn get_endpoint_descriptor(&self, index: u8) -> Result<EfiUsbEndpointDescriptor> {
        let mut desc = MaybeUninit::<EfiUsbEndpointDescriptor>::uninit();
        unsafe {
            ((self.as_ptr().as_mut().unwrap()).usb_get_endpoint_descriptor)(
                self.as_ptr(),
                index,
                desc.as_mut_ptr(),
            )
        }
        .to_result_with_val(|| unsafe { desc.assume_init() })
    }

    /// USB control transfer via the default control pipe (endpoint 0).
    pub fn control_transfer(
        &self,
        request: &mut EfiUsbDeviceRequest,
        direction: EfiUsbDataDirection,
        timeout_ms: u32,
        data: &mut [u8],
    ) -> Result<usize> {
        let mut transfer_status = 0u32;
        let data_len = data.len();
        unsafe {
            ((self.as_ptr().as_mut().unwrap()).usb_control_transfer)(
                self.as_ptr(),
                request as *mut EfiUsbDeviceRequest,
                direction,
                timeout_ms,
                data.as_mut_ptr(),
                data_len,
                &mut transfer_status,
            )
        }
        .to_result_with_val(|| data_len)
    }

    /// HID GET_REPORT via USB control transfer.
    /// Uses the default control pipe (endpoint 0) which is always available,
    /// even when UEFI's HID driver has the interrupt pipe locked.
    pub fn hid_get_report(
        &self,
        report_id: u8,
        interface_num: u8,
        buf: &mut [u8],
    ) -> Result<usize> {
        let mut request = EfiUsbDeviceRequest {
            request_type: 0xA1,                 // Device-to-host | Class | Interface
            request: 0x01,                      // GET_REPORT
            value: 0x0100 | (report_id as u16), // Report Type: Input (01) | Report ID
            index: interface_num as u16,
            length: buf.len() as u16,
        };
        self.control_transfer(
            &mut request,
            EfiUsbDataDirection::In, // value = 0 = EfiUsbDataIn (device → host)
            100,                     // 100ms timeout
            buf,
        )
    }

    /// HID SET_REPORT via USB control transfer.
    /// Sends an output report to the device (e.g. to set LED colors on DualSense/DS4).
    pub fn hid_set_report(
        &self,
        report_id: u8,
        interface_num: u8,
        buf: &mut [u8],
    ) -> Result<usize> {
        let mut request = EfiUsbDeviceRequest {
            request_type: 0x21,                 // Host-to-device | Class | Interface
            request: 0x09,                      // SET_REPORT
            value: 0x0200 | (report_id as u16), // Report Type: Output (02) | Report ID
            index: interface_num as u16,
            length: buf.len() as u16,
        };
        self.control_transfer(
            &mut request,
            EfiUsbDataDirection::Out, // host → device
            200,                      // 200ms timeout
            buf,
        )
    }

    /// Get the active USB configuration descriptor.
    pub fn get_config_descriptor(&self) -> Result<EfiUsbConfigDescriptor> {
        let mut desc = MaybeUninit::<EfiUsbConfigDescriptor>::uninit();
        unsafe {
            ((self.as_ptr().as_mut().unwrap()).usb_get_config_descriptor)(
                self.as_ptr(),
                desc.as_mut_ptr(),
            )
        }
        .to_result_with_val(|| unsafe { desc.assume_init() })
    }

    /// USB SET_CONFIGURATION standard request via control transfer.
    /// This puts the device into the Configured state so endpoints become active.
    /// Must be called after disconnecting UEFI drivers, which may de-configure the device.
    pub fn set_configuration(&self, config_value: u8) -> Result<usize> {
        let mut request = EfiUsbDeviceRequest {
            request_type: 0x00, // Host-to-device | Standard | Device
            request: 0x09,      // SET_CONFIGURATION
            value: config_value as u16,
            index: 0,
            length: 0,
        };
        let mut dummy = [0u8; 0];
        self.control_transfer(
            &mut request,
            EfiUsbDataDirection::NoData,
            500, // 500ms timeout
            &mut dummy,
        )
    }

    /// Synchronous interrupt transfer (IN or OUT, depending on endpoint address).
    pub fn sync_interrupt_transfer(
        &self,
        endpoint: u8,
        data: &mut [u8],
        timeout_ms: usize,
    ) -> Result<usize> {
        let mut data_len = data.len();
        let mut transfer_status = 0u32;
        unsafe {
            ((self.as_ptr().as_mut().unwrap()).usb_sync_interrupt_transfer)(
                self.as_ptr(),
                endpoint,
                data.as_mut_ptr(),
                &mut data_len,
                timeout_ms,
                &mut transfer_status,
            )
        }
        .to_result_with_val(|| data_len)
    }

    /// Cancel any existing async interrupt transfer on this endpoint.
    pub fn cancel_async_interrupt(&self, endpoint: u8) -> Result {
        unsafe {
            ((self.as_ptr().as_mut().unwrap()).usb_async_interrupt_transfer)(
                self.as_ptr(),
                endpoint,
                0, // IsNewTransfer = FALSE (cancel)
                0, // PollingInterval (ignored)
                0, // DataLength (ignored)
                0, // InterruptCallback (ignored)
                0, // Context (ignored)
            )
        }
        .to_result()
    }

    /// Reset the USB port for this device.
    pub fn port_reset(&self) -> Result {
        unsafe { ((self.as_ptr().as_mut().unwrap()).usb_port_reset)(self.as_ptr()) }.to_result()
    }

    pub fn interrupt_in_endpoint(&self) -> Result<Option<u8>> {
        self.find_interrupt_endpoint(true)
    }

    pub fn interrupt_out_endpoint(&self) -> Result<Option<u8>> {
        self.find_interrupt_endpoint(false)
    }

    fn find_interrupt_endpoint(&self, in_endpoint: bool) -> Result<Option<u8>> {
        let interface = self.get_interface_descriptor()?;
        for index in 0..interface.num_endpoints {
            let endpoint = self.get_endpoint_descriptor(index)?;
            let is_interrupt = endpoint.attributes & 0x03 == 0x03;
            let is_in = endpoint.endpoint_address & 0x80 != 0;
            if is_interrupt && is_in == in_endpoint {
                return Ok(Some(endpoint.endpoint_address));
            }
        }
        Ok(None)
    }
}

pub fn find_usb_io_handles() -> Result<Vec<Handle>> {
    // Primary: use the Protocol trait GUID generated by unsafe_protocol macro
    match boot::find_handles::<RawUsbIoProtocol>() {
        Ok(handles) if !handles.is_empty() => {
            return Ok(handles);
        }
        _ => {}
    }

    // Fallback: use the manually-constructed GUID constant directly.
    // This catches cases where the macro generates a slightly different GUID
    // or the trait-based search doesn't work on this firmware.
    crate::dbg_log!("Trying raw GUID fallback for USB IO...");
    match boot::locate_handle_buffer(boot::SearchType::ByProtocol(&EFI_USB_IO_PROTOCOL_GUID)) {
        Ok(buf) => {
            let handles: Vec<Handle> = buf.to_vec();
            if !handles.is_empty() {
                crate::dbg_log!("Raw GUID found {} handle(s)!", handles.len());
                return Ok(handles);
            }
            Err(uefi::Error::new(Status::NOT_FOUND, ()))
        }
        Err(e) => Err(e),
    }
}

/// Programmatically connect all controllers recursively in the UEFI system database
/// using a multi-pass approach with delays between passes. This ensures child handles
/// created during the connection of parent controllers (e.g., PCI -> USB Host Controller
/// -> USB Bus -> USB Device) are themselves recursively connected in subsequent passes.
///
/// After all passes, waits 2 seconds for the USB bus driver to fully enumerate
/// connected devices and create their USB IO protocol instances.
pub fn reconnect_usb_host_controllers() {
    crate::dbg_log!("Resetting USB Host Controllers...");
    let mut hc_handles = Vec::new();
    let mut pci_count = 0;

    // 1a. Locate by USB2 HC Protocol
    if let Ok(handles) =
        boot::locate_handle_buffer(boot::SearchType::ByProtocol(&EFI_USB2_HC_PROTOCOL_GUID))
    {
        for &h in handles.iter() {
            if !hc_handles.contains(&h) {
                hc_handles.push(h);
            }
        }
    }

    // 1b. Locate by USB HC Protocol
    if let Ok(handles) =
        boot::locate_handle_buffer(boot::SearchType::ByProtocol(&EFI_USB_HC_PROTOCOL_GUID))
    {
        for &h in handles.iter() {
            if !hc_handles.contains(&h) {
                hc_handles.push(h);
            }
        }
    }

    // 1c. Locate by PCI IO Protocol and inspect PCI class code
    if let Ok(handles) =
        boot::locate_handle_buffer(boot::SearchType::ByProtocol(&EFI_PCI_IO_PROTOCOL_GUID))
    {
        for &h in handles.iter() {
            if hc_handles.contains(&h) {
                continue;
            }
            unsafe {
                let params = boot::OpenProtocolParams {
                    handle: h,
                    agent: boot::image_handle(),
                    controller: None,
                };
                if let Ok(mut protocol) = boot::open_protocol::<RawPciIoProtocol>(
                    params,
                    boot::OpenProtocolAttributes::GetProtocol,
                ) {
                    let raw_ptr: *mut RawPciIoProtocol = &mut *protocol as *mut _;
                    let mut config_data = 0u32;
                    let status = ((raw_ptr.as_mut().unwrap()).pci_read)(
                        raw_ptr,
                        2,    // EfiPciIoWidthUint32 = 2
                        0x08, // Revision ID + Class Code offset
                        1,    // Count
                        &mut config_data as *mut u32 as *mut u8,
                    );
                    core::mem::forget(protocol);

                    if status.is_success() {
                        let class_code = (config_data >> 8) & 0x00FFFFFF;
                        let base_class = (class_code >> 16) & 0xFF;
                        let subclass = (class_code >> 8) & 0xFF;
                        let prog_if = class_code & 0xFF;
                        if base_class == 0x0C && subclass == 0x03 {
                            let speed_str = match prog_if {
                                0x00 => "UHCI",
                                0x10 => "OHCI",
                                0x20 => "EHCI",
                                0x30 => "XHCI",
                                _ => "USB",
                            };
                            crate::dbg_log!("  Found {} HC: {:?}", speed_str, h);
                            hc_handles.push(h);
                        } else {
                            pci_count += 1;
                        }
                    }
                }
            }
        }
    }

    crate::dbg_log!(
        "Found {} USB HC handle(s). Other PCI: {}",
        hc_handles.len(),
        pci_count
    );

    // 2. Disconnect each host controller
    for &h in hc_handles.iter() {
        crate::dbg_log!("  Disconnecting HC: {:?}", h);
        let _ = boot::disconnect_controller(h, None, None);
    }

    // 3. Connect each host controller recursively
    for &h in hc_handles.iter() {
        crate::dbg_log!("  Connecting HC recursively: {:?}", h);
        let _ = boot::connect_controller(h, None, None, true);
    }
}

/// Programmatically connect all controllers recursively in the UEFI system database
/// using a multi-pass approach with delays between passes. This ensures child handles
/// created during the connection of parent controllers (e.g., PCI -> USB Host Controller
/// -> USB Bus -> USB Device) are themselves recursively connected in subsequent passes.
///
/// After all passes, waits 2 seconds for the USB bus driver to fully enumerate
/// connected devices and create their USB IO protocol instances.
pub fn connect_all_controllers() {
    crate::dbg_log!("Connecting all controllers in system...");

    // Perform full USB Host Controller reset/reconnection pass
    reconnect_usb_host_controllers();

    let mut last_handle_count = 0;
    for pass in 1..=6 {
        if let Ok(handles) = boot::locate_handle_buffer(boot::SearchType::AllHandles) {
            let count = handles.len();
            crate::dbg_log!("Pass {}: Found {} handles in system", pass, count);

            let mut connected_count = 0;
            for &handle in handles.iter() {
                // Recursive connect to start all drivers downstream
                if boot::connect_controller(handle, None, None, true).is_ok() {
                    connected_count += 1;
                }
            }
            crate::dbg_log!(
                "  Connected {}/{} handles recursively",
                connected_count,
                count
            );

            // If no new handles were created, we may have fully connected the tree
            // But do at least 3 passes to be safe
            if count == last_handle_count && pass >= 3 {
                crate::dbg_log!("No new handles discovered after pass {}. Done.", pass);
                break;
            }
            last_handle_count = count;

            // Wait 500ms between passes for USB bus enumeration
            boot::stall(500_000);
        } else {
            crate::dbg_log!("Failed to locate all handles buffer");
            break;
        }
    }

    // Final wait: give the USB bus driver time to fully enumerate devices,
    // reset them, assign addresses, fetch descriptors, and install USB IO protocol.
    // USB 2.0 spec allows up to 100ms per device reset + 10ms recovery.
    // With hubs and multiple devices, 2 seconds is a safe margin.
    crate::dbg_log!("Waiting 2s for USB device enumeration...");
    boot::stall(2_000_000);

    // One final connection pass after the delay
    if let Ok(handles) = boot::locate_handle_buffer(boot::SearchType::AllHandles) {
        crate::dbg_log!("Final pass: {} handles in system", handles.len());
        for &handle in handles.iter() {
            let _ = boot::connect_controller(handle, None, None, true);
        }
    }
}

/// Disconnect all UEFI drivers (e.g. the built-in HID driver) from a handle.
pub fn disconnect_existing_drivers(handle: Handle) -> Result {
    boot::disconnect_controller(handle, None, None)
}

/// Classify a controller by Vendor ID / Product ID, Class, Subclass, and Protocol.
pub fn classify_controller(
    vendor_id: u16,
    product_id: u16,
    interface_class: u8,
    interface_subclass: u8,
    interface_protocol: u8,
) -> ControllerKind {
    // Sony PlayStation controllers — only match HID interfaces (class 0x03)
    // to avoid locking onto audio or other non-gamepad interfaces
    if vendor_id == 0x054c && interface_class == 0x03 {
        // DualShock 4: PID 0x05c4 (v1), 0x09cc (v2)
        if matches!(product_id, 0x05c4 | 0x09cc) {
            return ControllerKind::DualShock4;
        }
        // DualSense: PID 0x0ce6 (standard), 0x0df2 (Edge)
        if matches!(product_id, 0x0ce6 | 0x0df2) {
            return ControllerKind::DualSense;
        }
    }

    // Xbox / XInput controllers:
    // Typically use vendor-specific class (0xFF)
    // Subclass 0x47 (Xbox One / Series GIP), Subclass 0x5D (XInput/Xbox 360), Subclass 0x01 (Xbox 360 Wired)
    // Protocol 0xD0 (Xbox One/Series), Protocol 0x01 (Xbox 360/XInput)
    if interface_class == 0xFF {
        if vendor_id == 0x045e
            || matches!(interface_subclass, 0x47 | 0x5D | 0x01)
            || interface_protocol == 0x01
        {
            return ControllerKind::Xbox;
        }
    }

    ControllerKind::Unknown
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ControllerKind {
    Unknown,
    DualSense,
    DualShock4,
    Xbox,
}
