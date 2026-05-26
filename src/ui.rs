extern crate alloc;

use crate::config::BootEntry;
use crate::font::FontRenderer;
use crate::icons;
use crate::input::InputSource;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

const WHITE: u32 = 0x00ff_ffff;
const BLACK: u32 = 0x0000_0000;
const BACKGROUND_IMAGE: &[u8] = include_bytes!("background_boot.raw");
const BACKGROUND_DIM_PERCENT: u32 = 70;

pub struct UI {
    width: usize,
    height: usize,
    fb: *mut u8,
    stride: usize,
    backbuffer: Vec<u32>,
}

impl UI {
    pub fn new(width: usize, height: usize, fb: *mut u8) -> Self {
        Self {
            width,
            height,
            fb,
            stride: width * 4,
            backbuffer: vec![BLACK; width * height],
        }
    }

    pub fn clear(&mut self) {
        self.backbuffer.fill(BLACK);
    }

    pub fn draw_background(&mut self) {
        self.draw_dimmed_background(BACKGROUND_IMAGE, BACKGROUND_DIM_PERCENT);
    }

    pub fn draw_title(&mut self, title: &str, font: Option<&FontRenderer>) {
        if let Some(f) = font {
            if f.draw_text(self, 48, 32, 32, title, WHITE) {
                self.draw_hline(0, 108, self.width, WHITE);
                self.draw_hline(0, self.height.saturating_sub(170), self.width, WHITE);
                return;
            }
        }
        self.draw_text(48, 48, title, 4, WHITE);
        self.draw_hline(0, 108, self.width, WHITE);
        self.draw_hline(0, self.height.saturating_sub(100), self.width, WHITE);
    }

    pub fn draw_status(&mut self, text: &str, y: usize, font: Option<&FontRenderer>) {
        self.draw_text_line(48, y, 18, text, font);
    }

    pub fn draw_scan_screen(
        &mut self,
        font: Option<&FontRenderer>,
        completed_disks: usize,
        total_disks: usize,
        source: InputSource,
    ) {
        let left = self.width / 6;
        let body_y = self.height / 3;
        let bar_x = self.width / 8;
        let bar_y = self.height / 2 + 18;
        let bar_w = self.width * 3 / 4;
        let bar_h = 22;

        self.draw_text_line(
            left,
            body_y,
            20,
            "Scanning disk... Please do not turn off the device or unplug the USB/SSD",
            font,
        );
        self.draw_text_line(left, body_y + 34, 20, "drive during scanning.", font);

        self.draw_progress_bar(bar_x, bar_y, bar_w, bar_h, completed_disks, total_disks);
        self.draw_scan_footer(font, source);
    }

    fn draw_scan_footer(&mut self, font: Option<&FontRenderer>, source: InputSource) {
        let y = self.height.saturating_sub(117);
        match source {
            InputSource::PlayStation => {
                icons::CIRCLE.draw(self, 56, y);
                self.draw_label(font, 112, y + 18, "Cancel");
            }
            InputSource::Xbox => {
                icons::XBOX_B.draw(self, 56, y);
                self.draw_label(font, 112, y + 18, "Cancel");
            }
            InputSource::Both => {
                icons::CIRCLE.draw(self, 32, y);
                icons::XBOX_B.draw(self, 92, y);
                self.draw_label(font, 148, y + 18, "Cancel");
            }
            InputSource::None | InputSource::Keyboard => {}
        }
    }

    fn draw_progress_bar(
        &mut self,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        completed: usize,
        total: usize,
    ) {
        self.draw_rect(x, y, w, h, WHITE);

        if total == 0 {
            return;
        }

        let filled = (w.saturating_sub(6) * completed.min(total)) / total;
        if filled > 0 {
            self.fill_rect(x + 3, y + 3, filled, h.saturating_sub(6), WHITE);
        }
    }

    fn draw_text_line(
        &mut self,
        x: usize,
        y: usize,
        size: usize,
        text: &str,
        font: Option<&FontRenderer>,
    ) {
        if let Some(f) = font {
            if f.draw_text(self, x, y, size, text, WHITE) {
                return;
            }
        }
        let scale = if size >= 28 {
            3
        } else if size >= 18 {
            2
        } else {
            1
        };
        self.draw_text(x, y + 4, text, scale, WHITE);
    }

