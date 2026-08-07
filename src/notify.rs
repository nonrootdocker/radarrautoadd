use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde_json::json;

use crate::config::Notifications;

pub struct Notifier<'a> {
    client: Client,
    config: &'a Notifications,
}

impl<'a> Notifier<'a> {
    pub fn new(client: Client, config: &'a Notifications) -> Self {
        Self { client, config }
    }

    /// Fan the message out to whatever is configured. Notifications are
    /// best-effort: a failure here must not stop us adding movies.
    pub fn send(&self, message: &str) {
        if let Some(ha) = &self.config.homeassistant {
            if let Err(err) = self.home_assistant(&ha.url, &ha.title, message) {
                log::warn!("Home Assistant notification failed: {err:#}");
            }
        }
    }

    fn home_assistant(&self, url: &str, title: &str, message: &str) -> Result<()> {
        self.client
            .post(url)
            .json(&json!({ "message": message, "title": title }))
            .send()
            .context("sending request")?
            .error_for_status()?;
        Ok(())
    }
}
