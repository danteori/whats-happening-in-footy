# What's Happening in Footy ⚽

A free, open-source web app for people who want to *get into* football (soccer), starting with
the Premier League. The goal is to surface the **interesting stories** that make the sport fun to
follow:

- 📰 **News**: one aggregated feed instead of ten tabs
- 📺 **Matches worth watching**, and *why* they matter (title race, relegation scrap, derby, a streak on the line…)
- 📊 **Stats before, during and after a game**, explained for newcomers

> **Status:** hello world. The pieces are wired together (Rust backend ⇄ plain-JS frontend ⇄
> Render), and the features come next.

## Stack

| Part     | Tech                                                              |
| -------- | ----------------------------------------------------------------- |
| Backend  | Rust, [axum](https://github.com/tokio-rs/axum) on tokio            |
| Frontend | Plain HTML/CSS/JavaScript: no framework, no build step             |
| Hosting  | [Render](https://render.com) free web service (see `render.yaml`)  |

The backend serves the JSON API under `/api/*` and serves everything else straight out of
`frontend/`, so it's one service and one URL, with no CORS to configure.

## Run it locally

1. Install Rust: <https://rustup.rs> (on Windows, rustup will also prompt for the
   Visual Studio C++ Build Tools).
2. From the repo root:

   ```sh
   cargo run
   ```

3. Open <http://localhost:3000>. You should see "Hello, world" plus a greeting fetched from the
   Rust backend.

Frontend changes (`frontend/*`) only need a browser refresh. Backend changes need a restart of
`cargo run`.

| Env var                  | Default                            | Purpose                                           |
| ------------------------ | ---------------------------------- | ------------------------------------------------- |
| `PORT`                   | `3000`                             | Port to listen on (Render sets this)              |
| `HOST`                   | `127.0.0.1`                        | Interface to bind (`0.0.0.0` on Render)           |
| `FRONTEND_DIR`           | `../frontend`                      | Where the static files live                       |
| `FOOTBALL_DATA_API_KEY`  | *(none)*                           | football-data.org key for matches, table, scorers |
| `HIGHLIGHTLY_API_KEY`    | *(none)*                           | Highlightly key for lineups and match events      |
| `FOOTBALL_DATA_BASE_URL` | `https://api.football-data.org/v4` | Only for tests with a mock server                 |
| `HIGHLIGHTLY_BASE_URL`   | `https://soccer.highlightly.net`   | Only for tests with a mock server                 |

Without the two keys the site still runs, and the data routes answer `503`. To get the keys,
see [docs/data-sources.md](docs/data-sources.md).

## Project layout

```
backend/            Rust web server (axum)
  src/main.rs       reads the environment and starts the server
  src/lib.rs        routes: /api/hello, /healthz, static files
  src/api.rs        data routes: /api/matches, /api/table, /api/scorers,
                    /api/matches/{id}/lineups, /api/matches/{id}/events
  src/service.rs    cache and Highlightly request budget
  src/football_data.rs, src/highlightly.rs   the two API clients
  src/clubs.rs      team names from each source → one club ID
  src/table.rs      the table computed from results, as a check
  tests/            route tests with a mock server, and JSON fixtures
frontend/           what the browser loads, served as-is
  index.html
  app.js
  style.css
render.yaml         Render deployment blueprint
scripts/            Render build/start scripts (incl. joining the tailnet)
docs/               longer guides: data-sources.md, tailnet.md
.github/workflows/  CI: fmt, clippy, tests, Render script smoke test
```

## Deploy

`render.yaml` is a [Render Blueprint](https://render.com/docs/infrastructure-as-code). In the
Render dashboard choose **New → Blueprint**, pick this repo, and Render builds and deploys it on
every push to `main`. The free plan sleeps after 15 minutes without traffic, so the first visit
after that takes about a minute.

**Football data:** the site needs two free API keys, from football-data.org and Highlightly.
Set them in the Render dashboard. See [docs/data-sources.md](docs/data-sources.md) for the
sign-up steps, the checks, and the terms that the site must follow.

**Optional:** the Render server can join a private [Tailscale](https://tailscale.com) network,
so it can reach a database hosted on a home PC. See [docs/tailnet.md](docs/tailnet.md).

## Contributing

PRs welcome, including from football newcomers. CI runs these, so run them before pushing:

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

## License

[MIT](LICENSE)
