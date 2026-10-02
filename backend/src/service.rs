use std::{collections::HashMap, sync::Arc, time::Duration, time::Instant};

use chrono::{NaiveDate, Utc};
use reqwest::Client;
use serde::Serialize;
use tokio::sync::Mutex;

use crate::{
    budget::{BudgetExhausted, DailyBudget},
    cache::{
        DATE_LOOKUP_TTL, Expiring, ExpiringMap, Lifetime, events_lifetime, lineups_lifetime,
        match_data_ttl,
    },
    clubs::UnknownTeam,
    config::{Config, FOOTBALL_DATA_API_KEY, HIGHLIGHTLY_API_KEY},
    domain::{Match, MatchEvent, MatchLineups, MatchStatus, Scorer, TableRow},
    football_data::FootballDataClient,
    highlightly::HighlightlyClient,
    match_link::link_matches,
    table::{TableCheck, compare_tables, compute_table},
    upstream::SourceError,
};

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("set {} in the server environment to use this route", .0.join(" and "))]
    MissingKeys(Vec<&'static str>),
    #[error(transparent)]
    Source(#[from] SourceError),
    #[error("no Premier League match has the id {0}")]
    MatchNotFound(u64),
    #[error("match {0} is not finished; lineups and events are available after full time")]
    MatchNotFinished(u64),
    #[error(
        "Highlightly has no Premier League match that links to match {0}; the free plan can hide some matches"
    )]
    NoLinkedMatch(u64),
    #[error(transparent)]
    BudgetExhausted(#[from] BudgetExhausted),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TableReport {
    pub table: Vec<TableRow>,
    pub check: TableCheck,
}

#[derive(Clone)]
pub struct DataService {
    inner: Arc<Inner>,
}

struct Inner {
    football_data: Option<FootballDataClient>,
    highlightly: Option<HighlightlyClient>,
    matches: Mutex<Expiring<Vec<Match>>>,
    table: Mutex<Expiring<TableReport>>,
    scorers: Mutex<Expiring<Vec<Scorer>>>,
    post_match: Mutex<PostMatchData>,
}

struct PostMatchData {
    budget: DailyBudget,
    highlightly_ids: HashMap<u64, u64>,
    date_lookups: ExpiringMap<NaiveDate, Vec<UnknownTeam>>,
    lineups: ExpiringMap<u64, MatchLineups>,
    events: ExpiringMap<u64, Vec<MatchEvent>>,
}

impl DataService {
    pub fn new(config: Config, http: Client) -> Self {
        let football_data = config
            .football_data
            .api_key
            .map(|key| FootballDataClient::new(http.clone(), config.football_data.base_url, key));
        let highlightly = config
            .highlightly
            .api_key
            .map(|key| HighlightlyClient::new(http, config.highlightly.base_url, key));
        Self {
            inner: Arc::new(Inner {
                football_data,
                highlightly,
                matches: Mutex::default(),
                table: Mutex::default(),
                scorers: Mutex::default(),
                post_match: Mutex::new(PostMatchData {
                    budget: DailyBudget::new(config.highlightly_daily_budget),
                    highlightly_ids: HashMap::new(),
                    date_lookups: ExpiringMap::default(),
                    lineups: ExpiringMap::default(),
                    events: ExpiringMap::default(),
                }),
            }),
        }
    }

    pub async fn matches(&self) -> Result<Vec<Match>, ServiceError> {
        let client = self.football_data()?;
        cached(&self.inner.matches, client.matches(), |matches| {
            match_data_ttl(matches, Utc::now())
        })
        .await
    }

    pub async fn table(&self) -> Result<TableReport, ServiceError> {
        let client = self.football_data()?;
        let matches = self.matches().await?;
        let ttl = match_data_ttl(&matches, Utc::now());
        let fetch = async {
            let table = client.standings().await?;
            let check = compare_tables(&table, &compute_table(&matches));
            log_table_differences(&check);
            Ok(TableReport { table, check })
        };
        cached(&self.inner.table, fetch, |_| ttl).await
    }

    pub async fn scorers(&self) -> Result<Vec<Scorer>, ServiceError> {
        let client = self.football_data()?;
        let matches = self.matches().await?;
        let ttl = match_data_ttl(&matches, Utc::now());
        cached(&self.inner.scorers, client.scorers(), |_| ttl).await
    }