    pub fn draw_menu(
        &mut self,
        entries: &[BootEntry],
        selected: usize,
        font: Option<&FontRenderer>,
    ) {
        let start_x = self.width / 8;
        let start_y = self.height / 4 + 10;
        let row_h = 56;
        let box_w = self.width * 3 / 4;

        for (i, entry) in entries.iter().enumerate() {
            let y = start_y + i * row_h;
            if i == selected {
                self.draw_rect(start_x, y - 8, box_w, 60, WHITE);
            }
            let label = format_entry(i + 1, &entry.title);
            if let Some(f) = font {
                if f.draw_text(self, start_x + 12, y + 4, 30, &label, WHITE) {
                    continue;
                }
            }
            self.draw_text(start_x + 12, y + 12, &label, 3, WHITE);
        }
    }

    pub fn draw_volume_browser(
        &mut self,
        entries: &[BootEntry],
        selected: usize,
        font: Option<&FontRenderer>,
        source: InputSource,
    ) {
        let start_x = self.width / 8;
        let start_y = 190;
        let row_h = 118;
        let box_w = self.width * 3 / 4 + 60;

        self.draw_text_line(start_x - 28, 150, 24, "List volume available :", font);

        for (i, entry) in entries.iter().enumerate() {
            let y = start_y + i * row_h;
            if i == selected {
                self.draw_rect(start_x, y - 10, box_w, 90, WHITE);
            }

            icons::STORAGE.draw(self, start_x + 12, y + 2);
            self.draw_text_line(start_x + 92, y + 14, 24, &entry.title, font);
            if let Some(subtitle) = entry.subtitle.as_deref() {
                self.draw_text_line(start_x + 92, y + 52, 16, subtitle, font);
            }
        }

        self.draw_text_line(
            start_x - 28,
            self.height.saturating_sub(230),
            18,
            "If you don't see these volumes, please rescan the disk.",
            font,
        );
        self.draw_volume_footer(font, source);
    }

    fn draw_volume_footer(&mut self, font: Option<&FontRenderer>, source: InputSource) {
        let y = self.height.saturating_sub(117);
        match source {
            InputSource::PlayStation => {
                icons::CIRCLE.draw(self, 30, y);
                self.draw_label(font, 86, y + 18, "Cancel");
                icons::CROSS.draw(self, 200, y);
                self.draw_label(font, 256, y + 18, "Select");
            }
            InputSource::Xbox => {
                icons::XBOX_B.draw(self, 30, y);
                self.draw_label(font, 86, y + 18, "Cancel");
                icons::XBOX_A.draw(self, 200, y);
                self.draw_label(font, 256, y + 18, "Select");
            }
            InputSource::Both => {
                icons::CIRCLE.draw(self, 18, y);
                icons::XBOX_B.draw(self, 72, y);
                self.draw_label(font, 128, y + 18, "Cancel");
                icons::CROSS.draw(self, 250, y);
                icons::XBOX_A.draw(self, 304, y);
                self.draw_label(font, 360, y + 18, "Select");
            }
            InputSource::None | InputSource::Keyboard => {}
        }
    }

    pub fn draw_footer(&mut self, font: Option<&FontRenderer>, source: InputSource) {
        let y = self.height.saturating_sub(117);
        match source {
            InputSource::PlayStation => {
                self.draw_footer_row(font, y, &icons::CROSS, &icons::DPAD_DOWN, &icons::DPAD_UP)
            }
            InputSource::Xbox => self.draw_footer_row(
                font,
                y,
                &icons::XBOX_A,
                &icons::XBOX_DPAD_DOWN,
                &icons::XBOX_DPAD_UP,
            ),
            InputSource::Both => self.draw_footer_row_both(font, y),
            InputSource::None | InputSource::Keyboard => {}
        }
    }

    fn draw_footer_row_both(&mut self, font: Option<&FontRenderer>, y: usize) {
        // Draw PlayStation Cross & Xbox A together for "Select"
        icons::CROSS.draw(self, 30, y);
        icons::XBOX_A.draw(self, 90, y);
        self.draw_label(font, 165, y + 16, "Select");

        // Draw PlayStation Dpad Down & Xbox Dpad Down together for "Down"
        icons::DPAD_DOWN.draw(self, 290, y);
        icons::XBOX_DPAD_DOWN.draw(self, 350, y);
        self.draw_label(font, 425, y + 16, "Down");

        // Draw PlayStation Dpad Up & Xbox Dpad Up together for "Up"
        icons::DPAD_UP.draw(self, 550, y);
        icons::XBOX_DPAD_UP.draw(self, 610, y);
        self.draw_label(font, 685, y + 16, "Up");
    }

