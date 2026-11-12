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
mod import;
mod model;
mod moment;
mod zones;

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock, mpsc as reply};
use std::time::Duration;

use chrono::{DateTime, Datelike, TimeZone, Utc};
use tokio::sync::mpsc;

pub use import::{Import, Imported};
pub use model::{Account, Calendar, CalendarId, Change, Occurrence, OccurrenceTime, Scope, Status};

use cache::{CalendarData, Resource};
use caldav::Connection;
use credentials::{Credentials, Feed};
use feed::Feeds;

const SYNC_EVERY: Duration = Duration::from_secs(300);
const FETCH_FEEDS_EVERY: chrono::Duration = chrono::Duration::hours(1);
pub const SUBSCRIPTIONS: &str = "Subscriptions";
pub const SUBSCRIPTION_COLORS: [[u8; 3]; 7] = [
    [0xFF, 0x29, 0x68],
    [0xFF, 0x95, 0x00],
    [0xFF, 0xCC, 0x00],
    [0x63, 0xDA, 0x38],
    [0x1B, 0xAD, 0xF8],
    [0xCC, 0x73, 0xE1],
    [0xA2, 0x84, 0x5E],
];

pub type Reply = reply::Receiver<Result<(), String>>;

pub trait CalendarSource {
    fn calendars(&self) -> Vec<Calendar>;
    fn occurrences(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<Occurrence>;
    fn add_moment(&self, at: DateTime<Utc>, title: String, calendar: &CalendarId) -> Reply;
    fn change(&self, occurrence: &Occurrence, change: Change, scope: Scope) -> Reply;
    fn delete(&self, occurrence: &Occurrence, scope: Scope) -> Reply;
    fn import(&self, import: Import, calendar: Option<CalendarId>) -> Reply;
    fn holding(&self, uid: &str) -> Option<CalendarId>;
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
    Import {
        import: Import,
        calendar: Option<CalendarId>,
        reply: reply::Sender<Result<(), String>>,
    },
    SignIn {
        credentials: Credentials,
        reply: reply::Sender<Result<(), String>>,
    },
    SignOut {
        account: Account,
    },
    Subscribe {
        source: String,
        reply: reply::Sender<Result<(), String>>,
    },
    Unsubscribe {
        calendar: CalendarId,
    },
    Recolor {
        calendar: CalendarId,
        color: [u8; 3],
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
            subscriptions: Vec::new(),
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

    pub fn subscribe(&self, source: String) -> Reply {
        let (sender, receiver) = reply::channel();
        let _ = self.commands.send(Command::Subscribe {
            source,
            reply: sender,
        });
        receiver
    }

    pub fn unsubscribe(&self, calendar: &CalendarId) {
        let _ = self.commands.send(Command::Unsubscribe {
            calendar: calendar.clone(),
        });
    }

    pub fn recolor(&self, calendar: &CalendarId, color: [u8; 3]) {
        let _ = self.commands.send(Command::Recolor {
            calendar: calendar.clone(),
            color,
        });
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

    fn import(&self, import: Import, calendar: Option<CalendarId>) -> Reply {
        let (sender, receiver) = reply::channel();
        let _ = self.commands.send(Command::Import {
            import,
            calendar,
            reply: sender,
        });
        receiver
    }

    fn holding(&self, uid: &str) -> Option<CalendarId> {
        let published = self.shared.published.read().expect("published lock");
        published
            .resources
            .iter()
            .filter(|(calendar, _, _)| {
                published
                    .calendars
                    .iter()
                    .any(|found| &found.id == calendar && !found.read_only)
            })
            .find(|(_, _, events)| events.iter().any(|event| event.uid == uid))
            .map(|(calendar, _, _)| calendar.clone())
    }
}

fn standalone_id(feed: &Feed) -> CalendarId {
    CalendarId(format!("{SUBSCRIPTIONS} {}", feed.id))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Feeding {
    Now,
    WhenDue,
}

#[derive(Clone, Copy)]
enum FeedSource<'a> {
    Subscribed(&'a Connection, &'a str),
    Standalone(&'a str),
}

struct Worker {
    shared: Arc<Shared>,
    path: PathBuf,
    calendars: Vec<CalendarData>,
    accounts: Vec<Credentials>,
    subscriptions: Vec<Feed>,
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
            Command::Import {
                import,
                calendar,
                reply,
            } => {
                let _ = reply.send(self.import(&import, calendar.as_ref()).await);
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
            Command::Subscribe { source, reply } => {
                let result = self.subscribe(&source).await;
                let succeeded = result.is_ok();
                let _ = reply.send(result);
                if succeeded {
                    self.sync(Feeding::WhenDue).await;
                }
            }
            Command::Unsubscribe { calendar } => {
                if let Some(feed) = self.subscription(&calendar) {
                    let _ = credentials::forget_feed(&feed.id).await;
                }
                self.calendars.retain(|data| data.calendar.id != calendar);
                self.save();
                self.sync(Feeding::WhenDue).await;
            }
            Command::Recolor { calendar, color } => {
                if let Err(error) = self.recolor(&calendar, color).await {
                    self.set_status(Status::Failed(error));
                }
            }
        }
    }

    fn set_status(&self, status: Status) {
        *self.shared.status.write().expect("status lock") = status;
        (self.shared.notify)();
    }

    async fn sync(&mut self, feeding: Feeding) {
        self.set_status(Status::Syncing);
        let loaded = match (credentials::all().await, credentials::feeds().await) {
            (Ok(accounts), Ok(subscriptions)) => (accounts, subscriptions),
            (Err(error), _) | (_, Err(error)) => {
                self.set_status(Status::Failed(error));
                return;
            }
        };
        (self.accounts, self.subscriptions) = loaded;
        if self.accounts.is_empty() && self.subscriptions.is_empty() {
            self.calendars.clear();
            self.save();
            self.publish();
            self.set_status(Status::NoAccounts);
            return;
        }
        let keys: HashSet<String> = self.accounts.iter().map(Credentials::key).collect();
        let feeds: HashSet<String> = self
            .subscriptions
            .iter()
            .map(|feed| feed.id.clone())
            .collect();
        self.calendars.retain(|data| {
            keys.contains(&data.calendar.account)
                || (data.calendar.account == SUBSCRIPTIONS && feeds.contains(&data.href))
        });
        let mut failures = Vec::new();
        for credentials in self.accounts.clone() {
            failures.extend(self.sync_account(&credentials, feeding).await);
        }
        for feed in self.subscriptions.clone() {
            let calendar = self.standalone(&feed);
            let fetched = self
                .fetch_feed(
                    calendar,
                    feed.id.clone(),
                    feeding,
                    FeedSource::Standalone(&feed.source),
                )
                .await;
            if let Err(error) = fetched {
                failures.push(error);
            }
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
            let source = FeedSource::Subscribed(&connection, &href);
            if let Err(error) = self
                .fetch_feed(calendar, href.clone(), feeding, source)
                .await
            {
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
        calendar: Calendar,
        href: String,
        feeding: Feeding,
        source: FeedSource<'_>,
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
            Some(self.feed(source).await)
        } else {
            None
        };
        if let Some(Ok(text)) = &fetched {
            if let (FeedSource::Standalone(_), Some(name)) = (source, ics::calendar_name(text)) {
                data.calendar.title = name;
            }
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

    async fn feed(&self, source: FeedSource<'_>) -> Result<String, String> {
        let feeds = self.feeds.as_ref()?;
        let source = match source {
            FeedSource::Subscribed(connection, href) => connection.source(href).await?,
            FeedSource::Standalone(source) => source.to_string(),
        };
        let text = feeds.fetch(&source).await?;
        ics::parse(&text).map_err(|error| format!("the feed could not be read ({error})"))?;
        Ok(text)
    }

    fn standalone(&self, feed: &Feed) -> Calendar {
        let id = standalone_id(feed);
        let title = self
            .calendars
            .iter()
            .find(|data| data.calendar.id == id)
            .and_then(|data| data.resources.values().next())
            .and_then(|resource| ics::calendar_name(&resource.data))
            .unwrap_or_else(|| feed::name(&feed.source));
        Calendar {
            id,
            title,
            color: feed.color,
            account: SUBSCRIPTIONS.to_string(),
            order: 0,
            read_only: true,
        }
    }

    fn subscription(&self, calendar: &CalendarId) -> Option<Feed> {
        self.subscriptions
            .iter()
            .find(|feed| &standalone_id(feed) == calendar)
            .cloned()
    }

    fn unused_color(&self) -> [u8; 3] {
        SUBSCRIPTION_COLORS
            .into_iter()
            .find(|color| {
                self.calendars
                    .iter()
                    .all(|data| data.calendar.color != *color)
            })
            .unwrap_or(SUBSCRIPTION_COLORS[self.subscriptions.len() % SUBSCRIPTION_COLORS.len()])
    }

    async fn subscribe(&mut self, source: &str) -> Result<(), String> {
        let source = source.trim();
        let address = feed::secure(source)?;
        if self
            .subscriptions
            .iter()
            .any(|feed| feed::secure(&feed.source).is_ok_and(|known| known == address))
        {
            return Err("That calendar is already subscribed.".to_string());
        }
        let text = self.feed(FeedSource::Standalone(source)).await?;
        let feed = Feed {
            id: uuid::Uuid::new_v4().to_string().to_uppercase(),
            source: source.to_string(),
            color: self.unused_color(),
        };
        credentials::save_feed(&feed).await?;
        let mut data = CalendarData::new(self.standalone(&feed), feed.id.clone());
        if let Some(name) = ics::calendar_name(&text) {
            data.calendar.title = name;
        }
        data.resources = [(feed.id.clone(), Resource::new(String::new(), text))].into();
        data.fetched = Some(Utc::now());
        self.calendars.push(data);
        self.subscriptions.push(feed);
        self.save();
        self.publish();
        Ok(())
    }

    async fn recolor(&mut self, calendar: &CalendarId, color: [u8; 3]) -> Result<(), String> {
        let mut feed = self
            .subscription(calendar)
            .ok_or("Only a subscription added here can change color.")?;
        feed.color = color;
        credentials::recolor_feed(&feed).await?;
        for known in &mut self.subscriptions {
            if known.id == feed.id {
                known.color = color;
            }
        }
        for data in &mut self.calendars {
            if &data.calendar.id == calendar {
                data.calendar.color = color;
            }
        }
        self.save();
        self.publish();
        Ok(())
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

    fn held(&self, uid: &str) -> Option<(usize, String)> {
        self.calendars
            .iter()
            .enumerate()
            .filter(|(_, data)| !data.calendar.read_only)
            .find_map(|(index, data)| {
                data.resources
                    .iter()
                    .find(|(_, resource)| resource.events.iter().any(|event| event.uid == uid))
                    .map(|(href, _)| (index, href.clone()))
            })
    }

    async fn import(
        &mut self,
        import: &Import,
        calendar: Option<&CalendarId>,
    ) -> Result<(), String> {
        let mut connections: HashMap<usize, Connection> = HashMap::new();
        for event in &import.events {
            let held = self.held(&event.uid);
            let index = match (&held, import.cancellation) {
                (None, true) => continue,
                (Some((index, _)), _) => *index,
                (None, false) => {
                    self.calendar_index(calendar.ok_or("There is no calendar to add to.")?)?
                }
            };
            let connection = match connections.entry(index) {
                Entry::Occupied(entry) => entry.into_mut(),
                Entry::Vacant(entry) => entry.insert(self.connect(index).await?),
            };
            let data = &mut self.calendars[index];
            match held {
                Some((_, href)) => {
                    let etag = data.resources[&href].etag.clone();
                    if import.cancellation {
                        connection.delete(&href, &etag).await?;
                        data.resources.remove(&href);
                    } else {
                        let etag = connection.update(&href, event.text.clone(), &etag).await?;
                        data.resources
                            .insert(href, Resource::new(etag, event.text.clone()));
                    }
                }
                None => {
                    let name = uuid::Uuid::new_v4().to_string().to_uppercase();
                    let (href, etag) = connection
                        .put(&data.href, &name, event.text.clone())
                        .await?;
                    data.resources.insert(
                        href,
                        Resource::new(etag.unwrap_or_default(), event.text.clone()),
                    );
                }
            }
            self.save();
            self.publish();
        }
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
        calendars.sort_by_key(|calendar| {
            (
                calendar.account == SUBSCRIPTIONS,
                calendar.account.clone(),
                calendar.order,
                calendar.title.clone(),
            )
        });
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
