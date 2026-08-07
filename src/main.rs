mod config;
mod notify;
mod radarr;
mod tmdb;

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use chrono::{Duration, NaiveDate, Utc};
use reqwest::blocking::Client;

use config::Config;
use radarr::{AddMovie, AddOptions, Radarr};
use tmdb::{Credit, Tmdb};

const DEFAULT_CONFIG_PATH: &str = "/config/config.toml";

/// A credit we have decided to add, with the person who earned it.
struct Candidate {
    credit: Credit,
    person: String,
    role: String,
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .init();

    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            log::error!("{err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let args = Args::parse();
    log::info!("Loading config from {}", args.config.display());
    let config = Config::load(&args.config)?;

    if args.check_only {
        let people = config.people()?;
        log::info!(
            "Config OK: {} people, Radarr at {}:{}",
            people.len(),
            config.radarr.host,
            config.radarr.port
        );
        return Ok(());
    }

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .user_agent(concat!(
            "radarr-auto-add-people/",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
        .context("building the HTTP client")?;

    let tmdb = Tmdb::new(client.clone(), config.tmdb_key.clone());
    let radarr = Radarr::new(client.clone(), &config.radarr);
    let notifier = notify::Notifier::new(client, &config.notifications);

    let existing = radarr.existing_tmdb_ids()?;
    log::info!("Radarr already has {} movies", existing.len());

    let cutoff = config
        .radarr
        .daysreleased
        .map(|days| (Utc::now() - Duration::days(days)).date_naive());
    if let Some(cutoff) = cutoff {
        log::info!("Only considering movies released on or after {cutoff}");
    }

    let mut candidates: Vec<Candidate> = Vec::new();
    let mut seen: HashSet<u64> = HashSet::new();

    for (person_id, roles) in config.people()? {
        let name = match tmdb.person(person_id) {
            Ok(person) => person.name,
            Err(err) => {
                log::warn!("Skipping person {person_id}: {err:#}");
                continue;
            }
        };
        log::info!("Getting movies for {name} ({person_id}) as {roles:?}");

        let credits = match tmdb.movie_credits(person_id) {
            Ok(credits) => credits,
            Err(err) => {
                log::warn!("Skipping {name} ({person_id}): {err:#}");
                continue;
            }
        };

        for role in roles {
            let matched: Vec<&Credit> = if role.eq_ignore_ascii_case("actor") {
                credits.cast.iter().collect()
            } else {
                credits
                    .crew
                    .iter()
                    .filter(|c| {
                        c.job
                            .as_deref()
                            .is_some_and(|job| job.to_lowercase().contains(&role.to_lowercase()))
                    })
                    .collect()
            };

            for credit in matched {
                if !released_after(credit, cutoff) {
                    continue;
                }
                if credit.best_title().is_none() {
                    log::warn!("Skipping TMDB movie {} with no title", credit.id);
                    continue;
                }
                if existing.contains(&credit.id) {
                    log::debug!("Already in Radarr: {}", credit.id);
                    continue;
                }
                if !seen.insert(credit.id) {
                    continue;
                }
                candidates.push(Candidate {
                    credit: credit.clone(),
                    person: name.clone(),
                    role: role.clone(),
                });
            }
        }
    }

    log::info!("{} movies to add", candidates.len());

    let mut added = 0;
    let mut failed = 0;
    for candidate in &candidates {
        match add(&radarr, &config, candidate) {
            Ok(movie) => {
                added += 1;
                log::info!("Added {} ({})", movie.title, movie.tmdb_id);
                notifier.send(&format!(
                    "Adding movie {} as it stars the {} {}",
                    movie.title, candidate.role, candidate.person
                ));
            }
            Err(err) => {
                failed += 1;
                log::error!("{err:#}");
            }
        }
    }

    log::info!("Done: {added} added, {failed} failed");
    Ok(())
}

fn add(radarr: &Radarr, config: &Config, candidate: &Candidate) -> Result<AddMovie> {
    let credit = &candidate.credit;
    let title = credit
        .best_title()
        .expect("candidates without a title are filtered out")
        .to_string();

    let year = credit
        .release_date
        .as_deref()
        .and_then(|d| d.get(..4))
        .and_then(|y| y.parse::<i32>().ok())
        .unwrap_or_default();

    let movie = AddMovie {
        title_slug: slug::slugify(format!("{title}-{}", credit.id)),
        quality_profile_id: config.quality_profile_for(credit.original_language.as_deref()),
        title,
        tmdb_id: credit.id,
        year,
        images: radarr.lookup_images(credit.id).unwrap_or_else(|err| {
            log::warn!("No images for tmdbId {}: {err:#}", credit.id);
            Vec::new()
        }),
        root_folder_path: config.radarr.root_folder_path.clone(),
        monitored: config.radarr.monitored,
        add_options: AddOptions {
            search_for_movie: config.radarr.search,
        },
        minimum_availability: config.radarr.minimum_availability.clone(),
    };

    radarr.add_movie(&movie)?;
    Ok(movie)
}

/// TMDB release dates are `YYYY-MM-DD`, but are frequently empty for
/// unannounced projects. Those are skipped, matching the Python original.
fn released_after(credit: &Credit, cutoff: Option<NaiveDate>) -> bool {
    let Some(date) = credit.release_date.as_deref().filter(|d| !d.is_empty()) else {
        return false;
    };
    let Some(cutoff) = cutoff else {
        return true;
    };
    match NaiveDate::parse_from_str(date, "%Y-%m-%d") {
        Ok(released) => released > cutoff,
        Err(_) => {
            log::warn!(
                "Unparseable release date {date:?} on TMDB movie {}",
                credit.id
            );
            false
        }
    }
}

struct Args {
    config: PathBuf,
    /// Validate the config and exit without contacting TMDB or Radarr.
    check_only: bool,
}

impl Args {
    fn parse() -> Self {
        let args: Vec<OsString> = std::env::args_os().skip(1).collect();
        let check_only = args.iter().any(|a| a == "--check");
        let config = args
            .into_iter()
            .find(|a| !a.to_string_lossy().starts_with("--"))
            .or_else(|| std::env::var_os("CONFIG_FILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH));
        Self { config, check_only }
    }
}
