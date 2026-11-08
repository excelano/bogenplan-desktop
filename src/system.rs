// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::sync::mpsc;
use std::time::Duration;

use futures_util::StreamExt;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::{Connection, Proxy};

use crate::civil::Clock;
use crate::place::Place;

const GEOCLUE: &str = "org.freedesktop.GeoClue2";
const CITY_ACCURACY: u32 = 4;
const LOCATE_TIMEOUT: Duration = Duration::from_secs(30);

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

pub fn locate() -> mpsc::Receiver<Result<Place, String>> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let found = block_on(async {
            tokio::time::timeout(LOCATE_TIMEOUT, geoclue())
                .await
                .unwrap_or_else(|_| Err("GeoClue did not answer in time.".to_string()))
        });
        let _ = sender.send(found);
    });
    receiver
}

async fn geoclue() -> Result<Place, String> {
    let unavailable = |error: zbus::Error| format!("GeoClue is not available: {error}");
    let connection = Connection::system().await.map_err(unavailable)?;
    let manager = Proxy::new(
        &connection,
        GEOCLUE,
        "/org/freedesktop/GeoClue2/Manager",
        "org.freedesktop.GeoClue2.Manager",
    )
    .await
    .map_err(unavailable)?;
    let path: OwnedObjectPath = manager.call("GetClient", &()).await.map_err(unavailable)?;
    let client = Proxy::new(
        &connection,
        GEOCLUE,
        path,
        "org.freedesktop.GeoClue2.Client",
    )
    .await
    .map_err(unavailable)?;
    client
        .set_property("DesktopId", "bogenplan")
        .await
        .map_err(|error| error.to_string())?;
    client
        .set_property("RequestedAccuracyLevel", CITY_ACCURACY)
        .await
        .map_err(|error| error.to_string())?;
    let mut updates = client
        .receive_signal("LocationUpdated")
        .await
        .map_err(unavailable)?;
    client
        .call_method("Start", &())
        .await
        .map_err(|error| format!("Location Services refused Bogenplan: {error}"))?;
    let message = updates
        .next()
        .await
        .ok_or("GeoClue stopped without a location.")?;
    let (_, location): (OwnedObjectPath, OwnedObjectPath) = message
        .body()
        .deserialize()
        .map_err(|error| error.to_string())?;
    let location = Proxy::new(
        &connection,
        GEOCLUE,
        location,
        "org.freedesktop.GeoClue2.Location",
    )
    .await
    .map_err(unavailable)?;
    let latitude: f64 = location
        .get_property("Latitude")
        .await
        .map_err(|error| error.to_string())?;
    let longitude: f64 = location
        .get_property("Longitude")
        .await
        .map_err(|error| error.to_string())?;
    let _ = client.call_method("Stop", &()).await;
    Ok(Place {
        latitude,
        longitude,
    })
}
