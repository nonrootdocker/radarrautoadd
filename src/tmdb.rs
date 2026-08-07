use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde::Deserialize;

const BASE_URL: &str = "https://api.themoviedb.org/3";

pub struct Tmdb {
    client: Client,
    api_key: String,
}

#[derive(Debug, Deserialize)]
pub struct Person {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct MovieCredits {
    #[serde(default)]
    pub cast: Vec<Credit>,
    #[serde(default)]
    pub crew: Vec<Credit>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Credit {
    pub id: u64,
    #[serde(default)]
    pub original_title: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub original_language: Option<String>,
    #[serde(default)]
    pub release_date: Option<String>,
    /// Only present on crew credits.
    #[serde(default)]
    pub job: Option<String>,
}

impl Credit {
    /// Radarr wants a title; TMDB occasionally omits `original_title`.
    pub fn best_title(&self) -> Option<&str> {
        self.original_title
            .as_deref()
            .filter(|t| !t.is_empty())
            .or_else(|| self.title.as_deref().filter(|t| !t.is_empty()))
    }
}

impl Tmdb {
    pub fn new(client: Client, api_key: String) -> Self {
        Self { client, api_key }
    }

    pub fn person(&self, person_id: u64) -> Result<Person> {
        self.get(&format!("{BASE_URL}/person/{person_id}"))
            .with_context(|| format!("looking up TMDB person {person_id}"))
    }

    pub fn movie_credits(&self, person_id: u64) -> Result<MovieCredits> {
        self.get(&format!("{BASE_URL}/person/{person_id}/movie_credits"))
            .with_context(|| format!("looking up movie credits for TMDB person {person_id}"))
    }

    fn get<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        let response = self
            .client
            .get(url)
            .query(&[("api_key", self.api_key.as_str())])
            .send()?
            .error_for_status()?;
        Ok(response.json()?)
    }
}
