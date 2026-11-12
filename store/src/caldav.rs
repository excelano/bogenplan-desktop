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
    Delete, FindCollections, FindPropertyHrefs, FoundCollection, GetEtag, ListResources,
    PutResource, SyncCollection, WebDavClient, WebDavError,
};
use libdav::{PropertyName, names};
use tower_http::auth::AddAuthorization;

use crate::cache::{CalendarData, Resource};
use crate::credentials::Credentials;
use crate::model::{Calendar, CalendarId, parse_hex};

type Http = AddAuthorization<Client<HttpsConnector<HttpConnector>, String>>;
type Dav = CalDavClient<Http>;

const FETCH_BATCH: usize = 100;

const CALENDARSERVER: &str = "http://calendarserver.org/ns/";
const SUBSCRIBED: PropertyName = PropertyName::new(CALENDARSERVER, "subscribed");
const SOURCE: PropertyName = PropertyName::new(CALENDARSERVER, "source");

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
            calendars.push((self.calendar(&collection, false), collection.href));
        }
        Ok(calendars)
    }

    pub async fn subscriptions(&self, home: &Uri) -> Result<Vec<(Calendar, String)>, String> {
        let found = self
            .client
            .request(
                FindCollections::new(home.path())
                    .with_collection_type(&SUBSCRIBED)
                    .with_additional_properties(&[
                        &names::DISPLAY_NAME,
                        &names::CALENDAR_COLOUR,
                        &names::CALENDAR_ORDER,
                    ]),
            )
            .await
            .map_err(|error| error.to_string())?
            .collections;
        Ok(found
            .into_iter()
            .map(|collection| (self.calendar(&collection, true), collection.href))
            .collect())
    }

    pub async fn source(&self, subscription: &str) -> Result<String, String> {
        self.client
            .request(FindPropertyHrefs::new(subscription, &SOURCE))
            .await
            .map_err(|error| error.to_string())?
            .hrefs
            .into_iter()
            .next()
            .map(|source| source.to_string())
            .ok_or_else(|| "the subscription has no address".to_string())
    }

    async fn holds_events(&self, href: &str) -> bool {
        match self.client.request(GetSupportedComponents::new(href)).await {
            Ok(response) => response.components.contains(&CalendarComponent::VEvent),
            Err(_) => true,
        }
    }

    fn calendar(&self, collection: &FoundCollection, read_only: bool) -> Calendar {
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
                .and_then(|hex| parse_hex(&hex))
                .unwrap_or([0x8F, 0xA3, 0xBF]),
            account: self.account.clone(),
            order: property(&names::CALENDAR_ORDER)
                .and_then(|order| order.parse().ok())
                .unwrap_or(0),
            read_only,
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
        let etag = match response.etag {
            Some(etag) => Some(etag),
            None => self.etag(&href).await,
        };
        Ok((href, etag))
    }

    pub async fn update(&self, href: &str, ics: String, etag: &str) -> Result<String, String> {
        let response = self
            .client
            .request(PutResource::new(href).update(ics, "text/calendar; charset=utf-8", etag))
            .await
            .map_err(write_error)?;
        Ok(match response.etag {
            Some(etag) => etag,
            None => self.etag(href).await.unwrap_or_default(),
        })
    }

    pub async fn delete(&self, href: &str, etag: &str) -> Result<(), String> {
        self.client
            .request(Delete::new(href).with_etag(etag))
            .await
            .map(|_| ())
            .map_err(write_error)
    }

    async fn etag(&self, href: &str) -> Option<String> {
        self.client
            .request(GetEtag::new(href))
            .await
            .ok()
            .map(|response| response.etag)
    }
}

fn write_error<E: std::fmt::Display>(error: WebDavError<E>) -> String {
    match error {
        WebDavError::BadStatusCode(StatusCode::FORBIDDEN) => {
            "This calendar does not accept changes.".to_string()
        }
        WebDavError::BadStatusCode(StatusCode::PRECONDITION_FAILED) => {
            "The event changed on the server since the last sync. Refresh and try again."
                .to_string()
        }
        other => {
            let text = other.to_string();
            if text.contains("412") || text.to_ascii_lowercase().contains("precondition") {
                "The event changed on the server since the last sync. Refresh and try again."
                    .to_string()
            } else {
                text
            }
        }
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
    let uri: Uri = url
        .parse()
        .map_err(|_| format!("{server} is not a server address"))?;
    if uri.scheme_str() == Some("http") && !is_loopback(uri.host().unwrap_or_default()) {
        return Err(format!(
            "{server} would send the password unencrypted; use https://"
        ));
    }
    Ok(uri)
}

fn is_loopback(host: &str) -> bool {
    host == "localhost"
        || host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(base_uri("http://127.0.0.1:5232/").is_ok());
        assert!(base_uri("http://[::1]:5232/").is_ok());
        assert!(base_uri("http://caldav.example.com/").is_err());
    }

    #[test]
    #[ignore = "needs BOGENPLAN_TEST_SERVER, BOGENPLAN_TEST_USERNAME and BOGENPLAN_TEST_PASSWORD"]
    fn a_server_takes_a_moment_through_its_whole_life() {
        let variable = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name}"));
        let credentials = Credentials {
            server: variable("BOGENPLAN_TEST_SERVER"),
            username: variable("BOGENPLAN_TEST_USERNAME"),
            password: variable("BOGENPLAN_TEST_PASSWORD"),
        };
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let (connection, home) = Connection::open(&credentials).await.unwrap();
                let (calendar, href) = connection
                    .calendars(&home)
                    .await
                    .unwrap()
                    .into_iter()
                    .next()
                    .expect("a calendar on the server");
                let data = connection
                    .sync(calendar.clone(), href.clone(), None)
                    .await
                    .unwrap();

                let uid = uuid::Uuid::new_v4().to_string().to_uppercase();
                let now = chrono::Utc::now();
                let text = crate::moment::ics(&uid, now, "Bogenplan test", now);
                let (resource, etag) = connection.put(&href, &uid, text.clone()).await.unwrap();
                let data = connection
                    .sync(calendar.clone(), href.clone(), Some(data))
                    .await
                    .unwrap();
                let synced = &data.resources[&resource];
                assert_eq!(synced.events[0].uid, uid);
                let etag = etag.unwrap_or_else(|| synced.etag.clone());

                let renamed = text.replace("Bogenplan test", "Bogenplan test renamed");
                let newer = connection
                    .update(&resource, renamed.clone(), &etag)
                    .await
                    .unwrap();
                let stale = connection.update(&resource, renamed, &etag).await;
                assert!(stale.unwrap_err().contains("changed on the server"));

                connection.delete(&resource, &newer).await.unwrap();
                let data = connection.sync(calendar, href, Some(data)).await.unwrap();
                assert!(!data.resources.contains_key(&resource));
            });
    }
}
