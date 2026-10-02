use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use axum::{
    Router,
    body::Body,
    extract::{Path, Query, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use backend::{
    app,
    config::{ApiKey, Config, SourceConfig},
    service::DataService,
    upstream::build_client,
};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

const FOOTBALL_DATA_KEY: &str = "test-football-data-key";
const HIGHLIGHTLY_KEY: &str = "test-highlightly-key";
const FINISHED_MATCH: u64 = 560050;
const OTHER_FINISHED_MATCH_SAME_DAY: u64 = 560047;
const FUTURE_MATCH: u64 = 560051;
const MATCH_WITH_EMPTY_DATA: u64 = 560048;
const FIRST_MATCH_ON_UNLISTED_DATE: u64 = 560042;
const SECOND_MATCH_ON_UNLISTED_DATE: u64 = 560043;

const FD_MATCHES: &str = include_str!("fixtures/football-data/matches.json");
const FD_STANDINGS: &str = include_str!("fixtures/football-data/standings.json");
const FD_SCORERS: &str = include_str!("fixtures/football-data/scorers.json");
const HL_MATCHES: &str = include_str!("fixtures/highlightly/matches-2026-09-20.json");
const HL_LINEUPS: &str = include_str!("fixtures/highlightly/lineups-1180004.json");
const HL_EVENTS: &str = include_str!("fixtures/highlightly/events-1180004.json");
const HL_EMPTY_LINEUPS: &str = r#"{
    "homeTeam": {"id": 4341, "name": "Leeds", "formation": null, "initialLineup": [], "substitutes": []},
    "awayTeam": {"id": 4354, "name": "Crystal Palace", "formation": null, "initialLineup": [], "substitutes": []}
}"#;

#[derive(Clone, Default)]
struct Upstream {
    hits: Arc<Mutex<Vec<String>>>,
    football_data_fails: bool,
    football_data_failing_resource: Option<&'static str>,
    highlightly_remaining: Option<u32>,
    highlightly_rate_limited: bool,
}

impl Upstream {
    fn hits(&self, path: &str) -> usize {
        self.hits
            .lock()
            .unwrap()
            .iter()
            .filter(|hit| *hit == path)
            .count()
    }

    fn hits_with_prefix(&self, prefix: &str) -> usize {
        self.hits
            .lock()
            .unwrap()
            .iter()
            .filter(|hit| hit.starts_with(prefix))
            .count()
    }
}

fn json(body: &'static str) -> Response {
    ([("content-type", "application/json")], body).into_response()
}

fn has_key(headers: &HeaderMap, name: &str, key: &str) -> bool {
    headers.get(name).and_then(|value| value.to_str().ok()) == Some(key)
}

async fn football_data(
    State(upstream): State<Upstream>,
    Path(resource): Path<String>,
    headers: HeaderMap,
) -> Response {
    if upstream.football_data_fails
        || upstream.football_data_failing_resource == Some(resource.as_str())
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    if !has_key(&headers, "x-auth-token", FOOTBALL_DATA_KEY) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match resource.as_str() {
        "matches" => json(FD_MATCHES),
        "standings" => json(FD_STANDINGS),
        "scorers" => json(FD_SCORERS),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn highlightly_matches(
    headers: HeaderMap,
    Query(query): Query<Vec<(String, String)>>,
) -> Response {
    if !has_key(&headers, "x-rapidapi-key", HIGHLIGHTLY_KEY) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let has = |name: &str, value: &str| query.iter().any(|(k, v)| k == name && v == value);
    if has("leagueId", "33973") && has("date", "2026-09-20") {
        json(HL_MATCHES)
    } else {
        json(r#"{"data":[],"pagination":{"totalCount":0,"offset":0,"limit":100}}"#)
    }
}

async fn highlightly_match_data(
    headers: HeaderMap,
    Path((resource, id)): Path<(String, u64)>,
) -> Response {
    if !has_key(&headers, "x-rapidapi-key", HIGHLIGHTLY_KEY) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match (resource.as_str(), id) {
        ("lineups", 1180004) => json(HL_LINEUPS),
        ("events", 1180004) => json(HL_EVENTS),
        ("lineups", 1180002) => json(HL_EMPTY_LINEUPS),
        ("events", 1180002) => json("[]"),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn record_hit(State(upstream): State<Upstream>, request: Request, next: Next) -> Response {
    upstream
        .hits
        .lock()
        .unwrap()
        .push(request.uri().path().to_owned());
    if !request.uri().path().starts_with("/hl/") {
        return next.run(request).await;
    }
    if upstream.highlightly_rate_limited {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [("content-type", "application/json")],
            r#"{"message":"Too many requests","statusCode":429}"#,
        )
            .into_response();
    }
    let mut response = next.run(request).await;
    if let Some(remaining) = upstream.highlightly_remaining {
        response.headers_mut().insert(
            "x-ratelimit-requests-remaining",
            remaining.to_string().parse().unwrap(),
        );
    }
    response
}

async fn start_upstream(upstream: Upstream) -> SocketAddr {
    let router = Router::new()
        .route("/fd/competitions/PL/{resource}", get(football_data))
        .route("/hl/matches", get(highlightly_matches))
        .route("/hl/{resource}/{id}", get(highlightly_match_data))
        .layer(middleware::from_fn_with_state(upstream.clone(), record_hit))
        .with_state(upstream);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    address
}

struct Setup {
    football_data_key: Option<&'static str>,
    highlightly_key: Option<&'static str>,
    budget: u32,
    upstream: Upstream,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            football_data_key: Some(FOOTBALL_DATA_KEY),
            highlightly_key: Some(HIGHLIGHTLY_KEY),
            budget: 90,
            upstream: Upstream::default(),
        }
    }
}

impl Setup {
    async fn start(self) -> (Router, Upstream) {
        let address = start_upstream(self.upstream.clone()).await;
        let config = Config {
            football_data: SourceConfig {
                base_url: format!("http://{address}/fd"),
                api_key: self.football_data_key.map(ApiKey::new),
            },
            highlightly: SourceConfig {
                base_url: format!("http://{address}/hl"),
                api_key: self.highlightly_key.map(ApiKey::new),
            },
            highlightly_daily_budget: self.budget,
        };
        let service = DataService::new(config, build_client());
        (app(PathBuf::from("../frontend"), service), self.upstream)
    }
}

async fn get_json(app: &Router, uri: &str) -> (StatusCode, Value) {
    let (status, body) = get_text(app, uri).await;
    let value = serde_json::from_str(&body).unwrap_or_else(|_| panic!("not JSON: {body}"));
    (status, value)
}

async fn get_text(app: &Router, uri: &str) -> (StatusCode, String) {
    let response = app
        .clone()
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&body).into_owned())
}

fn lineups_uri(id: u64) -> String {
    format!("/api/matches/{id}/lineups")
}

fn events_uri(id: u64) -> String {
    format!("/api/matches/{id}/events")
}

#[tokio::test]
async fn hello_returns_greeting() {
    let (app, _) = Setup::default().start().await;

    let (status, body) = get_text(&app, "/api/hello").await;

    assert!(status.is_success());
    assert!(body.contains("Hello, world!"));
}

#[tokio::test]
async fn healthz_answers_ok_without_keys() {
    let (app, _) = Setup {
        football_data_key: None,
        highlightly_key: None,
        ..Setup::default()
    }
    .start()
    .await;

    assert_eq!(
        get_text(&app, "/healthz").await,
        (StatusCode::OK, "ok".into())
    );
}

#[tokio::test]
async fn routes_without_keys_answer_503_with_the_variable_names() {
    let (app, upstream) = Setup {
        football_data_key: None,
        highlightly_key: None,
        ..Setup::default()
    }
    .start()
    .await;
    let both = ["FOOTBALL_DATA_API_KEY", "HIGHLIGHTLY_API_KEY"];
    let cases = [
        ("/api/matches".to_owned(), &both[..1]),
        ("/api/table".to_owned(), &both[..1]),
        ("/api/scorers".to_owned(), &both[..1]),
        (lineups_uri(FINISHED_MATCH), &both[..]),
        (events_uri(FINISHED_MATCH), &both[..]),
    ];

    for (uri, variables) in cases {
        let (status, body) = get_json(&app, &uri).await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{uri}");
        assert_eq!(
            body["missing_variables"],
            serde_json::json!(variables),
            "{uri}"
        );
        for variable in variables {
            assert!(body["error"].as_str().unwrap().contains(variable), "{uri}");
        }
    }
    assert_eq!(upstream.hits_with_prefix("/"), 0);
}

#[tokio::test]
async fn post_match_routes_name_only_the_missing_highlightly_key() {
    let (app, _) = Setup {
        highlightly_key: None,
        ..Setup::default()
    }
    .start()
    .await;

    let (status, body) = get_json(&app, &lineups_uri(FINISHED_MATCH)).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        body["missing_variables"],
        serde_json::json!(["HIGHLIGHTLY_API_KEY"])
    );
}

