// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::ics::{self, Event};
use crate::model::Calendar;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct CalendarData {
    pub calendar: Calendar,
    pub href: String,
    pub sync_token: Option<String>,
    pub resources: BTreeMap<String, Resource>,
}

impl CalendarData {
    pub fn new(calendar: Calendar, href: String) -> CalendarData {
        CalendarData {
            calendar,
            href,
            sync_token: None,
            resources: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Resource {
    pub etag: String,
    pub data: String,
    #[serde(skip)]
    pub events: Arc<[Event]>,
}

impl Resource {
    pub fn new(etag: String, data: String) -> Resource {
        let events = ics::parse(&data).unwrap_or_default().into();
        Resource { etag, data, events }
    }
}

pub(crate) fn load(path: &Path) -> Vec<CalendarData> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut calendars: Vec<CalendarData> = serde_json::from_str(&text).unwrap_or_default();
    for calendar in &mut calendars {
        for resource in calendar.resources.values_mut() {
            resource.events = ics::parse(&resource.data).unwrap_or_default().into();
        }
    }
    calendars
}

pub(crate) fn save(path: &Path, calendars: &[CalendarData]) -> Result<(), String> {
    let text = serde_json::to_string(calendars).map_err(|error| error.to_string())?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    }
    let partial = path.with_extension("partial");
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&partial).map_err(|error| error.to_string())?;
    file.write_all(text.as_bytes())
        .map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    fs::rename(&partial, path).map_err(|error| error.to_string())
}