    fn draw_footer_row(
        &mut self,
        font: Option<&FontRenderer>,
        y: usize,
        select: &icons::Icon,
        down: &icons::Icon,
        up: &icons::Icon,
    ) {
        select.draw(self, 60, y);
        self.draw_label(font, 140, y + 16, "Select");
        down.draw(self, 330, y);
        self.draw_label(font, 410, y + 16, "Down");
        up.draw(self, 600, y);
        self.draw_label(font, 680, y + 16, "Up");
    }

    fn draw_label(&mut self, font: Option<&FontRenderer>, x: usize, y: usize, text: &str) {
        self.draw_text_line(x, y, 20, text, font);
    }

    fn draw_dimmed_background(&mut self, raw: &[u8], dim_percent: u32) {
        if raw.len() < 8 {
            self.clear();
            return;
        }

        let src_w = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize;
        let src_h = u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]) as usize;
        if src_w == 0 || src_h == 0 {
            self.clear();
            return;
        }

        let pixels = &raw[8..];
        let brightness = 100u32.saturating_sub(dim_percent);

        if self.width * src_h >= self.height * src_w {
            let scaled_h = src_h * self.width / src_w;
            let crop_top = scaled_h.saturating_sub(self.height) / 2;
            for y in 0..self.height {
                let sy = ((y + crop_top) * src_w) / self.width;
                for x in 0..self.width {
                    let sx = (x * src_w) / self.width;
                    self.backbuffer[y * self.width + x] =
                        sample_dimmed_rgba(pixels, src_w, sx, sy, brightness);
                }
            }
        } else {
            let scaled_w = src_w * self.height / src_h;
            let crop_left = scaled_w.saturating_sub(self.width) / 2;
            for y in 0..self.height {
                let sy = (y * src_h) / self.height;
                for x in 0..self.width {
                    let sx = ((x + crop_left) * src_h) / self.height;
                    self.backbuffer[y * self.width + x] =
                        sample_dimmed_rgba(pixels, src_w, sx, sy, brightness);
                }
            }
        }
    }

    pub fn flush(&mut self) {
        for y in 0..self.height {
            for x in 0..self.width {
                let offset = y * self.stride + x * 4;
                unsafe {
                    let p = self.fb.add(offset) as *mut u32;
                    p.write_volatile(self.backbuffer[y * self.width + x]);
                }
            }
        }
    }

    pub(crate) fn fill_rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: u32) {
        for py in y..(y + h).min(self.height) {
            for px in x..(x + w).min(self.width) {
                self.put_pixel(px, py, color);
            }
        }
    }

    fn draw_rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: u32) {
        self.fill_rect(x, y, w, 3, color);
        self.fill_rect(x, y + h.saturating_sub(3), w, 3, color);
        self.fill_rect(x, y, 3, h, color);
        self.fill_rect(x + w.saturating_sub(3), y, 3, h, color);
    }

    fn draw_hline(&mut self, x: usize, y: usize, w: usize, color: u32) {
        self.fill_rect(x, y, w, 2, color);
    }

    fn draw_select_icon(&mut self, cx: usize, cy: usize) {
        self.draw_circle_outline(cx, cy, 22, WHITE);
        self.draw_line(cx - 10, cy - 10, cx + 10, cy + 10, WHITE);
        self.draw_line(cx - 9, cy - 10, cx + 11, cy + 10, WHITE);
        self.draw_line(cx + 10, cy - 10, cx - 10, cy + 10, WHITE);
        self.draw_line(cx + 11, cy - 10, cx - 9, cy + 10, WHITE);
    }

    fn draw_dpad_down_icon(&mut self, x: usize, y: usize) {
        self.draw_dpad_icon(x, y);
        self.fill_rect(x + 14, y + 27, 16, 16, WHITE);
    }

    fn draw_dpad_up_icon(&mut self, x: usize, y: usize) {
        self.draw_dpad_icon(x, y);
        self.fill_rect(x + 14, y, 16, 16, WHITE);
    }

    fn draw_dpad_icon(&mut self, x: usize, y: usize) {
        self.draw_rect(x + 12, y, 20, 44, WHITE);
        self.draw_rect(x, y + 12, 44, 20, WHITE);
        self.fill_rect(x + 16, y + 16, 12, 12, BLACK);
    }

    fn draw_circle_outline(&mut self, cx: usize, cy: usize, r: usize, color: u32) {
        let inner = (r - 3) * (r - 3);
        let outer = r * r;
        for y in cy.saturating_sub(r)..=(cy + r).min(self.height - 1) {
            for x in cx.saturating_sub(r)..=(cx + r).min(self.width - 1) {
                let dx = x.abs_diff(cx);
                let dy = y.abs_diff(cy);
                let d = dx * dx + dy * dy;
                if d >= inner && d <= outer {
                    self.put_pixel(x, y, color);
                }
            }
        }
    }

    pub(crate) fn draw_line(&mut self, x0: usize, y0: usize, x1: usize, y1: usize, color: u32) {
        let mut x0 = x0 as isize;
        let mut y0 = y0 as isize;
        let x1 = x1 as isize;
        let y1 = y1 as isize;
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            if x0 >= 0 && y0 >= 0 {
                self.fill_rect(x0 as usize, y0 as usize, 2, 2, color);
            }
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }

    fn draw_text(&mut self, mut x: usize, y: usize, text: &str, scale: usize, color: u32) {
        for ch in text.chars() {
            if ch == ' ' {
                x += 4 * scale;
                continue;
            }
            self.draw_glyph(x, y, ch, scale, color);
            x += 6 * scale;
        }
    }

    fn draw_glyph(&mut self, x: usize, y: usize, ch: char, scale: usize, color: u32) {
        let glyph = glyph(ch);
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..5 {
                if bits & (1 << (4 - col)) != 0 {
                    self.fill_rect(x + col * scale, y + row * scale, scale, scale, color);
                }
            }
        }
    }

    pub(crate) fn put_pixel(&mut self, x: usize, y: usize, color: u32) {
        if x < self.width && y < self.height {
            self.backbuffer[y * self.width + x] = color;
        }
    }
}