#[tokio::test]
async fn upstream_failure_answers_502_without_the_key() {
    let (app, _) = Setup {
        upstream: Upstream {
            football_data_fails: true,
            ..Upstream::default()
        },
        ..Setup::default()
    }
    .start()
    .await;

    for uri in ["/api/matches", "/api/table", "/api/scorers"] {
        let (status, body) = get_text(&app, uri).await;

        assert_eq!(status, StatusCode::BAD_GATEWAY, "{uri}");
        let body: Value = serde_json::from_str(&body).unwrap();
        let error = body["error"].as_str().unwrap();
        assert!(error.contains("football-data.org"), "{error}");
        assert!(error.contains("500"), "{error}");
        assert!(!error.contains(FOOTBALL_DATA_KEY));
    }
}

#[tokio::test]
async fn rejected_key_answers_502() {
    let (app, _) = Setup {
        football_data_key: Some("wrong-key"),
        ..Setup::default()
    }
    .start()
    .await;

    let (status, body) = get_text(&app, "/api/matches").await;

    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert!(body.contains("403"));
    assert!(!body.contains("wrong-key"));
}

#[tokio::test]
async fn unreachable_upstream_answers_502() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let closed_address = listener.local_addr().unwrap();
    drop(listener);
    let config = Config {
        football_data: SourceConfig {
            base_url: format!("http://{closed_address}/fd"),
            api_key: Some(ApiKey::new(FOOTBALL_DATA_KEY)),
        },
        highlightly: SourceConfig {
            base_url: format!("http://{closed_address}/hl"),
            api_key: None,
        },
        highlightly_daily_budget: 90,
    };
    let app = app(
        PathBuf::from("../frontend"),
        DataService::new(config, build_client()),
    );

    let (status, body) = get_json(&app, "/api/matches").await;

    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("football-data.org")
    );
}

