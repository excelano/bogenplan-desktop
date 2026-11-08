// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::collections::BTreeMap;

use http::{StatusCode, Uri};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::client::legacy::{Client, connect::HttpConnector};
use hyper_util::rt::TokioExecutor;
use libdav::CalDavClient;
use libdav::caldav::{
    CalendarComponent, FindCalendarHomeSet, FindCalendars, GetCalendarResources,
    GetSupportedComponents,
};
use libdav::dav::{
    FoundCollection, ListResources, PutResource, SyncCollection, WebDavClient, WebDavError,
};
use libdav::names;
use tower_http::auth::AddAuthorization;

use crate::cache::{CalendarData, Resource};
use crate::credentials::Credentials;
use crate::model::{Calendar, CalendarId};

type Http = AddAuthorization<Client<HttpsConnector<HttpConnector>, String>>;
type Dav = CalDavClient<Http>;

const FETCH_BATCH: usize = 100;

pub(crate) struct Connection {
    client: Dav,
    account: String,
}

impl Connection {
    pub async fn open(credentials: &Credentials) -> Result<(Connection, Uri), String> {
        let base = base_uri(&credentials.server)?;
        let http = http(&credentials.username, &credentials.password)?;
        let mut client = CalDavClient::new(WebDavClient::new(base, http));
        let principal = match client.find_current_user_principal().await {
            Ok(Some(principal)) => principal,
            Ok(None) => {
                client =
                    CalDavClient::bootstrap_via_service_discovery(client.webdav_client.clone())
                        .await
                        .map_err(|error| {
                            format!(
                                "{}: no calendar service found ({error})",
                                credentials.server
                            )
                        })?;
                client
                    .find_current_user_principal()
                    .await
                    .map_err(|error| refusal(credentials, &error.to_string()))?
                    .ok_or_else(|| {
                        format!(
                            "{} did not say whose calendars these are",
                            credentials.server
                        )
                    })?
            }
            Err(error) => return Err(refusal(credentials, &error.to_string())),
        };
        let home = client
            .request(FindCalendarHomeSet::new(principal.path()))
            .await
            .map_err(|error| describe(credentials, &error))?
            .home_sets
            .into_iter()
            .next()
            .ok_or_else(|| {
                format!(
                    "{} has no calendars for {}",
                    credentials.server, credentials.username
                )
            })?;
        if let (Some(scheme), Some(authority)) = (home.scheme(), home.authority()) {
            client.webdav_client.base_url = Uri::builder()
                .scheme(scheme.clone())
                .authority(authority.clone())
                .path_and_query("/")
                .build()
                .map_err(|error| error.to_string())?;
        }
        Ok((
            Connection {
                client,
                account: credentials.key(),
            },
            home,
        ))
    }

    pub async fn calendars(&self, home: &Uri) -> Result<Vec<(Calendar, String)>, String> {
        let found = self
            .client
            .request(FindCalendars::new(home.path()))
            .await
            .map_err(|error| error.to_string())?
            .calendars;
        let mut calendars = Vec::new();
        for collection in found {
            if !self.holds_events(&collection.href).await {
                continue;
            }
            calendars.push((self.calendar(&collection), collection.href));
        }
        Ok(calendars)
    }

    async fn holds_events(&self, href: &str) -> bool {
        match self.client.request(GetSupportedComponents::new(href)).await {
            Ok(response) => response.components.contains(&CalendarComponent::VEvent),
            Err(_) => true,
        }
    }

    fn calendar(&self, collection: &FoundCollection) -> Calendar {
        let property = |name: &libdav::PropertyName| {
            collection
                .properties
                .iter()
                .find(|(found, _)| *found == name)
                .and_then(|(_, value)| value.clone())
        };
        Calendar {
            id: CalendarId(format!("{} {}", self.account, collection.href)),
            title: property(&names::DISPLAY_NAME).unwrap_or_else(|| collection.href.clone()),
            color: property(&names::CALENDAR_COLOUR)
                .and_then(|hex| color(&hex))
                .unwrap_or([0x8F, 0xA3, 0xBF]),
            account: self.account.clone(),
            order: property(&names::CALENDAR_ORDER)
                .and_then(|order| order.parse().ok())
                .unwrap_or(0),
        }
    }

    pub async fn sync(
        &self,
        calendar: Calendar,
        href: String,
        previous: Option<CalendarData>,
    ) -> Result<CalendarData, String> {
        let mut data =
            previous.unwrap_or_else(|| CalendarData::new(calendar.clone(), href.clone()));
        data.calendar = calendar;
        let changed = match self.sync_changes(&mut data).await {
            Some(changed) => changed,
            None => self.full_listing(&mut data).await?,
        };
        self.fetch(&mut data, changed).await?;
        Ok(data)
    }

