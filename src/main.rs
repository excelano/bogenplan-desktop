// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

#![deny(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod accounts;
mod allday;
mod app;
mod axis;
mod civil;
mod day;
mod demo;
mod detail;
mod display;
mod editor;
mod entry;
mod fields;
mod glyph;
mod import;
mod instance;
mod layout;
mod moment;
mod palette;
mod place;
#[cfg(feature = "screenshot")]
mod screenshot;
mod settings;
mod system;
mod time;
mod week;

use std::path::PathBuf;

use bogenplan_store::Store;
use chrono::NaiveDate;
use eframe::egui::{self, Color32, CornerRadius, Stroke, Visuals};

use crate::app::{App, Source};

const APP_ID: &str = "bogenplan";
const PRODUCT: &str = "Bogenplan";

fn argument(flag: &str) -> Option<String> {
    let arguments: Vec<String> = std::env::args().collect();
    arguments
        .iter()
        .position(|argument| argument == flag)
        .and_then(|i| arguments.get(i + 1).cloned())
}

const FLAGS_WITH_VALUES: [&str; 2] = ["--date", "--zoom"];

fn files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        if FLAGS_WITH_VALUES.contains(&argument.as_str()) {
            arguments.next();
        } else if !argument.starts_with("--") {
            let file = PathBuf::from(argument);
            files.push(std::path::absolute(&file).unwrap_or(file));
        }
    }
    files
}

fn visuals() -> Visuals {
    let mut visuals = Visuals::dark();
    visuals.panel_fill = palette::NIGHT;
    visuals.window_fill = palette::ASTRONOMICAL_TWILIGHT;
    visuals.extreme_bg_color = Color32::from_rgb(0x07, 0x0D, 0x19);
    visuals.faint_bg_color = palette::ASTRONOMICAL_TWILIGHT;
    visuals.window_stroke = Stroke::new(1.0, palette::NAUTICAL_TWILIGHT);
    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.override_text_color = Some(palette::LINE);
    visuals.hyperlink_color = palette::SUN;
    visuals.selection.bg_fill = palette::with_opacity(palette::SUN, 0.35);
    visuals.selection.stroke = Stroke::new(1.0, palette::SUN);
    visuals.widgets.inactive.weak_bg_fill = palette::NAUTICAL_TWILIGHT;
    visuals.widgets.inactive.bg_fill = palette::NAUTICAL_TWILIGHT;
    visuals.widgets.hovered.weak_bg_fill = palette::CIVIL_TWILIGHT;
    visuals.widgets.hovered.bg_fill = palette::CIVIL_TWILIGHT;
    visuals.widgets.active.weak_bg_fill = palette::DAY;
    visuals.widgets.active.bg_fill = palette::DAY;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, palette::NAUTICAL_TWILIGHT);
    visuals
}

fn main() -> eframe::Result {
    let demo = std::env::args().any(|argument| argument == "--demo");
    let opening =
        argument("--date").and_then(|text| NaiveDate::parse_from_str(&text, "%Y-%m-%d").ok());
    let zoom = argument("--zoom").and_then(|text| text.parse::<f32>().ok());
    let opened = if demo {
        None
    } else {
        match instance::claim(files()) {
            Some(opened) => Some(opened),
            None => return Ok(()),
        }
    };

    let viewport = egui::ViewportBuilder::default()
        .with_app_id(APP_ID)
        .with_title(PRODUCT)
        .with_inner_size([1100.0, 800.0])
        .with_min_inner_size([420.0, 480.0]);
    #[cfg(feature = "screenshot")]
    let viewport = match screenshot::Screenshot::size() {
        Some(size) => viewport.with_inner_size(size),
        None => viewport,
    };
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        PRODUCT,
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(visuals());
            let source = if demo {
                Source::Demo(demo::Demo)
            } else {
                let ctx = cc.egui_ctx.clone();
                Source::Live(Store::open(settings::cache_dir(), move || {
                    ctx.request_repaint()
                }))
            };
            let mut app = App::new(source, opening);
            if let Some(opened) = opened {
                let _ = opened.window.set(cc.egui_ctx.clone());
                app.receive_files(opened.files);
            }
            if let Some(zoom) = zoom {
                app.set_zoom(zoom);
            }
            #[cfg(feature = "screenshot")]
            if let Some(opening) = screenshot::Screenshot::opening() {
                app.open_for_screenshot(&opening);
            }
            Ok(Box::new(app))
        }),
    )
}
