use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Deserialize;

use crate::{
    clubs::{Club, club_for_name},
    config::ApiKey,
    domain::{Goals, Match, MatchStatus, Score, Scorer, TableRow},
    upstream::{SourceError, fetch_json, with_key},
};

pub const SOURCE_NAME: &str = "football-data.org";
const KEY_HEADER: &str = "x-auth-token";
const SCORERS_LIMIT: &str = "20";

#[derive(Debug, Clone)]
pub struct FootballDataClient {
    http: Client,
    base_url: String,
    api_key: ApiKey,
}

impl FootballDataClient {
    pub fn new(http: Client, base_url: String, api_key: ApiKey) -> Self {
        Self {
            http,
            base_url,
            api_key,
        }
    }

    pub async fn matches(&self) -> Result<Vec<Match>, SourceError> {
        let response: MatchesResponse = self.get("/competitions/PL/matches", &[]).await?;
        matches_from(response)
    }

    pub async fn standings(&self) -> Result<Vec<TableRow>, SourceError> {
        let response: StandingsResponse = self.get("/competitions/PL/standings", &[]).await?;
        table_from(response)
    }

    pub async fn scorers(&self) -> Result<Vec<Scorer>, SourceError> {
        let response: ScorersResponse = self
            .get("/competitions/PL/scorers", &[("limit", SCORERS_LIMIT)])
            .await?;
        scorers_from(response)
    }

    async fn get<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T, SourceError> {
        let request = self
            .http
            .get(format!("{}{path}", self.base_url))
            .query(query);
        let request = with_key(request, KEY_HEADER, &self.api_key, SOURCE_NAME)?;
        fetch_json(request, SOURCE_NAME).await
    }
}

