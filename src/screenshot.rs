// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use eframe::egui::{self, ColorImage, Context, UserData, ViewportCommand};

pub struct Screenshot {
    path: PathBuf,
    settle: f64,
    requested: bool,
}

impl Screenshot {
    pub fn size() -> Option<[f32; 2]> {
        let text = std::env::var("BOGENPLAN_SCREENSHOT_SIZE").ok()?;
        let (width, height) = text.split_once('x')?;
        Some([width.parse().ok()?, height.parse().ok()?])
    }

    pub fn opening() -> Option<String> {
        std::env::var("BOGENPLAN_SCREENSHOT_OPEN").ok()
    }

    pub fn from_environment() -> Option<Screenshot> {
        let settle = std::env::var("BOGENPLAN_SCREENSHOT_DELAY")
            .ok()
            .and_then(|text| text.parse().ok())
            .unwrap_or(1.0);
        std::env::var_os("BOGENPLAN_SCREENSHOT").map(|path| Screenshot {
            path: path.into(),
            settle,
            requested: false,
        })
    }

    pub fn tick(&mut self, ctx: &Context) {
        let image = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            let _ = save(&self.path, &image);
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }
        if ctx.input(|input| input.time) >= self.settle && !self.requested {
            self.requested = true;
            ctx.send_viewport_cmd(ViewportCommand::Screenshot(UserData::default()));
        }
        ctx.request_repaint();
    }
}

fn save(path: &PathBuf, image: &ColorImage) -> Result<(), Box<dyn std::error::Error>> {
    let mut encoder = png::Encoder::new(
        BufWriter::new(File::create(path)?),
        image.size[0] as u32,
        image.size[1] as u32,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let pixels: Vec<u8> = image
        .pixels
        .iter()
        .flat_map(|pixel| pixel.to_array())
        .collect();
    encoder.write_header()?.write_image_data(&pixels)?;
    Ok(())
}