#[tokio::test]
async fn matches_come_from_the_cache_on_the_second_request() {
    let (app, upstream) = Setup::default().start().await;

    let (first_status, first) = get_json(&app, "/api/matches").await;
    let (second_status, second) = get_json(&app, "/api/matches").await;

    assert_eq!(
        (first_status, second_status),
        (StatusCode::OK, StatusCode::OK)
    );
    assert_eq!(first, second);
    assert_eq!(upstream.hits("/fd/competitions/PL/matches"), 1);
    let matches = first["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 60);
    assert_eq!(matches[0]["home"]["id"], "arsenal");
    assert_eq!(matches[0]["status"], "finished");
    assert_eq!(matches[0]["kickoff"], "2026-08-21T19:00:00Z");
    assert_eq!(
        matches[0]["score"]["full_time"],
        serde_json::json!({"home": 3, "away": 0})
    );
}

#[tokio::test]
async fn responses_hold_no_crest_or_logo_urls() {
    let (app, _) = Setup::default().start().await;

    for uri in [
        "/api/matches".to_owned(),
        "/api/table".to_owned(),
        "/api/scorers".to_owned(),
        lineups_uri(FINISHED_MATCH),
        events_uri(FINISHED_MATCH),
    ] {
        let (status, body) = get_text(&app, &uri).await;

        assert_eq!(status, StatusCode::OK, "{uri}");
        assert!(!body.contains("crest") && !body.contains("logo"), "{uri}");
        assert!(!body.contains("https://"), "{uri}");
    }
}

#[tokio::test]
async fn table_holds_the_official_table_and_the_check() {
    let (app, upstream) = Setup::default().start().await;

    let (status, body) = get_json(&app, "/api/table").await;
    get_json(&app, "/api/table").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["table"].as_array().unwrap().len(), 20);
    assert_eq!(body["table"][0]["team"]["id"], "man-city");
    assert_eq!(body["table"][0]["points"], 15);
    assert_eq!(body["check"]["matches_official"], true);
    assert_eq!(body["check"]["differences"], serde_json::json!([]));
    assert_eq!(upstream.hits("/fd/competitions/PL/standings"), 1);
    assert_eq!(upstream.hits("/fd/competitions/PL/matches"), 1);
}

#[tokio::test]
async fn scorers_are_listed() {
    let (app, upstream) = Setup::default().start().await;

    let (status, body) = get_json(&app, "/api/scorers").await;
    get_json(&app, "/api/scorers").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["scorers"][0]["player"], "Example Striker");
    assert_eq!(body["scorers"][0]["team"]["id"], "man-city");
    assert_eq!(body["scorers"][0]["assists"], Value::Null);
    assert_eq!(upstream.hits("/fd/competitions/PL/scorers"), 1);
}