#[derive(Debug, Deserialize)]
struct MatchesResponse {
    matches: Vec<RawMatch>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMatch {
    id: u64,
    utc_date: DateTime<Utc>,
    status: RawStatus,
    matchday: Option<u32>,
    home_team: RawTeam,
    away_team: RawTeam,
    score: RawScore,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum RawStatus {
    Scheduled,
    Timed,
    InPlay,
    Paused,
    ExtraTime,
    PenaltyShootout,
    Finished,
    Suspended,
    Postponed,
    Cancelled,
    Awarded,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
struct RawTeam {
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawScore {
    #[serde(default)]
    full_time: RawGoals,
    #[serde(default)]
    half_time: RawGoals,
}

#[derive(Debug, Default, Deserialize)]
struct RawGoals {
    home: Option<u32>,
    away: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct StandingsResponse {
    standings: Vec<RawStanding>,
}

#[derive(Debug, Deserialize)]
struct RawStanding {
    #[serde(rename = "type")]
    kind: String,
    table: Vec<RawTableRow>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTableRow {
    position: u32,
    team: RawTeam,
    played_games: u32,
    won: u32,
    draw: u32,
    lost: u32,
    points: u32,
    goals_for: u32,
    goals_against: u32,
    goal_difference: i32,
}

#[derive(Debug, Deserialize)]
struct ScorersResponse {
    scorers: Vec<RawScorer>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawScorer {
    player: RawPlayer,
    team: RawTeam,
    played_matches: Option<u32>,
    goals: u32,
    assists: Option<u32>,
    penalties: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct RawPlayer {
    name: String,
}

fn matches_from(response: MatchesResponse) -> Result<Vec<Match>, SourceError> {
    response.matches.into_iter().map(match_from).collect()
}

fn match_from(raw: RawMatch) -> Result<Match, SourceError> {
    Ok(Match {
        id: raw.id,
        kickoff: raw.utc_date,
        round: raw.matchday,
        status: status_from(raw.status),
        home: club(&raw.home_team)?,
        away: club(&raw.away_team)?,
        score: Score {
            full_time: goals_from(&raw.score.full_time),
            half_time: goals_from(&raw.score.half_time),
        },
    })
}

fn status_from(raw: RawStatus) -> MatchStatus {
    match raw {
        RawStatus::Scheduled | RawStatus::Timed => MatchStatus::Scheduled,
        RawStatus::InPlay | RawStatus::ExtraTime | RawStatus::PenaltyShootout => {
            MatchStatus::InPlay
        }
        RawStatus::Paused => MatchStatus::Paused,
        RawStatus::Finished => MatchStatus::Finished,
        RawStatus::Suspended => MatchStatus::Suspended,
        RawStatus::Postponed => MatchStatus::Postponed,
        RawStatus::Cancelled => MatchStatus::Cancelled,
        RawStatus::Awarded => MatchStatus::Awarded,
        RawStatus::Unknown => MatchStatus::Unknown,
    }
}

fn goals_from(raw: &RawGoals) -> Option<Goals> {
    Some(Goals {
        home: raw.home?,
        away: raw.away?,
    })
}

fn club(team: &RawTeam) -> Result<Club, SourceError> {
    Ok(club_for_name(&team.name)?)
}

fn table_from(response: StandingsResponse) -> Result<Vec<TableRow>, SourceError> {
    let total = response
        .standings
        .into_iter()
        .find(|standing| standing.kind == "TOTAL")
        .ok_or_else(|| SourceError::Shape {
            source_name: SOURCE_NAME,
            detail: "the standings have no TOTAL table".into(),
        })?;
    total.table.into_iter().map(table_row_from).collect()
}

fn table_row_from(raw: RawTableRow) -> Result<TableRow, SourceError> {
    Ok(TableRow {
        position: raw.position,
        team: club(&raw.team)?,
        played: raw.played_games,
        won: raw.won,
        drawn: raw.draw,
        lost: raw.lost,
        goals_for: raw.goals_for,
        goals_against: raw.goals_against,
        goal_difference: raw.goal_difference,
        points: raw.points,
    })
}

fn scorers_from(response: ScorersResponse) -> Result<Vec<Scorer>, SourceError> {
    response
        .scorers
        .into_iter()
        .map(|raw| {
            Ok(Scorer {
                player: raw.player.name,
                team: club(&raw.team)?,
                goals: raw.goals,
                assists: raw.assists,
                penalties: raw.penalties,
                played_matches: raw.played_matches,
            })
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;
    use crate::upstream::parse_json;

    const MATCHES: &str = include_str!("../tests/fixtures/football-data/matches.json");
    const STANDINGS: &str = include_str!("../tests/fixtures/football-data/standings.json");
    pub const SCORERS: &str = include_str!("../tests/fixtures/football-data/scorers.json");

    pub fn fixture_matches() -> Vec<Match> {
        matches_from(parse_json(MATCHES.as_bytes(), SOURCE_NAME).unwrap()).unwrap()
    }

    pub fn fixture_table() -> Vec<TableRow> {
        table_from(parse_json(STANDINGS.as_bytes(), SOURCE_NAME).unwrap()).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{SCORERS, fixture_matches, fixture_table};
    use super::*;
    use crate::upstream::parse_json;

    #[test]
    fn matches_parse_into_domain_matches() {
        let matches = fixture_matches();

        assert_eq!(matches.len(), 60);
        let first = &matches[0];
        assert_eq!(first.id, 560001);
        assert_eq!(first.kickoff.to_rfc3339(), "2026-08-21T19:00:00+00:00");
        assert_eq!(first.round, Some(1));
        assert_eq!(first.status, MatchStatus::Finished);
        assert_eq!(first.home.id, "arsenal");
        assert_eq!(first.away.id, "coventry");
        assert_eq!(first.score.full_time, Some(Goals { home: 3, away: 0 }));
        assert_eq!(first.score.half_time, Some(Goals { home: 2, away: 0 }));
    }

    #[test]
    fn future_matches_have_no_score() {
        let matches = fixture_matches();
        let future: Vec<_> = matches
            .iter()
            .filter(|fixture| fixture.status == MatchStatus::Scheduled)
            .collect();

        assert_eq!(future.len(), 10);
        assert!(
            future
                .iter()
                .all(|fixture| fixture.score == Score::default())
        );
    }

    #[test]
    fn missing_half_time_score_gives_none() {
        let matches = fixture_matches();

        assert!(matches.iter().any(|fixture| {
            fixture.status == MatchStatus::Finished
                && fixture.score.full_time.is_some()
                && fixture.score.half_time.is_none()
        }));
    }

    #[test]
    fn every_status_maps_to_a_domain_status() {
        let statuses = [
            ("SCHEDULED", MatchStatus::Scheduled),
            ("TIMED", MatchStatus::Scheduled),
            ("IN_PLAY", MatchStatus::InPlay),
            ("PAUSED", MatchStatus::Paused),
            ("EXTRA_TIME", MatchStatus::InPlay),
            ("PENALTY_SHOOTOUT", MatchStatus::InPlay),
            ("FINISHED", MatchStatus::Finished),
            ("SUSPENDED", MatchStatus::Suspended),
            ("POSTPONED", MatchStatus::Postponed),
            ("CANCELLED", MatchStatus::Cancelled),
            ("AWARDED", MatchStatus::Awarded),
            ("SOMETHING_NEW", MatchStatus::Unknown),
        ];
        for (raw, expected) in statuses {
            let json = format!(
                r#"{{"matches":[{{"id":1,"utcDate":"2026-10-10T11:30:00Z","status":"{raw}",
                "matchday":6,"homeTeam":{{"name":"Arsenal FC"}},"awayTeam":{{"name":"Leeds United FC"}},
                "score":{{"fullTime":{{"home":1,"away":0}},"halfTime":{{"home":null,"away":null}}}}}}]}}"#
            );
            let matches = matches_from(parse_json(json.as_bytes(), SOURCE_NAME).unwrap()).unwrap();

            assert_eq!(matches[0].status, expected, "{raw}");
        }
    }

    #[test]
    fn unknown_team_fails_with_its_name() {
        let json = r#"{"matches":[{"id":1,"utcDate":"2026-10-10T11:30:00Z","status":"TIMED",
            "matchday":6,"homeTeam":{"name":"West Ham United FC"},"awayTeam":{"name":"Arsenal FC"},
            "score":{"fullTime":{"home":null,"away":null},"halfTime":{"home":null,"away":null}}}]}"#;

        let error = matches_from(parse_json(json.as_bytes(), SOURCE_NAME).unwrap()).unwrap_err();

        assert!(error.to_string().contains("West Ham United FC"));
    }

    #[test]
    fn standings_use_the_total_table() {
        let table = fixture_table();

        assert_eq!(table.len(), 20);
        assert_eq!(table[0].team.id, "man-city");
        assert_eq!(table[0].points, 15);
        assert_eq!(table[2].team.id, "brighton");
        assert_eq!(table[2].goal_difference, 11);
        assert_eq!(table[19].team.id, "tottenham");
    }

    #[test]
    fn standings_without_a_total_table_are_an_error() {
        let json = r#"{"standings":[{"stage":"REGULAR_SEASON","type":"HOME","table":[]}]}"#;

        let error = table_from(parse_json(json.as_bytes(), SOURCE_NAME).unwrap()).unwrap_err();

        assert!(error.to_string().contains("TOTAL"));
    }

    #[test]
    fn scorers_parse_with_optional_numbers() {
        let scorers = scorers_from(parse_json(SCORERS.as_bytes(), SOURCE_NAME).unwrap()).unwrap();

        assert_eq!(scorers.len(), 3);
        assert_eq!(scorers[0].player, "Example Striker");
        assert_eq!(scorers[0].team.id, "man-city");
        assert_eq!(scorers[0].goals, 5);
        assert_eq!(scorers[0].assists, None);
        assert_eq!(scorers[0].played_matches, Some(5));
        assert_eq!(scorers[1].penalties, None);
    }
}
