// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use eframe::egui::Color32;

pub const NIGHT: Color32 = Color32::from_rgb(0x0A, 0x12, 0x22);
pub const ASTRONOMICAL_TWILIGHT: Color32 = Color32::from_rgb(0x12, 0x1F, 0x38);
pub const NAUTICAL_TWILIGHT: Color32 = Color32::from_rgb(0x1B, 0x2D, 0x50);
pub const CIVIL_TWILIGHT: Color32 = Color32::from_rgb(0x2A, 0x44, 0x70);
pub const DAY: Color32 = Color32::from_rgb(0x33, 0x59, 0x8A);
pub const LINE: Color32 = Color32::from_rgb(0xF2, 0xF5, 0xFA);
pub const MAJOR_TICK: Color32 = Color32::from_rgb(0xDD, 0xE6, 0xF2);
pub const SUN: Color32 = Color32::from_rgb(0xFF, 0xE2, 0xA0);
pub const LABEL: Color32 = Color32::from_rgb(0xA9, 0xB8, 0xCC);

pub fn with_opacity(color: Color32, opacity: f32) -> Color32 {
    color.gamma_multiply(opacity)
}

pub fn calendar(rgb: [u8; 3]) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}
