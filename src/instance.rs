// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use eframe::egui::{self, ViewportCommand};
use zbus::fdo::{RequestNameFlags, RequestNameReply};
use zbus::{Connection, connection, interface};

const NAME: &str = "com.excelano.Bogenplan";
const PATH: &str = "/com/excelano/Bogenplan";

pub enum Opening {
    File(PathBuf),
    Feed(String),
}

impl Opening {
    pub fn from_argument(argument: &str) -> Opening {
        let scheme = argument
            .split_once("://")
            .map(|(scheme, _)| scheme.to_ascii_lowercase());
        match scheme.as_deref() {
            Some("webcal" | "webcals") => Opening::Feed(argument.to_string()),
            Some("file") => {
                let file = url::Url::parse(argument)
                    .ok()
                    .and_then(|url| url.to_file_path().ok())
                    .unwrap_or_else(|| PathBuf::from(argument));
                Opening::File(file)
            }
            _ => {
                let file = PathBuf::from(argument);
                Opening::File(std::path::absolute(&file).unwrap_or(file))
            }
        }
    }

    fn to_argument(&self) -> String {
        match self {
            Opening::File(file) => file.to_string_lossy().into_owned(),
            Opening::Feed(address) => address.clone(),
        }
    }
}

pub struct Opened {
    pub openings: Receiver<Opening>,
    pub window: Arc<OnceLock<egui::Context>>,
}

struct Opener {
    openings: Sender<Opening>,
    window: Arc<OnceLock<egui::Context>>,
}

#[interface(name = "com.excelano.Bogenplan")]
impl Opener {
    fn open(&self, arguments: Vec<String>) {
        for argument in arguments {
            let _ = self.openings.send(Opening::from_argument(&argument));
        }
        if let Some(window) = self.window.get() {
            window.send_viewport_cmd(ViewportCommand::Focus);
            window.request_repaint();
        }
    }
}

pub fn claim(openings: Vec<Opening>) -> Option<Opened> {
    let arguments: Vec<String> = openings.iter().map(Opening::to_argument).collect();
    let (sender, receiver) = mpsc::channel();
    for opening in openings {
        let _ = sender.send(opening);
    }
    let window = Arc::new(OnceLock::new());
    let opener = Opener {
        openings: sender,
        window: Arc::clone(&window),
    };
    let (answer, answered) = mpsc::channel();
    std::thread::Builder::new()
        .name("desktop bus".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a runtime for the desktop bus");
            runtime.block_on(async move {
                match serve(opener).await {
                    Ok(_connection) => {
                        let _ = answer.send(false);
                        std::future::pending::<()>().await;
                    }
                    Err(_) => {
                        let _ = answer.send(forward(arguments).await.is_ok());
                    }
                }
            });
        })
        .expect("a thread for the desktop bus");
    let forwarded = answered
        .recv_timeout(Duration::from_secs(3))
        .unwrap_or(false);
    (!forwarded).then_some(Opened {
        openings: receiver,
        window,
    })
}

async fn serve(opener: Opener) -> zbus::Result<Connection> {
    let connection = connection::Builder::session()?
        .serve_at(PATH, opener)?
        .build()
        .await?;
    match connection
        .request_name_with_flags(NAME, RequestNameFlags::DoNotQueue.into())
        .await?
    {
        RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner => Ok(connection),
        RequestNameReply::InQueue | RequestNameReply::Exists => Err(zbus::Error::NameTaken),
    }
}

async fn forward(arguments: Vec<String>) -> zbus::Result<()> {
    Connection::session()
        .await?
        .call_method(Some(NAME), PATH, Some(NAME), "Open", &(arguments,))
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_are_files_or_feeds() {
        assert!(matches!(
            Opening::from_argument("webcal://example.com/a.ics"),
            Opening::Feed(address) if address == "webcal://example.com/a.ics"
        ));
        assert!(matches!(
            Opening::from_argument("WEBCALS://example.com/a.ics"),
            Opening::Feed(_)
        ));
        assert!(matches!(
            Opening::from_argument("file:///tmp/an%20invite.ics"),
            Opening::File(file) if file.as_path() == std::path::Path::new("/tmp/an invite.ics")
        ));
        assert!(matches!(
            Opening::from_argument("/tmp/invite.ics"),
            Opening::File(file) if file.as_path() == std::path::Path::new("/tmp/invite.ics")
        ));
    }
}