fn format_entry(index: usize, title: &str) -> String {
    let mut s = String::new();
    s.push(char::from(b'0' + index as u8));
    s.push_str(". ");
    s.push_str(title);
    s
}

fn sample_dimmed_rgba(pixels: &[u8], width: usize, x: usize, y: usize, brightness: u32) -> u32 {
    let offset = (y * width + x) * 4;
    if offset + 3 >= pixels.len() {
        return BLACK;
    }

    let r = (u32::from(pixels[offset]) * brightness / 100) as u8;
    let g = (u32::from(pixels[offset + 1]) * brightness / 100) as u8;
    let b = (u32::from(pixels[offset + 2]) * brightness / 100) as u8;
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

fn glyph(ch: char) -> [u8; 7] {
    match ch.to_ascii_uppercase() {
        'A' => [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'B' => [0x1e, 0x11, 0x11, 0x1e, 0x11, 0x11, 0x1e],
        'C' => [0x0e, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0e],
        'D' => [0x1e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1e],
        'E' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f],
        'F' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x10],
        'G' => [0x0e, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0e],
        'H' => [0x11, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'I' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x1f],
        'J' => [0x07, 0x02, 0x02, 0x02, 0x12, 0x12, 0x0c],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1f],
        'M' => [0x11, 0x1b, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        'O' => [0x0e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'P' => [0x1e, 0x11, 0x11, 0x1e, 0x10, 0x10, 0x10],
        'Q' => [0x0e, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0d],
        'R' => [0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11],
        'S' => [0x0f, 0x10, 0x10, 0x0e, 0x01, 0x01, 0x1e],
        'T' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x0a, 0x0a, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x1b, 0x11],
        'X' => [0x11, 0x0a, 0x04, 0x04, 0x04, 0x0a, 0x11],
        'Y' => [0x11, 0x0a, 0x04, 0x04, 0x04, 0x04, 0x04],
        'Z' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1f],
        '0' => [0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e],
        '1' => [0x04, 0x0c, 0x04, 0x04, 0x04, 0x04, 0x0e],
        '2' => [0x0e, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1f],
        '3' => [0x1e, 0x01, 0x01, 0x0e, 0x01, 0x01, 0x1e],
        '4' => [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02],
        '5' => [0x1f, 0x10, 0x10, 0x1e, 0x01, 0x01, 0x1e],
        '6' => [0x0e, 0x10, 0x10, 0x1e, 0x11, 0x11, 0x0e],
        '7' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e],
        '9' => [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x01, 0x0e],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x0c, 0x0c],
        '/' => [0x01, 0x02, 0x02, 0x04, 0x08, 0x08, 0x10],
        '(' => [0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02],
        ')' => [0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08],
        '-' => [0x00, 0x00, 0x00, 0x1f, 0x00, 0x00, 0x00],
        _ => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
    }
}
