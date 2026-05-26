# NeoBoot

A Rust UEFI graphical bootloader beta with controller-friendly input.

## Build

```bash
rustup target add x86_64-unknown-uefi
cargo build --release --target x86_64-unknown-uefi
cp target/x86_64-unknown-uefi/release/neoboot.efi neodata.efi
```

## Files

- `neodata.efi` — UEFI bootloader binary
- `boot.cfg` — Boot menu configuration
- `boot-system.object` — System initialization metadata
- `src/` — Rust source code

## Features

- Embedded NotoSans TTF rendering with bitmap fallback
- Embedded PNG controller icons from `desgin/`
- Keyboard navigation (arrow keys, Enter, W/S, Space, A/X)
- Direct USB HID input for DualSense and Xbox controllers
- Auto-boot countdown for beta production use
- Boot actions: Linux EFI kernel launch, reboot, shutdown, disk scanning
- `scan-disk` walks detected EFI filesystems, finds likely Linux EFI kernels, and adds them to the menu without duplicating existing entries

## Configuration

Edit `boot.cfg` to customize boot entries. Format:

```ini
[global]
default=0
timeout=5
show_timeout=true
font=assets/NotoSans-Regular.ttf

[entry "Entry Name"]
type=linux
kernel=EFI/NeoBoot/vmlinuz.efi
initrd=EFI/NeoBoot/initrd.img
rootfs=/dev/usb1
cmdline=rw quiet
```

Entry types: `linux`, `firmware-volumes`, `reboot-menu`, `install-kernel`, `scan-disk`, `reboot`, `shutdown`.

For Linux entries:

- `kernel` points to the EFI-stub-enabled kernel image on the ESP.
- `initrd` is passed to the kernel as `initrd=\...` using EFI-style absolute paths.
- `rootfs` becomes `root=...` unless `cmdline` already includes `root=`.
- `cmdline` appends any additional kernel arguments.

For `scan-disk`:

- NeoBoot scans EFI volumes recursively for `.efi` kernels whose names look like `vmlinuz`, `linux`, `bzImage`, or `kernel`.
- Matching `initrd`, `initramfs`, `.cpio`, and `rootfs` files in the same directory are attached automatically when found.
- Re-running the scan rebuilds the scanned section instead of appending duplicates.

## Controller Support

UEFI firmware does not define a standard gamepad protocol, so NeoBoot talks to supported USB controllers directly through `EFI_USB_IO_PROTOCOL`. The current implementation supports:

- Sony DualSense USB input for D-pad navigation and Cross to select
- Microsoft Xbox USB input for D-pad navigation and A to select
- Keyboard fallback with arrow keys, Enter, W/S, Space, A, and X
