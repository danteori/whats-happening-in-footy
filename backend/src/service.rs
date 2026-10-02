use std::{
    collections::HashMap,
    sync::{Arc, Mutex as StdMutex, MutexGuard},
    time::{Duration, Instant},
};

use chrono::{NaiveDate, Utc};
use reqwest::Client;
use serde::Serialize;
use tokio::sync::Mutex;

use crate::{
    budget::{BudgetExhausted, DailyBudget},
    cache::{
        DATE_LOOKUP_TTL, ERROR_TTL, Expiring, ExpiringMap, Lifetime, events_lifetime,
        lineups_lifetime, match_data_ttl,
    },
    clubs::UnknownTeam,
    config::{Config, FOOTBALL_DATA_API_KEY, HIGHLIGHTLY_API_KEY},
    domain::{Match, MatchEvent, MatchLineups, MatchStatus, Scorer, TableRow},
    football_data::FootballDataClient,
    highlightly::{HighlightlyClient, Quoted},
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
    MatchNotFound(String),
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
    matches: Mutex<Expiring<Result<Vec<Match>, SourceError>>>,
    table: Mutex<Expiring<Result<TableReport, SourceError>>>,
    scorers: Mutex<Expiring<Result<Vec<Scorer>, SourceError>>>,
    post_match: StdMutex<PostMatchCache>,
    highlightly_calls: Mutex<DailyBudget>,
}

struct FinishedMatch {
    fixture: Match,
    season: Vec<Match>,
}

#[derive(Default)]
struct PostMatchCache {
    links: HashMap<u64, u64>,
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
                post_match: StdMutex::default(),
                highlightly_calls: Mutex::new(DailyBudget::new(config.highlightly_daily_budget)),
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
        self.post_match_data(
            client,
            match_id,
            |cache| &mut cache.lineups,
            |highlightly_id| client.lineups(highlightly_id),
            |lineups, _| lineups_lifetime(lineups),
        )
        .await
    }

    pub async fn events(&self, match_id: u64) -> Result<Vec<MatchEvent>, ServiceError> {
        let client = self.post_match_client()?;
        self.post_match_data(
            client,
            match_id,
            |cache| &mut cache.events,
            |highlightly_id| client.events(highlightly_id),
            |events, fixture| events_lifetime(events, fixture),
        )
        .await
    }

    async fn post_match_data<T: Clone, F: Future<Output = Quoted<T>>>(
        &self,
        client: &HighlightlyClient,
        match_id: u64,
        slot: fn(&mut PostMatchCache) -> &mut ExpiringMap<u64, T>,
        fetch: impl FnOnce(u64) -> F,
        lifetime: impl FnOnce(&T, &Match) -> Lifetime,
    ) -> Result<T, ServiceError> {
        let cached =
            |service: &Self| slot(&mut service.post_match_cache()).get(&match_id, Instant::now());
        if let Some(value) = cached(self) {
            return Ok(value);
        }
        let finished = self.finished_match(match_id).await?;
        let mut budget = self.inner.highlightly_calls.lock().await;
        if let Some(value) = cached(self) {
            return Ok(value);
        }
        let highlightly_id = self
            .linked_highlightly_id(&mut budget, client, &finished)
            .await?;
        let value = spend(&mut budget, fetch(highlightly_id)).await?;
        let lifetime = lifetime(&value, &finished.fixture);
        slot(&mut self.post_match_cache()).insert(
            match_id,
            value.clone(),
            lifetime,
            Instant::now(),
        );
        Ok(value)
    }

    async fn finished_match(&self, match_id: u64) -> Result<FinishedMatch, ServiceError> {
        let season = self.matches().await?;
        let fixture = season
            .iter()
            .find(|fixture| fixture.id == match_id)
            .cloned()
            .ok_or_else(|| ServiceError::MatchNotFound(match_id.to_string()))?;
        if fixture.status != MatchStatus::Finished {
            return Err(ServiceError::MatchNotFinished(match_id));
        }
        Ok(FinishedMatch { fixture, season })
    }

    async fn linked_highlightly_id(
        &self,
        budget: &mut DailyBudget,
        client: &HighlightlyClient,
        finished: &FinishedMatch,
    ) -> Result<u64, ServiceError> {
        let match_id = finished.fixture.id;
        let known_link = self.post_match_cache().links.get(&match_id).copied();
        if let Some(highlightly_id) = known_link {
            return Ok(highlightly_id);
        }
        let date = finished.fixture.kickoff.date_naive();
        let remembered_lookup = self
            .post_match_cache()
            .date_lookups
            .get(&date, Instant::now());
        let unknown_teams = match remembered_lookup {
            Some(unknown_teams) => unknown_teams,
            None => {
                let others = spend(budget, client.matches_on(date)).await?;
                let links = link_matches(&finished.season, &others);
                let mut cache = self.post_match_cache();
                cache.links.extend(links.pairs);
                cache.date_lookups.insert(
                    date,
                    links.unknown_teams.clone(),
                    Lifetime::For(DATE_LOOKUP_TTL),
                    Instant::now(),
                );
                links.unknown_teams
            }
        };
        let new_link = self.post_match_cache().links.get(&match_id).copied();
        new_link.ok_or_else(|| match unknown_teams.into_iter().next() {
            Some(unknown) => SourceError::from(unknown).into(),
            None => ServiceError::NoLinkedMatch(match_id),
        })
    }

    fn post_match_cache(&self) -> MutexGuard<'_, PostMatchCache> {
        self.inner
            .post_match
            .lock()
            .expect("no thread panics while it holds the post-match cache")
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
    slot: &Mutex<Expiring<Result<T, SourceError>>>,
    fetch: impl Future<Output = Result<T, SourceError>>,
    ttl: impl FnOnce(&T) -> Duration,
) -> Result<T, ServiceError> {
    let mut slot = slot.lock().await;
    if let Some(result) = slot.get(Instant::now()) {
        return Ok(result?);
    }
    let result = fetch.await;
    let ttl = match &result {
        Ok(value) => ttl(value),
        Err(_) => ERROR_TTL,
    };
    slot.set(result.clone(), Lifetime::For(ttl), Instant::now());
    Ok(result?)
}

async fn spend<T>(
    budget: &mut DailyBudget,
    call: impl Future<Output = Quoted<T>>,
) -> Result<T, ServiceError> {
    budget.try_spend(today())?;
    let answer = call.await;
    budget.observe(answer.quota, today());
    Ok(answer.result?)
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
