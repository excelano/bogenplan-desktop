// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

#![forbid(unsafe_code)]

mod cache;
mod caldav;
mod credentials;
mod expand;
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

pub use model::{Account, Calendar, CalendarId, Occurrence, OccurrenceTime, Status};

use cache::{CalendarData, Resource};
use caldav::Connection;
use credentials::Credentials;

const SYNC_EVERY: Duration = Duration::from_secs(300);

pub type Reply = reply::Receiver<Result<(), String>>;

pub trait CalendarSource {
    fn calendars(&self) -> Vec<Calendar>;
    fn occurrences(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<Occurrence>;
    fn add_moment(&self, at: DateTime<Utc>, title: String, calendar: &CalendarId) -> Reply;
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
                    key: format!("{href}@{}", instance.time.start_instant().timestamp()),
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
}

struct Worker {
    shared: Arc<Shared>,
    path: PathBuf,
    calendars: Vec<CalendarData>,
    accounts: Vec<Credentials>,
}

impl Worker {
    async fn run(mut self, mut commands: mpsc::UnboundedReceiver<Command>) {
        let path = self.path.clone();
        self.calendars = tokio::task::spawn_blocking(move || cache::load(&path))
            .await
            .unwrap_or_default();
        self.publish();
        self.sync().await;
        loop {
            tokio::select! {
                command = commands.recv() => match command {
                    None => return,
                    Some(command) => self.handle(command).await,
                },
                () = tokio::time::sleep(SYNC_EVERY) => self.sync().await,
            }
        }
    }

    async fn handle(&mut self, command: Command) {
        match command {
            Command::Refresh => self.sync().await,
            Command::AddMoment {
                at,
                title,
                calendar,
                reply,
            } => {
                let _ = reply.send(self.add_moment(at, &title, &calendar).await);
            }
            Command::SignIn { credentials, reply } => {
                let result = match Connection::open(&credentials).await {
                    Ok(_) => credentials::save(&credentials).await,
                    Err(error) => Err(error),
                };
                let succeeded = result.is_ok();
                let _ = reply.send(result);
                if succeeded {
                    self.sync().await;
                }
            }
            Command::SignOut { account } => {
                let _ = credentials::forget(&account.server, &account.username).await;
                let key = credentials::account_key(&account.server, &account.username);
                self.calendars.retain(|data| data.calendar.account != key);
                self.save();
                self.sync().await;
            }
        }
    }

    fn set_status(&self, status: Status) {
        *self.shared.status.write().expect("status lock") = status;
        (self.shared.notify)();
    }

    async fn sync(&mut self) {
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
            if let Err(error) = self.sync_account(&credentials).await {
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

    async fn sync_account(&mut self, credentials: &Credentials) -> Result<(), String> {
        let (connection, home) = Connection::open(credentials).await?;
        let found = connection.calendars(&home).await?;
        let account = credentials.key();
        let hrefs: HashSet<String> = found.iter().map(|(_, href)| href.clone()).collect();
        self.calendars
            .retain(|data| data.calendar.account != account || hrefs.contains(&data.href));
        let mut result = Ok(());
        for (calendar, href) in found {
            let index = self
                .calendars
                .iter()
                .position(|data| data.calendar.account == account && data.href == href);
            let previous = index.map(|index| self.calendars[index].clone());
            match connection.sync(calendar, href, previous).await {
                Ok(data) => {
                    match index {
                        Some(index) => self.calendars[index] = data,
                        None => self.calendars.push(data),
                    }
                    self.publish();
                }
                Err(error) => result = Err(format!("{account}: {error}")),
            }
        }
        result
    }

    async fn add_moment(
        &mut self,
        at: DateTime<Utc>,
        title: &str,
        calendar: &CalendarId,
    ) -> Result<(), String> {
        let index = self
            .calendars
            .iter()
            .position(|data| &data.calendar.id == calendar)
            .ok_or("That calendar is no longer on the server.")?;
        let account = self.calendars[index].calendar.account.clone();
        let credentials = self
            .accounts
            .iter()
            .find(|credentials| credentials.key() == account)
            .ok_or("The account for that calendar is signed out.")?
            .clone();
        let (connection, _) = Connection::open(&credentials).await?;
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