    pub async fn lineups(&self, match_id: u64) -> Result<MatchLineups, ServiceError> {
        let client = self.post_match_client()?;
        let mut data = self.inner.post_match.lock().await;
        if let Some(lineups) = data.lineups.get(&match_id, Instant::now()) {
            return Ok(lineups);
        }
        let highlightly_id = self
            .linked_highlightly_id(&mut data, client, match_id)
            .await?;
        data.budget.try_spend(today())?;
        let lineups = client.lineups(highlightly_id).await?;
        let lifetime = lineups_lifetime(&lineups);
        data.lineups
            .insert(match_id, lineups.clone(), lifetime, Instant::now());
        Ok(lineups)
    }

    pub async fn events(&self, match_id: u64) -> Result<Vec<MatchEvent>, ServiceError> {
        let client = self.post_match_client()?;
        let mut data = self.inner.post_match.lock().await;
        if let Some(events) = data.events.get(&match_id, Instant::now()) {
            return Ok(events);
        }
        let highlightly_id = self
            .linked_highlightly_id(&mut data, client, match_id)
            .await?;
        data.budget.try_spend(today())?;
        let events = client.events(highlightly_id).await?;
        let lifetime = events_lifetime(&events);
        data.events
            .insert(match_id, events.clone(), lifetime, Instant::now());
        Ok(events)
    }

    async fn linked_highlightly_id(
        &self,
        data: &mut PostMatchData,
        client: &HighlightlyClient,
        match_id: u64,
    ) -> Result<u64, ServiceError> {
        let matches = self.matches().await?;
        let fixture = matches
            .iter()
            .find(|fixture| fixture.id == match_id)
            .ok_or(ServiceError::MatchNotFound(match_id))?;
        if fixture.status != MatchStatus::Finished {
            return Err(ServiceError::MatchNotFinished(match_id));
        }
        if let Some(id) = data.highlightly_ids.get(&match_id) {
            return Ok(*id);
        }
        let date = fixture.kickoff.date_naive();
        let unknown_teams = match data.date_lookups.get(&date, Instant::now()) {
            Some(unknown_teams) => unknown_teams,
            None => {
                data.budget.try_spend(today())?;
                let others = client.matches_on(date).await?;
                let links = link_matches(&matches, &others);
                data.highlightly_ids.extend(links.pairs);
                data.date_lookups.insert(
                    date,
                    links.unknown_teams.clone(),
                    Lifetime::For(DATE_LOOKUP_TTL),
                    Instant::now(),
                );
                links.unknown_teams
            }
        };
        if let Some(id) = data.highlightly_ids.get(&match_id) {
            return Ok(*id);
        }
        Err(match unknown_teams.into_iter().next() {
            Some(unknown) => SourceError::from(unknown).into(),
            None => ServiceError::NoLinkedMatch(match_id),
        })
    }

    fn football_data(&self) -> Result<&FootballDataClient, ServiceError> {
        self.inner
            .football_data
            .as_ref()
            .ok_or_else(|| ServiceError::MissingKeys(vec![FOOTBALL_DATA_API_KEY]))
    }

    fn post_match_client(&self) -> Result<&HighlightlyClient, ServiceError> {
        match (&self.inner.football_data, &self.inner.highlightly) {
            (Some(_), Some(highlightly)) => Ok(highlightly),
            (football_data, highlightly) => Err(ServiceError::MissingKeys(
                [
                    football_data.is_none().then_some(FOOTBALL_DATA_API_KEY),
                    highlightly.is_none().then_some(HIGHLIGHTLY_API_KEY),
                ]
                .into_iter()
                .flatten()
                .collect(),
            )),
        }
    }
}

async fn cached<T: Clone>(
    slot: &Mutex<Expiring<T>>,
    fetch: impl Future<Output = Result<T, SourceError>>,
    ttl: impl FnOnce(&T) -> Duration,
) -> Result<T, ServiceError> {
    let mut slot = slot.lock().await;
    if let Some(value) = slot.get(Instant::now()) {
        return Ok(value);
    }
    let value = fetch.await?;
    let ttl = ttl(&value);
    slot.set(value.clone(), Lifetime::For(ttl), Instant::now());
    Ok(value)
}

fn log_table_differences(check: &TableCheck) {
    for difference in &check.differences {
        tracing::warn!(
            team = difference.team.id,
            official = ?difference.official,
            computed = ?difference.computed,
            "the computed table differs from the football-data.org table"
        );
    }
}

fn today() -> NaiveDate {
    Utc::now().date_naive()
}
