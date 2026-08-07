use std::collections::HashSet;

use anyhow::{Context, Result};
use reqwest::blocking::{Client, RequestBuilder};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config;

pub struct Radarr {
    client: Client,
    base_url: String,
    api_key: String,
    user: Option<String>,
    pass: Option<String>,
}

/// The payload Radarr expects on `POST /api/v3/movie`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddMovie {
    pub title: String,
    pub quality_profile_id: u32,
    pub title_slug: String,
    pub tmdb_id: u64,
    pub year: i32,
    pub images: Vec<Value>,
    pub root_folder_path: String,
    pub monitored: bool,
    pub add_options: AddOptions,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum_availability: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddOptions {
    pub search_for_movie: bool,
}

#[derive(Debug, Deserialize)]
struct MovieResource {
    #[serde(rename = "tmdbId")]
    tmdb_id: u64,
}

#[derive(Debug, Deserialize)]
struct LookupResource {
    #[serde(default)]
    images: Vec<Value>,
}

impl Radarr {
    pub fn new(client: Client, cfg: &config::Radarr) -> Self {
        let host = cfg.host.trim_end_matches('/');
        let base_url = match cfg.urlbase.as_deref().map(|b| b.trim_matches('/')) {
            Some(urlbase) if !urlbase.is_empty() => {
                format!("{host}:{}/{urlbase}/api/v3", cfg.port)
            }
            _ => format!("{host}:{}/api/v3", cfg.port),
        };
        Self {
            client,
            base_url,
            api_key: cfg.key.clone(),
            user: cfg.user.clone(),
            pass: cfg.pass.clone(),
        }
    }

    fn request(&self, method: Method, endpoint: &str) -> RequestBuilder {
        let builder = self
            .client
            .request(method, format!("{}/{endpoint}", self.base_url))
            .header("X-Api-Key", &self.api_key);
        match &self.user {
            Some(user) => builder.basic_auth(user, self.pass.as_ref()),
            None => builder,
        }
    }

    /// TMDB ids of everything already in the library.
    pub fn existing_tmdb_ids(&self) -> Result<HashSet<u64>> {
        let movies: Vec<MovieResource> = self
            .request(Method::GET, "movie")
            .send()
            .and_then(|r| r.error_for_status())
            .context("listing movies from Radarr")?
            .json()
            .context("parsing the Radarr movie list")?;
        Ok(movies.into_iter().map(|m| m.tmdb_id).collect())
    }

    /// Radarr's own TMDB lookup, used purely to get poster/fanart metadata in
    /// the shape Radarr wants back on add.
    pub fn lookup_images(&self, tmdb_id: u64) -> Result<Vec<Value>> {
        let lookup: LookupResource = self
            .request(Method::GET, "movie/lookup/tmdb")
            .query(&[("tmdbId", tmdb_id)])
            .send()
            .and_then(|r| r.error_for_status())
            .with_context(|| format!("looking up tmdbId {tmdb_id} in Radarr"))?
            .json()
            .with_context(|| format!("parsing the Radarr lookup for tmdbId {tmdb_id}"))?;
        Ok(lookup.images)
    }

    pub fn add_movie(&self, movie: &AddMovie) -> Result<()> {
        let response = self
            .request(Method::POST, "movie")
            .json(movie)
            .send()
            .with_context(|| format!("adding {} to Radarr", movie.title))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            anyhow::bail!("Radarr rejected {} with {status}: {body}", movie.title);
        }
        Ok(())
    }
}