#[tokio::test]
async fn lineups_and_events_of_a_finished_match_are_fetched_once() {
    let (app, upstream) = Setup::default().start().await;

    let (lineups_status, lineups) = get_json(&app, &lineups_uri(FINISHED_MATCH)).await;
    let (events_status, events) = get_json(&app, &events_uri(FINISHED_MATCH)).await;
    get_json(&app, &lineups_uri(FINISHED_MATCH)).await;
    get_json(&app, &events_uri(FINISHED_MATCH)).await;

    assert_eq!(
        (lineups_status, events_status),
        (StatusCode::OK, StatusCode::OK)
    );
    assert_eq!(lineups["home"]["team"]["id"], "fulham");
    assert_eq!(lineups["away"]["team"]["id"], "man-utd");
    assert_eq!(lineups["home"]["formation"], "4-2-3-1");
    assert_eq!(events["events"].as_array().unwrap().len(), 6);
    assert_eq!(events["events"][1]["kind"], "goal");
    assert_eq!(events["events"][5]["added_time"], 3);
    assert_eq!(upstream.hits("/hl/matches"), 1);
    assert_eq!(upstream.hits("/hl/lineups/1180004"), 1);
    assert_eq!(upstream.hits("/hl/events/1180004"), 1);
}

#[tokio::test]
async fn links_for_the_same_date_are_reused() {
    let (app, upstream) = Setup::default().start().await;

    get_json(&app, &lineups_uri(FINISHED_MATCH)).await;
    let (status, _) = get_json(&app, &lineups_uri(OTHER_FINISHED_MATCH_SAME_DAY)).await;

    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(upstream.hits("/hl/matches"), 1);
    assert_eq!(upstream.hits("/hl/lineups/1180001"), 1);
}

#[tokio::test]
async fn unfinished_match_answers_409_without_a_highlightly_call() {
    let (app, upstream) = Setup::default().start().await;

    for uri in [lineups_uri(FUTURE_MATCH), events_uri(FUTURE_MATCH)] {
        let (status, body) = get_json(&app, &uri).await;

        assert_eq!(status, StatusCode::CONFLICT, "{uri}");
        assert!(body["error"].as_str().unwrap().contains("not finished"));
    }
    assert_eq!(upstream.hits_with_prefix("/hl/"), 0);
}

#[tokio::test]
async fn unknown_match_answers_404() {
    let (app, upstream) = Setup::default().start().await;

    let (status, body) = get_json(&app, &lineups_uri(1)).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].as_str().unwrap().contains('1'));
    assert_eq!(upstream.hits_with_prefix("/hl/"), 0);
}

#[tokio::test]
async fn used_budget_stops_highlightly_calls_but_serves_cached_data() {
    let (app, upstream) = Setup {
        budget: 2,
        ..Setup::default()
    }
    .start()
    .await;

    let (lineups_status, _) = get_json(&app, &lineups_uri(FINISHED_MATCH)).await;
    let (events_status, events) = get_json(&app, &events_uri(FINISHED_MATCH)).await;
    let (cached_status, _) = get_json(&app, &lineups_uri(FINISHED_MATCH)).await;

    assert_eq!(lineups_status, StatusCode::OK);
    assert_eq!(events_status, StatusCode::SERVICE_UNAVAILABLE);
    let error = events["error"].as_str().unwrap();
    assert!(
        error.contains("budget of 2 Highlightly requests"),
        "{error}"
    );
    assert!(error.contains("00:00 UTC"), "{error}");
    assert_eq!(cached_status, StatusCode::OK);
    assert_eq!(upstream.hits_with_prefix("/hl/"), 2);
}

#[tokio::test]
async fn a_date_lookup_that_links_nothing_is_remembered() {
    let (app, upstream) = Setup::default().start().await;

    for uri in [
        lineups_uri(FIRST_MATCH_ON_UNLISTED_DATE),
        events_uri(FIRST_MATCH_ON_UNLISTED_DATE),
        lineups_uri(SECOND_MATCH_ON_UNLISTED_DATE),
    ] {
        let (status, body) = get_json(&app, &uri).await;

        assert_eq!(status, StatusCode::BAD_GATEWAY, "{uri}");
        assert!(
            body["error"].as_str().unwrap().contains("free plan"),
            "{uri}"
        );
    }
    assert_eq!(upstream.hits("/hl/matches"), 1);
    assert_eq!(upstream.hits_with_prefix("/hl/"), 1);
}

