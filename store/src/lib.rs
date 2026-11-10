// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

#![forbid(unsafe_code)]

mod cache;
mod caldav;
mod credentials;
mod edit;
mod expand;
mod feed;
mod ics;
mod model;
mod moment;
mod zones;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock, mpsc as reply};
use std::time::Duration;

use chrono::{DateTime, Datelike, TimeZone, Utc};
use tokio::sync::mpsc;

pub use model::{Account, Calendar, CalendarId, Change, Occurrence, OccurrenceTime, Scope, Status};

use cache::{CalendarData, Resource};
use caldav::Connection;
use credentials::Credentials;
use feed::Feeds;

const SYNC_EVERY: Duration = Duration::from_secs(300);
const FETCH_FEEDS_EVERY: chrono::Duration = chrono::Duration::hours(1);

pub type Reply = reply::Receiver<Result<(), String>>;

pub trait CalendarSource {
    fn calendars(&self) -> Vec<Calendar>;
    fn occurrences(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<Occurrence>;
    fn add_moment(&self, at: DateTime<Utc>, title: String, calendar: &CalendarId) -> Reply;
    fn change(&self, occurrence: &Occurrence, change: Change, scope: Scope) -> Reply;
    fn delete(&self, occurrence: &Occurrence, scope: Scope) -> Reply;
}

pub struct Store {
    shared: Arc<Shared>,
    commands: mpsc::UnboundedSender<Command>,
}

struct Shared {
    published: RwLock<Arc<Published>>,
    years: Mutex<HashMap<i32, Arc<Vec<Occurrence>>>>,
    status: RwLock<Status>,
    notify: Box<dyn Fn() + Send + Sync>,
}

#[derive(Default)]
struct Published {
    accounts: Vec<Account>,
    calendars: Vec<Calendar>,
    resources: Vec<(CalendarId, String, Arc<[ics::Event]>)>,
}

enum Command {
    Refresh,
    AddMoment {
        at: DateTime<Utc>,
        title: String,
        calendar: CalendarId,
        reply: reply::Sender<Result<(), String>>,
    },
    Change {
        occurrence: Occurrence,
        change: Change,
        scope: Scope,
        reply: reply::Sender<Result<(), String>>,
    },
    Delete {
        occurrence: Occurrence,
        scope: Scope,
        reply: reply::Sender<Result<(), String>>,
    },
    SignIn {
        credentials: Credentials,
        reply: reply::Sender<Result<(), String>>,
    },
    SignOut {
        account: Account,
    },
}

impl Store {
    pub fn open(cache_dir: PathBuf, notify: impl Fn() + Send + Sync + 'static) -> Store {
        let shared = Arc::new(Shared {
            published: RwLock::new(Arc::default()),
            years: Mutex::new(HashMap::new()),
            status: RwLock::new(Status::Syncing),
            notify: Box::new(notify),
        });
        let (commands, receiver) = mpsc::unbounded_channel();
        let worker = Worker {
            shared: Arc::clone(&shared),
            path: cache_dir.join("calendars.json"),
            calendars: Vec::new(),
            accounts: Vec::new(),
            feeds: Feeds::new(),
        };
        std::thread::Builder::new()
            .name("calendar sync".into())
            .spawn(move || {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("a runtime for calendar sync")
                    .block_on(worker.run(receiver));
            })
            .expect("a thread for calendar sync");
        Store { shared, commands }
    }

    pub fn status(&self) -> Status {
        self.shared.status.read().expect("status lock").clone()
    }

    pub fn accounts(&self) -> Vec<Account> {
        self.shared
            .published
            .read()
            .expect("published lock")
            .accounts
            .clone()
    }

    pub fn refresh(&self) {
        let _ = self.commands.send(Command::Refresh);
    }

    pub fn sign_in(&self, server: String, username: String, password: String) -> Reply {
        let (sender, receiver) = reply::channel();
        let credentials = Credentials {
            server,
            username,
            password,
        };
        let _ = self.commands.send(Command::SignIn {
            credentials,
            reply: sender,
        });
        receiver
    }

    pub fn sign_out(&self, account: Account) {
        let _ = self.commands.send(Command::SignOut { account });
    }

