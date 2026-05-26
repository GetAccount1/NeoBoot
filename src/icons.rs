pub struct Icon {
    pub width: usize,
    pub height: usize,
    data: &'static [u8],
}

impl Icon {
    pub const fn new(data: &'static [u8]) -> Self {
        let width = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let height = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
        Self {
            width,
            height,
            data,
        }
    }

    pub fn draw(&self, ui: &mut crate::ui::UI, x: usize, y: usize) {
        let mut i = 8;
        for py in 0..self.height {
            for px in 0..self.width {
                let r = self.data[i];
                let g = self.data[i + 1];
                let b = self.data[i + 2];
                let a = self.data[i + 3];
                i += 4;
                if a > 128 {
                    let color = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
                    ui.put_pixel(x + px, y + py, color);
                }
            }
        }
    }
}

pub static CROSS: Icon = Icon::new(include_bytes!("icons/playstation_button_cross.raw"));
pub static CIRCLE: Icon = Icon::new(include_bytes!("icons/playstation_button_circle.raw"));
pub static DPAD_DOWN: Icon = Icon::new(include_bytes!("icons/playstation_dpad_down_outline.raw"));
pub static DPAD_UP: Icon = Icon::new(include_bytes!("icons/playstation_dpad_up_outline.raw"));
pub static XBOX_A: Icon = Icon::new(include_bytes!("icons/xbox_button_a.raw"));
pub static XBOX_B: Icon = Icon::new(include_bytes!("icons/xbox_button_b.raw"));
pub static XBOX_DPAD_DOWN: Icon = Icon::new(include_bytes!("icons/xbox_dpad_down.raw"));
pub static XBOX_DPAD_UP: Icon = Icon::new(include_bytes!("icons/xbox_dpad_up.raw"));
pub static STORAGE: Icon = Icon::new(include_bytes!("icons/storage_icon.raw"));
