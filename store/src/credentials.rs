// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::collections::HashMap;

const SERVICE: &str = "bogenplan";

#[derive(Clone)]
pub(crate) struct Credentials {
    pub server: String,
    pub username: String,
    pub password: String,
}

impl Credentials {
    pub fn key(&self) -> String {
        account_key(&self.server, &self.username)
    }
}

pub(crate) fn account_key(server: &str, username: &str) -> String {
    format!("{username}@{server}")
}

async fn keyring() -> Result<oo7::Keyring, String> {
    oo7::Keyring::new()
        .await
        .map_err(|error| format!("the keyring could not be opened: {error}"))
}

pub(crate) async fn all() -> Result<Vec<Credentials>, String> {
    let keyring = keyring().await?;
    let items = keyring
        .search_items(&HashMap::from([("service", SERVICE)]))
        .await
        .map_err(|error| format!("the keyring could not be searched: {error}"))?;
    let mut found = Vec::new();
    for item in items {
        if item.is_locked().await.unwrap_or(false) {
            item.unlock()
                .await
                .map_err(|_| "the keyring is locked".to_string())?;
        }
        let attributes = item.attributes().await.map_err(|error| error.to_string())?;
        let secret = item.secret().await.map_err(|error| error.to_string())?;
        let (Some(server), Some(username)) = (attributes.get("server"), attributes.get("username"))
        else {
            continue;
        };
        found.push(Credentials {
            server: server.clone(),
            username: username.clone(),
            password: String::from_utf8_lossy(&secret).into_owned(),
        });
    }
    found.sort_by_key(Credentials::key);
    Ok(found)
}

pub(crate) async fn save(credentials: &Credentials) -> Result<(), String> {
    keyring()
        .await?
        .create_item(
            "Bogenplan CalDAV",
            &HashMap::from([
                ("service", SERVICE),
                ("server", credentials.server.as_str()),
                ("username", credentials.username.as_str()),
            ]),
            credentials.password.as_bytes(),
            true,
        )
        .await
        .map_err(|error| format!("the password could not be saved: {error}"))
}

pub(crate) async fn forget(server: &str, username: &str) -> Result<(), String> {
    keyring()
        .await?
        .delete(&HashMap::from([
            ("service", SERVICE),
            ("server", server),
            ("username", username),
        ]))
        .await
        .map_err(|error| format!("the password could not be removed: {error}"))
}
