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

pub struct Opened {
    pub files: Receiver<PathBuf>,
    pub window: Arc<OnceLock<egui::Context>>,
}

struct Opener {
    files: Sender<PathBuf>,
    window: Arc<OnceLock<egui::Context>>,
}

#[interface(name = "com.excelano.Bogenplan")]
impl Opener {
    fn open(&self, files: Vec<String>) {
        for file in files {
            let _ = self.files.send(file.into());
        }
        if let Some(window) = self.window.get() {
            window.send_viewport_cmd(ViewportCommand::Focus);
            window.request_repaint();
        }
    }
}

pub fn claim(files: Vec<PathBuf>) -> Option<Opened> {
    let (sender, receiver) = mpsc::channel();
    for file in &files {
        let _ = sender.send(file.clone());
    }
    let window = Arc::new(OnceLock::new());
    let opener = Opener {
        files: sender,
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
                        let _ = answer.send(forward(&files).await.is_ok());
                    }
                }
            });
        })
        .expect("a thread for the desktop bus");
    let forwarded = answered
        .recv_timeout(Duration::from_secs(3))
        .unwrap_or(false);
    (!forwarded).then_some(Opened {
        files: receiver,
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

async fn forward(files: &[PathBuf]) -> zbus::Result<()> {
    let files: Vec<String> = files
        .iter()
        .map(|file| file.to_string_lossy().into_owned())
        .collect();
    Connection::session()
        .await?
        .call_method(Some(NAME), PATH, Some(NAME), "Open", &(files,))
        .await
        .map(|_| ())
}
