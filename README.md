# radarr-auto-add-people

Follow people on TMDB — actors, directors, writers, composers — and add their
movies to Radarr automatically as they get released.

Packaged the same way as [explo](https://github.com/nonrootdocker/explo): a Nix
flake that builds a [minimalbase](https://github.com/nonrootdocker/minimalbase)
OCI image, published to GHCR by CI.

## Configuration

The program reads a single TOML file. It looks, in order, at:

1. the first command-line argument,
2. `$CONFIG_FILE`,
3. `/config/config.toml` (the container default).

Start from [`config.example.toml`](config.example.toml). Unknown keys are
rejected, so typos fail loudly at startup rather than being silently ignored.

To check a config without contacting TMDB or Radarr — worth doing before the
first real run, since a real run adds movies:

```bash
radarr-auto-add-people --check /path/to/config.toml
```

```toml
tmdb_key = "your-tmdb-api-key"

[radarr]
host = "http://radarr"
port = 7878
key = "your-radarr-api-key"
daysreleased = 365
root_folder_path = "/movies"
monitored = true
search = true
minimum_availability = "released"

[radarr.profiles]
default = 1       # required; keys are TMDB `original_language` codes
ja = 2

[people]
287 = ["actor"]                # Brad Pitt
525 = ["director", "writer"]   # Christopher Nolan

[notifications.homeassistant]
url = "http://172.18.0.21:1880/node-red/notifications"
```

In `[people]`, the key is a TMDB person id. `actor` matches cast credits;
anything else is matched as a case-insensitive substring against the crew `job`
field.

`daysreleased` limits candidates to movies released in the last N days. Omit it
for no date filter — note this differs from the Python original, where omitting
it silently matched nothing.

## Running

```bash
docker run --rm \
  -v /path/to/config.toml:/config/config.toml:ro \
  ghcr.io/<owner>/radarr-auto-add-people:latest
```

It is a one-shot job: it scans, adds, and exits. The container ABI declares
`mode = "triggered"`, so container-init runs it on demand rather than
supervising it as a daemon. Set `RUST_LOG=debug` for per-credit detail.

## Development

```bash
nix develop          # cargo, rustc, clippy, rustfmt
cargo test
cargo run -- config.toml

nix build            # the OCI image, as ./result
nix flake check      # builds the binary and runs the tests
```

## Behaviour notes

- Movies already in Radarr are skipped, and a movie credited to several
  followed people is only added once.
- Radarr's own `movie/lookup/tmdb` supplies the poster/fanart metadata. If that
  lookup fails the movie is still added, without images.
- Notification failures are logged and ignored; they never block an add.
- API keys go out as the `X-Api-Key` header rather than a query parameter, so
  they stay out of URLs and access logs.
