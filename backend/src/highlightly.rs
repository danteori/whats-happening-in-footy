use chrono::{DateTime, NaiveDate, Utc};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, de::DeserializeOwned};

use crate::{
    budget::Quota,
    clubs::club_for_name,
    config::ApiKey,
    domain::{EventKind, Goals, LineupPlayer, MatchEvent, MatchLineups, TeamLineup},
    upstream::{Reply, SourceError, send, with_key},
};

pub const SOURCE_NAME: &str = "Highlightly";
pub const PREMIER_LEAGUE_ID: &str = "33973";
const KEY_HEADER: &str = "x-rapidapi-key";
const REMAINING_HEADER: &str = "x-ratelimit-requests-remaining";
const PAGE_LIMIT: &str = "100";

#[derive(Debug, Clone)]
pub struct HighlightlyClient {
    http: Client,
    base_url: String,
    api_key: ApiKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightlyMatch {
    pub id: u64,
    pub kickoff: DateTime<Utc>,
    pub home_team: String,
    pub away_team: String,
    pub state: String,
    pub score: Option<Goals>,
}

impl HighlightlyClient {
    pub fn new(http: Client, base_url: String, api_key: ApiKey) -> Self {
        Self {
            http,
            base_url,
            api_key,
        }
    }

    pub async fn matches_on(&self, date: NaiveDate) -> Quoted<Vec<HighlightlyMatch>> {
        let date = date.format("%Y-%m-%d").to_string();
        self.get(
            "/matches",
            &[
                ("leagueId", PREMIER_LEAGUE_ID),
                ("date", &date),
                ("timezone", "Etc/UTC"),
                ("limit", PAGE_LIMIT),
            ],
        )
        .await
        .and_then(matches_from)
    }

    pub async fn lineups(&self, match_id: u64) -> Quoted<MatchLineups> {
        self.get(&format!("/lineups/{match_id}"), &[])
            .await
            .and_then(lineups_from)
    }

    pub async fn events(&self, match_id: u64) -> Quoted<Vec<MatchEvent>> {
        self.get(&format!("/events/{match_id}"), &[])
            .await
            .and_then(events_from)
    }

    async fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, &str)]) -> Quoted<T> {
        let request = self
            .http
            .get(format!("{}{path}", self.base_url))
            .query(query);
        let reply = match with_key(request, KEY_HEADER, &self.api_key, SOURCE_NAME) {
            Ok(request) => send(request, SOURCE_NAME).await,
            Err(error) => Err(error),
        };
        match reply {
            Ok(reply) => Quoted {
                quota: quota_from(&reply),
                result: reply.json(SOURCE_NAME),
            },
            Err(error) => Quoted {
                result: Err(error),
                quota: Quota::Unknown,
            },
        }
    }
}

#[derive(Debug)]
pub struct Quoted<T> {
    pub result: Result<T, SourceError>,
    pub quota: Quota,
}

impl<T> Quoted<T> {
    fn and_then<U>(self, next: impl FnOnce(T) -> Result<U, SourceError>) -> Quoted<U> {
        Quoted {
            result: self.result.and_then(next),
            quota: self.quota,
        }
    }
}

fn quota_from(reply: &Reply) -> Quota {
    if reply.status() == StatusCode::TOO_MANY_REQUESTS {
        return Quota::UsedUp;
    }
    reply
        .header_number(REMAINING_HEADER)
        .map_or(Quota::Unknown, Quota::Remaining)
}