#[tokio::test]
async fn empty_lineups_and_events_are_remembered_for_a_while() {
    let (app, upstream) = Setup::default().start().await;

    for _ in 0..2 {
        let (lineups_status, lineups) = get_json(&app, &lineups_uri(MATCH_WITH_EMPTY_DATA)).await;
        let (events_status, events) = get_json(&app, &events_uri(MATCH_WITH_EMPTY_DATA)).await;

        assert_eq!(
            (lineups_status, events_status),
            (StatusCode::OK, StatusCode::OK)
        );
        assert_eq!(lineups["home"]["starting_rows"], serde_json::json!([]));
        assert_eq!(events["events"], serde_json::json!([]));
    }
    assert_eq!(upstream.hits("/hl/lineups/1180002"), 1);
    assert_eq!(upstream.hits("/hl/events/1180002"), 1);
}

async fn start_with_highlightly_remaining(remaining: u32) -> (Router, Upstream) {
    Setup {
        upstream: Upstream {
            highlightly_remaining: Some(remaining),
            ..Upstream::default()
        },
        ..Setup::default()
    }
    .start()
    .await
}

#[tokio::test]
async fn highlightly_calls_continue_while_more_than_10_remain() {
    let (app, upstream) = start_with_highlightly_remaining(11).await;

    let (status, _) = get_json(&app, &lineups_uri(FINISHED_MATCH)).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(upstream.hits_with_prefix("/hl/"), 2);
}

#[tokio::test]
async fn highlightly_calls_stop_when_10_or_fewer_remain() {
    let (app, upstream) = start_with_highlightly_remaining(10).await;

    let (lineups_status, lineups) = get_json(&app, &lineups_uri(FINISHED_MATCH)).await;
    let (events_status, _) = get_json(&app, &events_uri(OTHER_FINISHED_MATCH_SAME_DAY)).await;

    assert_eq!(lineups_status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(events_status, StatusCode::SERVICE_UNAVAILABLE);
    let error = lineups["error"].as_str().unwrap();
    assert!(error.contains("10 requests left"), "{error}");
    assert!(error.contains("00:00 UTC"), "{error}");
    assert_eq!(upstream.hits_with_prefix("/hl/"), 1);
}

#[tokio::test]
async fn a_highlightly_429_stops_calls_until_the_next_day() {
    let (app, upstream) = Setup {
        upstream: Upstream {
            highlightly_rate_limited: true,
            ..Upstream::default()
        },
        ..Setup::default()
    }
    .start()
    .await;

    let (first_status, first) = get_json(&app, &lineups_uri(FINISHED_MATCH)).await;
    let (second_status, second) = get_json(&app, &events_uri(FINISHED_MATCH)).await;

    assert_eq!(first_status, StatusCode::BAD_GATEWAY);
    assert!(first["error"].as_str().unwrap().contains("429"));
    assert_eq!(second_status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        second["error"]
            .as_str()
            .unwrap()
            .contains("limit is used up")
    );
    assert_eq!(upstream.hits_with_prefix("/hl/"), 1);
}

#[tokio::test]
async fn a_football_data_error_is_remembered_for_its_resource() {
    let (app, upstream) = Setup {
        upstream: Upstream {
            football_data_fails: true,
            ..Upstream::default()
        },
        ..Setup::default()
    }
    .start()
    .await;

    for uri in ["/api/matches", "/api/matches", "/api/table", "/api/scorers"] {
        let (status, _) = get_json(&app, uri).await;

        assert_eq!(status, StatusCode::BAD_GATEWAY, "{uri}");
    }
    assert_eq!(upstream.hits_with_prefix("/fd/"), 1);
}

#[tokio::test]
async fn a_remembered_error_affects_only_its_resource() {
    let (app, upstream) = Setup {
        upstream: Upstream {
            football_data_failing_resource: Some("standings"),
            ..Upstream::default()
        },
        ..Setup::default()
    }
    .start()
    .await;

    let (first_table, _) = get_json(&app, "/api/table").await;
    let (second_table, _) = get_json(&app, "/api/table").await;
    let (matches, _) = get_json(&app, "/api/matches").await;
    let (scorers, _) = get_json(&app, "/api/scorers").await;

    assert_eq!(
        (first_table, second_table),
        (StatusCode::BAD_GATEWAY, StatusCode::BAD_GATEWAY)
    );
    assert_eq!((matches, scorers), (StatusCode::OK, StatusCode::OK));
    assert_eq!(upstream.hits("/fd/competitions/PL/standings"), 1);
}