    fn year(&self, year: i32) -> Arc<Vec<Occurrence>> {
        if let Some(found) = self.shared.years.lock().expect("years lock").get(&year) {
            return Arc::clone(found);
        }
        let from = Utc
            .with_ymd_and_hms(year, 1, 1, 0, 0, 0)
            .single()
            .expect("new year exists");
        let to = Utc
            .with_ymd_and_hms(year + 1, 1, 1, 0, 0, 0)
            .single()
            .expect("new year exists");
        let published = Arc::clone(&self.shared.published.read().expect("published lock"));
        let mut found = Vec::new();
        for (calendar, href, events) in &published.resources {
            for instance in expand::instances(events, from, to) {
                found.push(Occurrence {
                    calendar: calendar.clone(),
                    key: format!(
                        "{href}#{}@{}",
                        instance.event.uid,
                        instance.time.start_instant().timestamp()
                    ),
                    href: href.clone(),
                    uid: instance.event.uid.clone(),
                    original: instance.original,
                    title: instance.event.summary.clone(),
                    location: instance.event.location.clone(),
                    notes: instance.event.description.clone(),
                    time: instance.time,
                });
            }
        }
        let found = Arc::new(found);
        self.shared
            .years
            .lock()
            .expect("years lock")
            .insert(year, Arc::clone(&found));
        found
    }
}

impl CalendarSource for Store {
    fn calendars(&self) -> Vec<Calendar> {
        self.shared
            .published
            .read()
            .expect("published lock")
            .calendars
            .clone()
    }

