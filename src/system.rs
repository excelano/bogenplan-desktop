// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::time::Duration;

use zbus::zvariant::OwnedValue;
use zbus::{Connection, Proxy};

use crate::civil::Clock;


fn block_on<T>(work: impl Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime for the desktop bus")
        .block_on(work)
}

pub fn clock() -> Clock {
    let format = block_on(async {
        tokio::time::timeout(Duration::from_secs(1), interface_setting("clock-format"))
            .await
            .ok()
            .flatten()
    });
    match format.as_deref() {
        Some("12h") => Clock::TwelveHour,
        Some(_) => Clock::TwentyFourHour,
        None => Clock::TwentyFourHour,
    }
}

async fn interface_setting(key: &str) -> Option<String> {
    let connection = Connection::session().await.ok()?;
    let settings = Proxy::new(
        &connection,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Settings",
    )
    .await
    .ok()?;
    let value: OwnedValue = settings
        .call("ReadOne", &("org.gnome.desktop.interface", key))
        .await
        .ok()?;
    String::try_from(value).ok()
}
