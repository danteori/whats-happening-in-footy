# Football data sources (football-data.org and Highlightly)

The backend gets Premier League data from two free APIs. Each API needs its own key. The keys
are secrets, so they are not in the repo. Without keys the site still starts, and the data
routes answer `HTTP 503` with the name of the missing variable.

| Source | What we use it for | Free plan |
| --- | --- | --- |
| [football-data.org](https://www.football-data.org) | Fixtures, results, match status, the league table, top scorers | 10 requests/minute; Premier League included; scores are a few minutes late |
| [Highlightly](https://highlightly.net/football-api/) | Lineups and match events (goals, cards, substitutions), after full time | 100 requests/day for all endpoints together |

Why these two:

- football-data.org is the only official, free, and legal source with the full current Premier
  League season. Its terms allow a public app with attribution. Its free plan has no lineups or
  match events.
- Highlightly has a free plan with lineups and events, and its terms allow storage of the data.
  100 requests a day is enough when the app fetches each match only once, after full time.
- The backend also computes the table from the results. It compares the result with the
  official table and logs a warning for each row that is different. It shows only the official
  table, because only that table includes point deductions.

The facts come from the official pages: football-data.org
[pricing](https://www.football-data.org/pricing), [coverage](https://www.football-data.org/coverage),
[policies](https://docs.football-data.org/general/v4/policies.html), and
[terms](https://www.football-data.org/about); Highlightly [plans](https://highlightly.net/football-api/),
[API docs](https://highlightly.net/football-api/documentation/), and
[terms](https://highlightly.net/terms/).

```
Browser ──▶ backend (Render) ──cache──▶ football-data.org   (matches, table, scorers)
                              └─cache──▶ Highlightly         (lineups, events of finished matches)
```

## One-time setup

1. **Get a football-data.org key.**
   1. Open <https://www.football-data.org/client/register>.
   2. Type your first name and your e-mail address. Select "Rust" as your language (optional).
   3. Read and accept the terms, then click the button to create the account.
   4. football-data.org sends the key to your e-mail address. It is a string of 32 letters and
      digits. The free tier starts immediately. You do not need a credit card.
2. **Get a Highlightly key.** Use highlightly.net directly. Do not use RapidAPI: the accounts
   and the keys are different, and the code expects a direct key.
   1. Open <https://highlightly.net/login> and create an account.
   2. Open the dashboard at <https://highlightly.net/dashboard>.
   3. Subscribe to the **Football API** on the **BASIC** plan ($0, 100 requests a day). Each
      sport is a separate subscription. The "All Sports" API is a different product.
   4. Copy the API key from the dashboard. You do not need a credit card.

   The Highlightly dashboard can use other names for these screens. The important parts are:
   the Football API, the BASIC plan, and a key from highlightly.net.
3. **Give the keys to Render.** In the Render dashboard, open the service, go to
   **Environment**, and add these two variables. `render.yaml` declares them with
   `sync: false`, so the values never come from the repo. Render does not ask for them on an
   existing service, so you must add them yourself.

   | Variable | Value |
   | --- | --- |
   | `FOOTBALL_DATA_API_KEY` | the football-data.org key |
   | `HIGHLIGHTLY_API_KEY` | the Highlightly key |

   Save with the option that also deploys, for example **Save and deploy**. With **Save only**,
   the service gets the keys only at the next deploy.
4. **Optional: use the keys when you run the app on your computer.** Do not write the keys in a
   file in the repo, and do not type them in a command, because the shell history keeps
   commands. Read each key into the shell instead. Then start the app from the same shell.

   macOS, Linux, or Git Bash on Windows:

   ```sh
   read -rs FOOTBALL_DATA_API_KEY && export FOOTBALL_DATA_API_KEY
   read -rs HIGHLIGHTLY_API_KEY && export HIGHLIGHTLY_API_KEY
   cargo run
   ```

   PowerShell 7 on Windows:

   ```powershell
   $env:FOOTBALL_DATA_API_KEY = Read-Host -MaskInput "football-data.org key"
   $env:HIGHLIGHTLY_API_KEY = Read-Host -MaskInput "Highlightly key"
   cargo run
   ```

   Each `read` or `Read-Host` waits for you to paste the key and press Enter. The screen does
   not show the key. The keys stay only in that shell, until you close it.
5. **Check it worked.** Do the checks in the next section. Do them once on your computer or on
   the Render URL. On Render, use your site address in place of `http://localhost:3000`.

Treat both keys like passwords. Keep them out of the repo, chat, issues, and screenshots. If a
key leaks, get a new key from the same site and replace the value in Render.

## Check it worked

### 1. The server sees the keys

At start-up, the server logs one warning for each key that it does not have:

```
WARN backend: FOOTBALL_DATA_API_KEY is not set; the routes that need it answer HTTP 503
```

If you see no such line, the server has both keys. On Render, look in **Logs**.

### 2. The football-data.org routes

```sh
curl -sS http://localhost:3000/api/matches
curl -sS http://localhost:3000/api/table
curl -sS http://localhost:3000/api/scorers
```

A good `/api/matches` response has all 380 matches of the season:

```json
{"matches":[{"id":537785,"kickoff":"2026-08-21T19:00:00Z","round":1,"status":"finished",
  "home":{"id":"arsenal","name":"Arsenal","short_name":"Arsenal"},
  "away":{"id":"coventry","name":"Coventry City","short_name":"Coventry"},
  "score":{"full_time":{"home":3,"away":0},"half_time":{"home":2,"away":0}}}, ...]}
```

The `id` values above are examples. `status` is one of `scheduled`, `in_play`, `paused` (half
time), `finished`, `postponed`, `suspended`, `cancelled`, `awarded`, or `unknown`.

A good `/api/table` response has 20 rows and a check:

```json
{"table":[{"position":1,"team":{"id":"man-city",...},"played":5,"won":5,"drawn":0,"lost":0,
  "goals_for":13,"goals_against":5,"goal_difference":8,"points":15}, ...],
 "check":{"matches_official":true,"differences":[]}}
```

If `matches_official` is `false`, look at `differences` and at the warnings in the log. A point
deduction or a match that the league awarded can cause a difference. The site still shows the
official table.

A good `/api/scorers` response:

```json
{"scorers":[{"player":"...","team":{"id":"man-city",...},"goals":5,"assists":1,
  "penalties":0,"played_matches":5}, ...]}
```

### 3. The Highlightly routes

Take the `id` of a **finished** match from `/api/matches`. Use a match from the last few days.
Then:

```sh
curl -sS http://localhost:3000/api/matches/<id>/lineups
curl -sS http://localhost:3000/api/matches/<id>/events
```

A good lineups response has the formation and the players, row by row. The first row is the
goalkeeper:

```json
{"home":{"team":{"id":"fulham",...},"formation":"4-2-3-1",
  "starting_rows":[[{"name":"...","number":1,"position":"Goalkeeper"}],[...]],
  "substitutes":[...]},
 "away":{...}}
```

A good events response:

```json
{"events":[{"team":{"id":"man-utd",...},"minute":52,"added_time":null,"kind":"goal",
  "player":"...","assist":"...","substituted":null}, ...]}
```

`kind` is one of `goal`, `own_goal`, `penalty_goal`, `missed_penalty`, `yellow_card`,
`red_card`, `substitution`, `var_goal_confirmed`, `var_goal_cancelled`,
`var_goal_cancelled_offside`, `var_penalty`, `var_penalty_cancelled`, or `other`.

The first lineups or events request for a match date uses 2 of the 100 daily Highlightly
requests: one to find the match, and one for the data. The other kind of data for the same
match uses 1 more. After that, requests for that match use none.

### Errors

Each error is JSON with an `error` text. When an API answers with an error, the text includes
the API's own message after the status.

| Status | Example `error` | What to do |
| --- | --- | --- |
| 503 | `set FOOTBALL_DATA_API_KEY in the server environment to use this route` | Set the variable (step 3 or 4). The body also has `missing_variables`. |
| 502 | `football-data.org answered with HTTP 400: Your API token is invalid.` | The key is wrong. Copy it again from the football-data.org e-mail. |
| 502 | `football-data.org answered with HTTP 403: ...` | The free plan does not include this data. |
| 502 | `football-data.org answered with HTTP 429: ...` | Too many requests in one minute. Wait one minute. |
| 502 | `the request to Highlightly failed: ...` | The API did not answer. Look at <https://status.highlightly.net>, then try again later. |
| 502 | `the team name "..." matches no Premier League 2026/27 club` | A source uses a team name that the app does not know. Add it to `backend/src/clubs.rs`. |
| 502 | `Highlightly has no Premier League match that links to match ...` | See "Facts that only a live check can confirm" below. |
| 503 | `the app used its daily budget of 90 Highlightly requests; ...` | Wait until 00:00 UTC. Cached lineups and events still work. |
| 409 | `match ... is not finished; lineups and events are available after full time` | Use a finished match. |
| 404 | `no Premier League match has the id ...` | Use an `id` from `/api/matches`. |

## Facts that only a live check can confirm

Nobody tested these with real keys before this change, because the keys did not exist. The
tests use example responses that match the API docs. Check each item once, and write what you
find in a GitHub issue.

1. **Does the Highlightly free plan include the Premier League?** The plan says
   "Unrestricted matches data", but each response says "Some results might be hidden with FREE
   tier". To check, use 1 of your 100 daily requests. Read the key into the shell first (step
   4 of the setup). Then put a recent match date in the command:

   ```sh
   curl -sS -H "x-rapidapi-key: $HIGHLIGHTLY_API_KEY" \
     "https://soccer.highlightly.net/matches?leagueId=33973&date=2026-09-20&timezone=Etc/UTC"
   ```

   Good: `data` lists the Premier League matches of that date. Bad: `data` is empty or has
   fewer matches than were played. If it is bad, the choices are:
   - Leave `HIGHLIGHTLY_API_KEY` unset. Lineups and events answer 503. All other routes work.
   - Pay for Highlightly PRO ($9.49 a month, 7,500 requests a day). The code needs no change.
   - Pay for football-data.org "Free + Deep Data" (€29 a month). The code then needs a change.
2. **Do the Highlightly team names match?** The app expects names such as "Manchester City",
   "Brighton", or "Nottingham Forest". If a name is different, the lineups route answers 502
   and names the team. Add the name to the aliases in `backend/src/clubs.rs`.
3. **Does `/api/scorers` work on the football-data.org free plan?** Other developers say yes.
   If it answers 502 with HTTP 403, the free plan does not include scorers.
4. **How late are football-data.org scores?** The free plan says "scores delayed". Other
   developers report a few minutes. During a match, compare `/api/matches` with a live score.
   Check also that `status` changes to `in_play` and `paused` during the match.
5. **When do Highlightly lineups and events appear?** The docs say that Highlightly reads
   lineups from 40 minutes before kickoff until 120 minutes after, and events once a minute.
   The app asks only after football-data.org says `finished`. If a lineups response is empty,
   the app does not keep it, and the next request asks again.
6. **What does `substituted` mean in a Highlightly substitution event?** The docs do not say
   which player comes on and which goes off. Look at one real substitution, and write the
   answer in the issue.

## Request limits

| Source | Limit | How the app stays inside it |
| --- | --- | --- |
| football-data.org | 10 requests a minute | The cache keeps each response for up to 10 minutes. During a match it keeps them for 60 seconds. One page view makes at most 3 requests: matches, table, and scorers. |
| Highlightly | 100 requests a day, reset at 00:00 UTC | The app counts its calls in each UTC day and stops at 90. It asks only once for each finished match. |

## Cache rules

The cache is in memory. Render stops the free service after 15 idle minutes, and the cache and
the request count are then lost. The first request after a wake-up fetches the data again. A
later workstream moves the cache to the database.

- football-data.org matches, table, and scorers: 60 seconds while a match is in play or just
  after its kickoff time, else until the next kickoff, and at most 10 minutes.
- Highlightly lineups and events of a finished match: no expiry. Empty lineups or events are not
  kept.
- The app does not ask Highlightly about a match that is not finished.

## Terms that the app must follow

- **Attribution (football-data.org §7.1).** Each page that shows this data must show this text
  in a visible place:

  > Football data provided by the Football-Data.org API

- **Keys (football-data.org §6.1; Highlightly docs).** Keys must not be in an open-source
  repository. They come only from environment variables. The backend never puts a key in a
  response or a log line.
- **One key for one app (football-data.org §2.3).** Use this key only for this site, and for
  local runs of this site.
- **Crests and logos (football-data.org §9.2; Highlightly §6.3).** Team crests belong to the
  clubs. The backend does not send crest or logo URLs to the frontend.
- **If you stop using football-data.org (§9.1),** the site must stop showing its data.
- **Highlightly (§6.1, §7).** Storage of the data is allowed. Resale of API access, a proxy of
  the API, and any gambling use are not allowed. The free plan "is not subject to the same
  terms as paid plans" (§4.1).
- **Fair use (football-data.org §3).** The operator can cancel a key after "continually
  excessive" use. The cache keeps the app far below the limit.

Read the full terms at <https://www.football-data.org/about> and <https://highlightly.net/terms/>.

## Good to know

- **Names and IDs.** The two sources use different team names and IDs. `backend/src/clubs.rs`
  maps every name to one club ID, for example `man-city`. If a club is promoted, add it there for
  the next season.
- **Match IDs.** The routes use the football-data.org match ID. The backend finds the
  Highlightly match with the same UTC date and the same home and away clubs.
- **Tables can differ for a short time.** The matches and the table come from two different
  requests, so after a result they can disagree for some minutes. A warning in the log for one
  request is not a problem. Look at a warning that stays.
- **Other base URLs.** `FOOTBALL_DATA_BASE_URL` and `HIGHLIGHTLY_BASE_URL` change the API
  addresses. The tests use them for a local mock server. Do not set them on Render.