    fn occurrences(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<Occurrence> {
        let first = (from - chrono::Duration::days(1)).year();
        let last = (to + chrono::Duration::days(1)).year();
        let mut seen = HashSet::new();
        let mut found = Vec::new();
        for year in first..=last {
            for occurrence in self.year(year).iter() {
                if occurrence.time.overlaps(from, to) && seen.insert(occurrence.key.clone()) {
                    found.push(occurrence.clone());
                }
            }
        }
        found
    }

    fn add_moment(&self, at: DateTime<Utc>, title: String, calendar: &CalendarId) -> Reply {
        let (sender, receiver) = reply::channel();
        let _ = self.commands.send(Command::AddMoment {
            at,
            title,
            calendar: calendar.clone(),
            reply: sender,
        });
        receiver
    }

    fn change(&self, occurrence: &Occurrence, change: Change, scope: Scope) -> Reply {
        let (sender, receiver) = reply::channel();
        let _ = self.commands.send(Command::Change {
            occurrence: occurrence.clone(),
            change,
            scope,
            reply: sender,
        });
        receiver
    }

    fn delete(&self, occurrence: &Occurrence, scope: Scope) -> Reply {
        let (sender, receiver) = reply::channel();
        let _ = self.commands.send(Command::Delete {
            occurrence: occurrence.clone(),
            scope,
            reply: sender,
        });
        receiver
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Feeding {
    Now,
    WhenDue,
}

struct Worker {
    shared: Arc<Shared>,
    path: PathBuf,
    calendars: Vec<CalendarData>,
    accounts: Vec<Credentials>,
    feeds: Result<Feeds, String>,
}

impl Worker {
    async fn run(mut self, mut commands: mpsc::UnboundedReceiver<Command>) {
        let path = self.path.clone();
        self.calendars = tokio::task::spawn_blocking(move || cache::load(&path))
            .await
            .unwrap_or_default();
        self.publish();
        self.sync(Feeding::WhenDue).await;
        loop {
            tokio::select! {
                command = commands.recv() => match command {
                    None => return,
                    Some(command) => self.handle(command).await,
                },
                () = tokio::time::sleep(SYNC_EVERY) => self.sync(Feeding::WhenDue).await,
            }
        }
    }

    async fn handle(&mut self, command: Command) {
        match command {
            Command::Refresh => self.sync(Feeding::Now).await,
            Command::AddMoment {
                at,
                title,
                calendar,
                reply,
            } => {
                let _ = reply.send(self.add_moment(at, &title, &calendar).await);
            }
            Command::Change {
                occurrence,
                change,
                scope,
                reply,
            } => {
                let _ = reply.send(self.change(&occurrence, &change, scope).await);
            }
            Command::Delete {
                occurrence,
                scope,
                reply,
            } => {
                let _ = reply.send(self.delete(&occurrence, scope).await);
            }
            Command::SignIn { credentials, reply } => {
                let result = match Connection::open(&credentials).await {
                    Ok(_) => credentials::save(&credentials).await,
                    Err(error) => Err(error),
                };
                let succeeded = result.is_ok();
                let _ = reply.send(result);
                if succeeded {
                    self.sync(Feeding::WhenDue).await;
                }
            }
            Command::SignOut { account } => {
                let _ = credentials::forget(&account.server, &account.username).await;
                let key = credentials::account_key(&account.server, &account.username);
                self.calendars.retain(|data| data.calendar.account != key);
                self.save();
                self.sync(Feeding::WhenDue).await;
            }
        }
    }

    fn set_status(&self, status: Status) {
        *self.shared.status.write().expect("status lock") = status;
        (self.shared.notify)();
    }

    async fn sync(&mut self, feeding: Feeding) {
        self.set_status(Status::Syncing);
        self.accounts = match credentials::all().await {
            Ok(accounts) => accounts,
            Err(error) => {
                self.set_status(Status::Failed(error));
                return;
            }
        };
        if self.accounts.is_empty() {
            self.calendars.clear();
            self.save();
            self.publish();
            self.set_status(Status::NoAccounts);
            return;
        }
        let keys: HashSet<String> = self.accounts.iter().map(Credentials::key).collect();
        self.calendars
            .retain(|data| keys.contains(&data.calendar.account));
        let mut failures = Vec::new();
        for credentials in self.accounts.clone() {
            failures.extend(self.sync_account(&credentials, feeding).await);
        }
        self.save();
        self.publish();
        self.set_status(if failures.is_empty() {
            Status::Synced(Utc::now())
        } else {
            Status::Failed(failures.join("\n"))
        });
    }

    async fn sync_account(&mut self, credentials: &Credentials, feeding: Feeding) -> Vec<String> {
        let account = credentials.key();
        let opened = Connection::open(credentials).await;
        let (connection, home) = match opened {
            Ok(opened) => opened,
            Err(error) => return vec![error],
        };
        let found = match connection.calendars(&home).await {
            Ok(found) => found,
            Err(error) => return vec![format!("{account}: {error}")],
        };
        let mut failures = Vec::new();
        let subscribed = match connection.subscriptions(&home).await {
            Ok(subscribed) => Some(subscribed),
            Err(error) => {
                failures.push(format!("{account}: subscriptions: {error}"));
                None
            }
        };
        let hrefs: HashSet<String> = found
            .iter()
            .chain(subscribed.iter().flatten())
            .map(|(_, href)| href.clone())
            .collect();
        self.calendars.retain(|data| {
            data.calendar.account != account
                || hrefs.contains(&data.href)
                || (subscribed.is_none() && data.calendar.read_only)
        });
        for (calendar, href) in found {
            let index = self.index_of(&account, &href);
            let previous = index.map(|index| self.calendars[index].clone());
            match connection.sync(calendar, href, previous).await {
                Ok(data) => {
                    self.keep(index, data);
                    self.publish();
                }
                Err(error) => failures.push(format!("{account}: {error}")),
            }
        }
        for (calendar, href) in subscribed.into_iter().flatten() {
            if let Err(error) = self.fetch_feed(&connection, calendar, href, feeding).await {
                failures.push(error);
            }
        }
        failures
    }

    fn index_of(&self, account: &str, href: &str) -> Option<usize> {
        self.calendars
            .iter()
            .position(|data| data.calendar.account == account && data.href == href)
    }

    fn keep(&mut self, index: Option<usize>, data: CalendarData) {
        match index {
            Some(index) => self.calendars[index] = data,
            None => self.calendars.push(data),
        }
    }

    async fn fetch_feed(
        &mut self,
        connection: &Connection,
        calendar: Calendar,
        href: String,
        feeding: Feeding,
    ) -> Result<(), String> {
        let index = self.index_of(&calendar.account, &href);
        let mut data = index.map_or_else(
            || CalendarData::new(calendar.clone(), href.clone()),
            |index| self.calendars[index].clone(),
        );
        let title = calendar.title.clone();
        data.calendar = calendar;
        let now = Utc::now();
        let due = feeding == Feeding::Now
            || data
                .fetched
                .is_none_or(|fetched| now - fetched >= FETCH_FEEDS_EVERY);
        let fetched = if due {
            Some(self.feed(connection, &href).await)
        } else {
            None
        };
        if let Some(Ok(text)) = &fetched {
            data.resources = [(href.clone(), Resource::new(String::new(), text.clone()))].into();
            data.fetched = Some(now);
        }
        self.keep(index, data);
        self.publish();
        match fetched {
            Some(Err(error)) => Err(format!("{title}: {error}")),
            _ => Ok(()),
        }
    }

    async fn feed(&self, connection: &Connection, href: &str) -> Result<String, String> {
        let feeds = self.feeds.as_ref()?;
        let source = connection.source(href).await?;
        let text = feeds.fetch(&source).await?;
        ics::parse(&text).map_err(|error| format!("the feed could not be read ({error})"))?;
        Ok(text)
    }

    fn calendar_index(&self, calendar: &CalendarId) -> Result<usize, String> {
        let index = self
            .calendars
            .iter()
            .position(|data| &data.calendar.id == calendar)
            .ok_or_else(|| "That calendar is no longer on the server.".to_string())?;
        if self.calendars[index].calendar.read_only {
            return Err("That calendar is a subscription, which cannot be changed.".to_string());
        }
        Ok(index)
    }

    async fn connect(&self, index: usize) -> Result<Connection, String> {
        let account = &self.calendars[index].calendar.account;
        let credentials = self
            .accounts
            .iter()
            .find(|credentials| &credentials.key() == account)
            .ok_or("The account for that calendar is signed out.")?;
        Ok(Connection::open(credentials).await?.0)
    }

    fn resource(&self, index: usize, href: &str) -> Result<Resource, String> {
        self.calendars[index]
            .resources
            .get(href)
            .cloned()
            .ok_or_else(|| {
                "The event is no longer in its calendar. Refresh and try again.".to_string()
            })
    }

    async fn delete(&mut self, occurrence: &Occurrence, scope: Scope) -> Result<(), String> {
        let index = self.calendar_index(&occurrence.calendar)?;
        let resource = self.resource(index, &occurrence.href)?;
        let target = edit::Target {
            uid: &occurrence.uid,
            original: occurrence.original,
            time: occurrence.time,
        };
        let write = edit::delete(&resource.data, &target, scope, Utc::now())?;
        let connection = self.connect(index).await?;
        let resources = match write {
            edit::Write::Delete => {
                connection.delete(&occurrence.href, &resource.etag).await?;
                None
            }
            edit::Write::Put(text) => {
                let etag = connection
                    .update(&occurrence.href, text.clone(), &resource.etag)
                    .await?;
                Some(Resource::new(etag, text))
            }
        };
        let data = &mut self.calendars[index].resources;
        match resources {
            Some(resource) => {
                data.insert(occurrence.href.clone(), resource);
            }
            None => {
                data.remove(&occurrence.href);
            }
        }
        self.save();
        self.publish();
        Ok(())
    }

    async fn change(
        &mut self,
        occurrence: &Occurrence,
        change: &Change,
        scope: Scope,
    ) -> Result<(), String> {
        let index = self.calendar_index(&occurrence.calendar)?;
        let resource = self.resource(index, &occurrence.href)?;
        let moving = change.calendar != occurrence.calendar;
        if moving && occurrence.repeats() && scope == Scope::One {
            return Err(
                "One occurrence cannot move to another calendar. Move the whole series instead."
                    .to_string(),
            );
        }
        let target = edit::Target {
            uid: &occurrence.uid,
            original: occurrence.original,
            time: occurrence.time,
        };
        let text = edit::change(&resource.data, &target, scope, change, Utc::now())?;
        let connection = self.connect(index).await?;
        if !moving {
            let etag = connection
                .update(&occurrence.href, text.clone(), &resource.etag)
                .await?;
            self.calendars[index]
                .resources
                .insert(occurrence.href.clone(), Resource::new(etag, text));
            self.save();
            self.publish();
            return Ok(());
        }
        let destination = self.calendar_index(&change.calendar)?;
        let name = uuid::Uuid::new_v4().to_string().to_uppercase();
        let (href, etag) = self
            .connect(destination)
            .await?
            .put(&self.calendars[destination].href, &name, text.clone())
            .await?;
        self.calendars[destination]
            .resources
            .insert(href, Resource::new(etag.unwrap_or_default(), text));
        let removed = connection.delete(&occurrence.href, &resource.etag).await;
        if removed.is_ok() {
            self.calendars[index].resources.remove(&occurrence.href);
        }
        self.save();
        self.publish();
        removed.map_err(|error| {
            format!("The event was copied, but the original could not be removed: {error}")
        })
    }

    async fn add_moment(
        &mut self,
        at: DateTime<Utc>,
        title: &str,
        calendar: &CalendarId,
    ) -> Result<(), String> {
        let index = self.calendar_index(calendar)?;
        let connection = self.connect(index).await?;
        let uid = uuid::Uuid::new_v4().to_string().to_uppercase();
        let ics = moment::ics(&uid, at, title, Utc::now());
        let (href, etag) = connection
            .put(&self.calendars[index].href, &uid, ics.clone())
            .await?;
        let data = &mut self.calendars[index];
        data.resources
            .insert(href, Resource::new(etag.unwrap_or_default(), ics));
        self.save();
        self.publish();
        Ok(())
    }

    fn save(&self) {
        let _ = cache::save(&self.path, &self.calendars);
    }

    fn publish(&self) {
        let mut calendars: Vec<Calendar> = self
            .calendars
            .iter()
            .map(|data| data.calendar.clone())
            .collect();
        calendars
            .sort_by(|a, b| (&a.account, a.order, &a.title).cmp(&(&b.account, b.order, &b.title)));
        let resources = self
            .calendars
            .iter()
            .flat_map(|data| {
                data.resources.iter().map(|(href, resource)| {
                    (
                        data.calendar.id.clone(),
                        href.clone(),
                        Arc::clone(&resource.events),
                    )
                })
            })
            .collect();
        let accounts = self
            .accounts
            .iter()
            .map(|credentials| Account {
                server: credentials.server.clone(),
                username: credentials.username.clone(),
            })
            .collect();
        *self.shared.published.write().expect("published lock") = Arc::new(Published {
            accounts,
            calendars,
            resources,
        });
        self.shared.years.lock().expect("years lock").clear();
        (self.shared.notify)();
    }
}