    async fn sync_changes(&self, data: &mut CalendarData) -> Option<Vec<String>> {
        let token = data.sync_token.clone()?;
        let response = self
            .client
            .request(SyncCollection::new(&data.href).with_sync_token(&token))
            .await
            .ok()?;
        for deleted in &response.deleted {
            data.resources.remove(deleted);
        }
        let changed = response
            .changed
            .into_iter()
            .filter(|listed| {
                listed.href.ends_with(".ics")
                    || listed.content_type.as_deref().is_some_and(is_calendar)
            })
            .filter(|listed| {
                data.resources
                    .get(&listed.href)
                    .map(|resource| &resource.etag)
                    != listed.etag.as_ref()
            })
            .map(|listed| listed.href)
            .collect();
        data.sync_token = Some(response.new_sync_token);
        Some(changed)
    }

    async fn full_listing(&self, data: &mut CalendarData) -> Result<Vec<String>, String> {
        let (listed, token) = match self.client.request(SyncCollection::new(&data.href)).await {
            Ok(response) => (response.changed, Some(response.new_sync_token)),
            Err(_) => (
                self.client
                    .request(ListResources::new(&data.href))
                    .await
                    .map_err(|error| error.to_string())?
                    .resources,
                None,
            ),
        };
        let listed: BTreeMap<String, Option<String>> = listed
            .into_iter()
            .filter(|listed| {
                listed.href.ends_with(".ics")
                    || listed.content_type.as_deref().is_some_and(is_calendar)
            })
            .map(|listed| (listed.href, listed.etag))
            .collect();
        data.resources.retain(|href, _| listed.contains_key(href));
        data.sync_token = token;
        Ok(listed
            .into_iter()
            .filter(|(href, etag)| {
                data.resources.get(href).map(|resource| &resource.etag) != etag.as_ref()
            })
            .map(|(href, _)| href)
            .collect())
    }

    async fn fetch(&self, data: &mut CalendarData, hrefs: Vec<String>) -> Result<(), String> {
        for batch in hrefs.chunks(FETCH_BATCH) {
            let fetched = self
                .client
                .request(GetCalendarResources::new(&data.href).with_hrefs(batch))
                .await
                .map_err(|error| error.to_string())?;
            for resource in fetched.resources {
                match resource.content {
                    Ok(content) => {
                        data.resources
                            .insert(resource.href, Resource::new(content.etag, content.data));
                    }
                    Err(StatusCode::NOT_FOUND) => {
                        data.resources.remove(&resource.href);
                    }
                    Err(_) => {}
                }
            }
        }
        Ok(())
    }

    pub async fn put(
        &self,
        calendar_href: &str,
        name: &str,
        ics: String,
    ) -> Result<(String, Option<String>), String> {
        let href = format!(
            "{}{name}.ics",
            if calendar_href.ends_with('/') {
                calendar_href.to_string()
            } else {
                format!("{calendar_href}/")
            }
        );
        let response = self
            .client
            .request(PutResource::new(&href).create(ics, "text/calendar; charset=utf-8"))
            .await
            .map_err(|error| match error {
                WebDavError::BadStatusCode(StatusCode::FORBIDDEN) => {
                    "This calendar does not accept new events.".to_string()
                }
                other => other.to_string(),
            })?;
        Ok((href, response.etag))
    }
}

fn is_calendar(content_type: &str) -> bool {
    content_type.starts_with("text/calendar")
}

fn base_uri(server: &str) -> Result<Uri, String> {
    let url = if server.contains("://") {
        server.to_string()
    } else {
        format!("https://{server}")
    };
    url.parse()
        .map_err(|_| format!("{server} is not a server address"))
}

fn http(username: &str, password: &str) -> Result<Http, String> {
    let connector = HttpsConnectorBuilder::new()
        .with_native_roots()
        .map_err(|error| format!("no trusted certificates on this system: {error}"))?
        .https_or_http()
        .enable_http1()
        .build();
    Ok(AddAuthorization::basic(
        Client::builder(TokioExecutor::new()).build(connector),
        username,
        password,
    ))
}

fn refusal(credentials: &Credentials, error: &str) -> String {
    if error.contains("401") {
        format!(
            "{} did not accept the password for {}",
            credentials.server, credentials.username
        )
    } else {
        format!("{}: {error}", credentials.server)
    }
}

fn describe<E: std::fmt::Display>(credentials: &Credentials, error: &E) -> String {
    refusal(credentials, &error.to_string())
}

fn color(hex: &str) -> Option<[u8; 3]> {
    let hex = hex.trim().trim_start_matches('#');
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_drop_their_alpha() {
        assert_eq!(color("#0E61B9FF"), Some([0x0E, 0x61, 0xB9]));
        assert_eq!(color("#FFCC00"), Some([0xFF, 0xCC, 0x00]));
        assert_eq!(color("blue"), None);
    }

    #[test]
    fn a_bare_host_is_https() {
        assert_eq!(
            base_uri("caldav.icloud.com").unwrap(),
            "https://caldav.icloud.com"
        );
        assert_eq!(
            base_uri("http://localhost:5232/").unwrap(),
            "http://localhost:5232/"
        );
    }
}