#[derive(Debug, Deserialize)]
struct Plan {
    tier: Option<String>,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MatchesPage {
    data: Vec<RawMatch>,
    plan: Option<Plan>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMatch {
    id: u64,
    date: DateTime<Utc>,
    home_team: RawTeam,
    away_team: RawTeam,
    state: RawState,
}

#[derive(Debug, Deserialize)]
struct RawTeam {
    name: String,
}

#[derive(Debug, Deserialize)]
struct RawState {
    description: String,
    score: Option<RawScore>,
}

#[derive(Debug, Deserialize)]
struct RawScore {
    current: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LineupsResponse {
    home_team: RawLineupTeam,
    away_team: RawLineupTeam,
    plan: Option<Plan>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawLineupTeam {
    name: String,
    formation: Option<String>,
    #[serde(default)]
    initial_lineup: Vec<Vec<RawLineupPlayer>>,
    #[serde(default)]
    substitutes: Vec<RawLineupPlayer>,
}

#[derive(Debug, Deserialize)]
struct RawLineupPlayer {
    name: String,
    number: Option<u32>,
    position: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum EventsResponse {
    List(Vec<RawEvent>),
    Page {
        data: Vec<RawEvent>,
        plan: Option<Plan>,
    },
}

#[derive(Debug, Deserialize)]
struct RawEvent {
    team: RawTeam,
    time: String,
    #[serde(rename = "type")]
    kind: String,
    player: Option<String>,
    assist: Option<String>,
    substituted: Option<String>,
}

fn log_plan(plan: Option<&Plan>) {
    if let Some(Plan {
        tier,
        message: Some(message),
    }) = plan
    {
        tracing::info!(
            tier = tier.as_deref().unwrap_or("unknown"),
            "Highlightly plan notice: {message}"
        );
    }
}

fn shape_error(detail: String) -> SourceError {
    SourceError::Shape {
        source_name: SOURCE_NAME,
        detail,
    }
}

fn matches_from(page: MatchesPage) -> Result<Vec<HighlightlyMatch>, SourceError> {
    log_plan(page.plan.as_ref());
    page.data
        .into_iter()
        .map(|raw| {
            let score = raw
                .state
                .score
                .and_then(|score| score.current)
                .map(|current| parse_score(&current))
                .transpose()?
                .flatten();
            Ok(HighlightlyMatch {
                id: raw.id,
                kickoff: raw.date,
                home_team: raw.home_team.name,
                away_team: raw.away_team.name,
                state: raw.state.description,
                score,
            })
        })
        .collect()
}

pub fn parse_score(text: &str) -> Result<Option<Goals>, SourceError> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    let invalid = || shape_error(format!("the score {text:?} is not in the form \"2 - 1\""));
    let (home, away) = text.split_once('-').ok_or_else(invalid)?;
    let home = home.trim().parse().map_err(|_| invalid())?;
    let away = away.trim().parse().map_err(|_| invalid())?;
    Ok(Some(Goals { home, away }))
}

fn lineups_from(response: LineupsResponse) -> Result<MatchLineups, SourceError> {
    log_plan(response.plan.as_ref());
    Ok(MatchLineups {
        home: team_lineup_from(response.home_team)?,
        away: team_lineup_from(response.away_team)?,
    })
}

fn team_lineup_from(raw: RawLineupTeam) -> Result<TeamLineup, SourceError> {
    Ok(TeamLineup {
        team: club_for_name(&raw.name)?,
        formation: raw.formation,
        starting_rows: raw
            .initial_lineup
            .into_iter()
            .map(|row| row.into_iter().map(lineup_player_from).collect())
            .collect(),
        substitutes: raw
            .substitutes
            .into_iter()
            .map(lineup_player_from)
            .collect(),
    })
}

fn lineup_player_from(raw: RawLineupPlayer) -> LineupPlayer {
    LineupPlayer {
        name: raw.name,
        number: raw.number,
        position: raw.position,
    }
}

fn events_from(response: EventsResponse) -> Result<Vec<MatchEvent>, SourceError> {
    let events = match response {
        EventsResponse::List(events) => events,
        EventsResponse::Page { data, plan } => {
            log_plan(plan.as_ref());
            data
        }
    };
    events.into_iter().map(event_from).collect()
}

fn event_from(raw: RawEvent) -> Result<MatchEvent, SourceError> {
    let (minute, added_time) = parse_event_time(&raw.time)?;
    Ok(MatchEvent {
        team: club_for_name(&raw.team.name)?,
        minute,
        added_time,
        kind: event_kind(&raw.kind),
        player: raw.player,
        assist: raw.assist,
        substituted: raw.substituted,
    })
}

pub fn parse_event_time(text: &str) -> Result<(u32, Option<u32>), SourceError> {
    let invalid = || {
        shape_error(format!(
            "the event time {text:?} is not in the form \"45+1\""
        ))
    };
    let trimmed = text.trim().trim_end_matches('\'');
    let (minute, added) = match trimmed.split_once('+') {
        Some((minute, added)) => (minute, Some(added)),
        None => (trimmed, None),
    };
    let minute = minute.trim().parse().map_err(|_| invalid())?;
    let added = added
        .map(|added| added.trim().parse().map_err(|_| invalid()))
        .transpose()?;
    Ok((minute, added))
}

fn event_kind(text: &str) -> EventKind {
    match text {
        "Goal" => EventKind::Goal,
        "Own Goal" => EventKind::OwnGoal,
        "Penalty" => EventKind::PenaltyGoal,
        "Missed Penalty" => EventKind::MissedPenalty,
        "Yellow Card" => EventKind::YellowCard,
        "Red Card" => EventKind::RedCard,
        "Substitution" => EventKind::Substitution,
        "VAR Goal Confirmed" => EventKind::VarGoalConfirmed,
        "VAR Goal Cancelled" => EventKind::VarGoalCancelled,
        "VAR Goal Cancelled - Offside" => EventKind::VarGoalCancelledOffside,
        "VAR Penalty" => EventKind::VarPenalty,
        "VAR Penalty Cancelled" => EventKind::VarPenaltyCancelled,
        other => {
            tracing::warn!("Highlightly sent an unknown event type {other:?}");
            EventKind::Other
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::upstream::parse_json;

    const MATCHES: &str = include_str!("../tests/fixtures/highlightly/matches-2026-09-20.json");
    const LINEUPS: &str = include_str!("../tests/fixtures/highlightly/lineups-1180004.json");
    const EVENTS: &str = include_str!("../tests/fixtures/highlightly/events-1180004.json");

    #[test]
    fn matches_parse_with_score_strings() {
        let matches = matches_from(parse_json(MATCHES.as_bytes(), SOURCE_NAME).unwrap()).unwrap();

        assert_eq!(matches.len(), 4);
        let last = &matches[3];
        assert_eq!(last.id, 1180004);
        assert_eq!(last.home_team, "Fulham");
        assert_eq!(last.away_team, "Manchester United");
        assert_eq!(last.kickoff.to_rfc3339(), "2026-09-20T15:30:00+00:00");
        assert_eq!(last.state, "Finished");
        assert_eq!(last.score, Some(Goals { home: 1, away: 1 }));
    }

    #[test]
    fn matches_without_plan_or_score_parse() {
        let json = r#"{"data":[{"id":7,"round":"Regular Season - 6","date":"2026-10-10T11:30:00.000Z",
            "homeTeam":{"id":1,"name":"Arsenal"},"awayTeam":{"id":2,"name":"Leeds"},
            "state":{"description":"Not started","score":{}}}],
            "pagination":{"totalCount":1,"offset":0,"limit":100}}"#;

        let matches = matches_from(parse_json(json.as_bytes(), SOURCE_NAME).unwrap()).unwrap();

        assert_eq!(matches[0].score, None);
    }

    #[test]
    fn score_strings_parse() {
        assert_eq!(
            parse_score("2 - 1").unwrap(),
            Some(Goals { home: 2, away: 1 })
        );
        assert_eq!(
            parse_score("0-0").unwrap(),
            Some(Goals { home: 0, away: 0 })
        );
        assert_eq!(
            parse_score(" 10 - 3 ").unwrap(),
            Some(Goals { home: 10, away: 3 })
        );
        assert_eq!(parse_score("").unwrap(), None);
        for invalid in ["2 : 1", "two - one", "2 -", "-1 - 2"] {
            assert!(parse_score(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn event_times_parse() {
        assert_eq!(parse_event_time("38").unwrap(), (38, None));
        assert_eq!(parse_event_time("90+3").unwrap(), (90, Some(3)));
        assert_eq!(parse_event_time("45 + 1'").unwrap(), (45, Some(1)));
        assert!(parse_event_time("HT").is_err());
    }

    #[test]
    fn lineups_parse_into_rows() {
        let lineups = lineups_from(parse_json(LINEUPS.as_bytes(), SOURCE_NAME).unwrap()).unwrap();

        assert_eq!(lineups.home.team.id, "fulham");
        assert_eq!(lineups.away.team.id, "man-utd");
        assert_eq!(lineups.home.formation.as_deref(), Some("4-2-3-1"));
        let row_sizes: Vec<_> = lineups.home.starting_rows.iter().map(Vec::len).collect();
        assert_eq!(row_sizes, [1, 4, 2, 3, 1]);
        assert_eq!(
            lineups.home.starting_rows[0][0].position.as_deref(),
            Some("Goalkeeper")
        );
        assert_eq!(lineups.away.substitutes.len(), 4);
        assert!(lineups.is_complete());
    }

    #[test]
    fn lineups_accept_a_plan_object() {
        let json = r#"{"homeTeam":{"id":1,"name":"Arsenal","formation":"4-3-3","initialLineup":[],"substitutes":[]},
            "awayTeam":{"id":2,"name":"Leeds","formation":"4-4-2","initialLineup":[],"substitutes":[]},
            "plan":{"tier":"BASIC","message":"Some results might be hidden with FREE tier."}}"#;

        let lineups = lineups_from(parse_json(json.as_bytes(), SOURCE_NAME).unwrap()).unwrap();

        assert!(!lineups.is_complete());
    }

    #[test]
    fn events_parse_with_kinds_and_added_time() {
        let events = events_from(parse_json(EVENTS.as_bytes(), SOURCE_NAME).unwrap()).unwrap();

        let kinds: Vec<_> = events.iter().map(|event| event.kind).collect();
        assert_eq!(
            kinds,
            [
                EventKind::YellowCard,
                EventKind::Goal,
                EventKind::Substitution,
                EventKind::VarGoalCancelledOffside,
                EventKind::PenaltyGoal,
                EventKind::RedCard,
            ]
        );
        assert_eq!(events[1].team.id, "man-utd");
        assert_eq!(events[1].assist.as_deref(), Some("United Player 9"));
        assert_eq!(events[2].substituted.as_deref(), Some("Fulham Player 10"));
        assert_eq!((events[5].minute, events[5].added_time), (90, Some(3)));
    }

    #[test]
    fn events_accept_a_page_with_a_plan() {
        let json = r#"{"data":[{"team":{"id":1,"name":"Arsenal"},"time":"12","type":"Own Goal"},
            {"team":{"id":1,"name":"Arsenal"},"time":"80","type":"Something New"}],
            "plan":{"tier":"BASIC","message":"Some results might be hidden with FREE tier."}}"#;

        let events = events_from(parse_json(json.as_bytes(), SOURCE_NAME).unwrap()).unwrap();

        assert_eq!(events[0].kind, EventKind::OwnGoal);
        assert_eq!(events[1].kind, EventKind::Other);
    }
}
