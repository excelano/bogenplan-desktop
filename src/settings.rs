// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::display::{Precision, Scale};
use crate::place::Place;

pub const STANDARD_POINTS_PER_SPAN: f32 = 4.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub scale: Scale,
    pub precision: Precision,
    pub shows_civil_time: bool,
    pub points_per_span: f32,
    pub hidden_calendars: BTreeSet<String>,
    pub moment_calendar: Option<String>,
    pub place: Option<Place>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            scale: Scale::Spans,
            precision: Precision::Medium,
            shows_civil_time: true,
            points_per_span: STANDARD_POINTS_PER_SPAN,
            hidden_calendars: BTreeSet::new(),
            moment_calendar: None,
            place: None,
        }
    }
}

pub fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_default()
        .join("bogenplan")
}

pub fn cache_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_default()
        .join("bogenplan")
}

impl Settings {
    pub fn load() -> Settings {
        fs::read_to_string(config_dir().join("settings.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let dir = config_dir();
        if fs::create_dir_all(&dir).is_ok()
            && let Ok(text) = serde_json::to_string_pretty(self)
        {
            let mut options = fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
            let path = dir.join("settings.json");
            let partial = path.with_extension("partial");
            if let Ok(mut file) = options.open(&partial)
                && file.write_all(text.as_bytes()).is_ok()
            {
                let _ = fs::rename(&partial, &path);
            }
        }
    }
}
