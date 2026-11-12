// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::time::Duration;

use http::{Request, StatusCode, Uri, header};
use http_body_util::{BodyExt, Limited};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::client::legacy::{Client, connect::HttpConnector};
use hyper_util::rt::TokioExecutor;

type Http = Client<HttpsConnector<HttpConnector>, String>;

const REDIRECTS: usize = 5;
const LARGEST: usize = 32 * 1024 * 1024;
const PATIENCE: Duration = Duration::from_secs(60);

pub(crate) struct Feeds {
    client: Http,
}

impl Feeds {
    pub fn new() -> Result<Feeds, String> {
        let connector = HttpsConnectorBuilder::new()
            .with_native_roots()
            .map_err(|error| format!("no trusted certificates on this system: {error}"))?
            .https_only()
            .enable_http1()
            .build();
        Ok(Feeds {
            client: Client::builder(TokioExecutor::new()).build(connector),
        })
    }

    pub async fn fetch(&self, source: &str) -> Result<String, String> {
        tokio::time::timeout(PATIENCE, self.follow(secure(source)?))
            .await
            .map_err(|_| "the feed did not answer".to_string())?
    }

    async fn follow(&self, mut uri: Uri) -> Result<String, String> {
        for _ in 0..=REDIRECTS {
            let request = Request::get(&uri)
                .header(
                    header::USER_AGENT,
                    concat!("Bogenplan/", env!("CARGO_PKG_VERSION")),
                )
                .header(header::ACCEPT, "text/calendar, */*;q=0.5")
                .body(String::new())
                .map_err(|error| error.to_string())?;
            let response = self
                .client
                .request(request)
                .await
                .map_err(|error| format!("the feed could not be reached ({error})"))?;
            let status = response.status();
            if status.is_redirection() {
                let location = response
                    .headers()
                    .get(header::LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .ok_or("the feed moved without saying where")?;
                uri = resolve(&uri, location)?;
                continue;
            }
            if status != StatusCode::OK {
                return Err(format!("the feed answered {status}"));
            }
            let body = Limited::new(response.into_body(), LARGEST)
                .collect()
                .await
                .map_err(|error| format!("the feed could not be read ({error})"))?
                .to_bytes();
            let text = String::from_utf8_lossy(&body);
            let text = text.trim_start_matches('\u{feff}').trim_start();
            if !text.starts_with("BEGIN:VCALENDAR") {
                return Err("the feed is not a calendar".to_string());
            }
            return Ok(text.to_string());
        }
        Err("the feed redirected too many times".to_string())
    }
}

pub(crate) fn secure(source: &str) -> Result<Uri, String> {
    let (scheme, rest) = source
        .split_once("://")
        .ok_or_else(|| format!("{source} is not a feed address"))?;
    if !["webcal", "webcals", "http", "https"].contains(&scheme.to_ascii_lowercase().as_str()) {
        return Err(format!("{source} is not a feed address"));
    }
    format!("https://{rest}")
        .parse()
        .map_err(|_| format!("{source} is not a feed address"))
}

pub(crate) fn name(source: &str) -> String {
    let Ok(uri) = secure(source) else {
        return source.to_string();
    };
    let file = uri.path().rsplit('/').next().unwrap_or_default();
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    if stem.is_empty() {
        uri.host().unwrap_or(source).to_string()
    } else {
        stem.to_string()
    }
}

fn resolve(from: &Uri, location: &str) -> Result<Uri, String> {
    if location.contains("://") {
        let uri: Uri = location
            .parse()
            .map_err(|_| "the feed moved to an address that is not one".to_string())?;
        if uri.scheme_str() != Some("https") {
            return Err("the feed moved to an unencrypted address".to_string());
        }
        return Ok(uri);
    }
    if !location.starts_with('/') {
        return Err("the feed moved to an address it did not give in full".to_string());
    }
    let mut parts = from.clone().into_parts();
    parts.path_and_query = Some(
        location
            .parse()
            .map_err(|_| "the feed moved to an address that is not one".to_string())?,
    );
    Uri::from_parts(parts).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_feed_is_fetched_over_https() {
        assert_eq!(
            secure("webcal://example.com/a.ics").unwrap(),
            "https://example.com/a.ics"
        );
        assert_eq!(
            secure("http://example.com/a.ics?x=1%2f").unwrap(),
            "https://example.com/a.ics?x=1%2f"
        );
        assert_eq!(
            secure("HTTPS://example.com/a.ics").unwrap(),
            "https://example.com/a.ics"
        );
        assert!(secure("ftp://example.com/a.ics").is_err());
        assert!(secure("example.com/a.ics").is_err());
    }

    #[test]
    fn a_feed_without_a_name_is_named_by_its_file() {
        assert_eq!(
            name("webcal://example.com/caldata/USHolidays.ics"),
            "USHolidays"
        );
        assert_eq!(name("https://example.com/feed?id=4"), "feed");
        assert_eq!(name("https://example.com/"), "example.com");
    }

    #[test]
    fn redirects_resolve_against_the_feed() {
        let from: Uri = "https://example.com/old/a.ics".parse().unwrap();
        assert_eq!(
            resolve(&from, "/new/a.ics?y=2").unwrap(),
            "https://example.com/new/a.ics?y=2"
        );
        assert_eq!(
            resolve(&from, "https://other.example/a.ics").unwrap(),
            "https://other.example/a.ics"
        );
        assert!(resolve(&from, "a.ics").is_err());
        assert!(resolve(&from, "http://other.example/a.ics").is_err());
    }
}
