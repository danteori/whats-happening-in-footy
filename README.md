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

| Env var        | Default        | Purpose                                     |
| -------------- | -------------- | ------------------------------------------- |
| `PORT`         | `3000`         | Port to listen on (Render sets this)        |
| `HOST`         | `127.0.0.1`    | Interface to bind (`0.0.0.0` on Render)     |
| `FRONTEND_DIR` | `../frontend`  | Where the static files live                 |

## Project layout

```
backend/            Rust web server (axum)
  src/main.rs       routes: /api/hello, /healthz, static files
frontend/           what the browser loads, served as-is
  index.html
  app.js
  style.css
render.yaml         Render deployment blueprint
.github/workflows/  CI: fmt, clippy, tests
```

## Deploy

`render.yaml` is a [Render Blueprint](https://render.com/docs/infrastructure-as-code). In the
Render dashboard choose **New → Blueprint**, pick this repo, and Render builds and deploys it on
every push to `main`. The free plan sleeps after 15 minutes without traffic, so the first visit
after that takes about a minute.

## Contributing

PRs welcome, including from football newcomers. CI runs these, so run them before pushing:

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

## License

[MIT](LICENSE)
