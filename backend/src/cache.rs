use std::time::{Duration, Instant};

use chrono::{DateTime, TimeDelta, Utc};

use crate::domain::{Match, MatchStatus};

pub const LIVE_TTL: Duration = Duration::from_secs(60);
pub const IDLE_TTL: Duration = Duration::from_secs(10 * 60);
const MATCH_WINDOW: TimeDelta = TimeDelta::minutes(150);

#[derive(Debug)]
pub struct Expiring<T> {
    entry: Option<(T, Instant)>,
}

impl<T> Default for Expiring<T> {
    fn default() -> Self {
        Self { entry: None }
    }
}

impl<T: Clone> Expiring<T> {
    pub fn get(&self, now: Instant) -> Option<T> {
        self.entry
            .as_ref()
            .filter(|(_, expires_at)| now < *expires_at)
            .map(|(value, _)| value.clone())
    }

    pub fn set(&mut self, value: T, ttl: Duration, now: Instant) {
        self.entry = Some((value, now + ttl));
    }
}

pub fn match_data_ttl(matches: &[Match], now: DateTime<Utc>) -> Duration {
    if matches.iter().any(|fixture| is_live(fixture, now)) {
        return LIVE_TTL;
    }
    matches
        .iter()
        .filter(|fixture| fixture.status == MatchStatus::Scheduled && fixture.kickoff > now)
        .filter_map(|fixture| (fixture.kickoff - now).to_std().ok())
        .min()
        .map_or(IDLE_TTL, |until_kickoff| {
            until_kickoff.clamp(LIVE_TTL, IDLE_TTL)
        })
}

fn is_live(fixture: &Match, now: DateTime<Utc>) -> bool {
    let in_kickoff_window = fixture.status == MatchStatus::Scheduled
        && fixture.kickoff <= now
        && now < fixture.kickoff + MATCH_WINDOW;
    fixture.status.is_in_play() || in_kickoff_window
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::{clubs::club_for_name, domain::Score};

    fn fixture(status: MatchStatus, kickoff: DateTime<Utc>) -> Match {
        Match {
            id: 1,
            kickoff,
            round: Some(6),
            status,
            home: club_for_name("Arsenal").unwrap(),
            away: club_for_name("Leeds").unwrap(),
            score: Score::default(),
        }
    }

    fn saturday(hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 10, hour, minute, 0).unwrap()
    }

    #[test]
    fn value_expires_after_its_ttl() {
        let start = Instant::now();
        let mut cache = Expiring::default();
        cache.set(7, Duration::from_secs(60), start);

        assert_eq!(cache.get(start), Some(7));
        assert_eq!(cache.get(start + Duration::from_secs(59)), Some(7));
        assert_eq!(cache.get(start + Duration::from_secs(60)), None);
    }

    #[test]
    fn empty_cache_has_no_value() {
        let cache: Expiring<u32> = Expiring::default();

        assert_eq!(cache.get(Instant::now()), None);
    }

    #[test]
    fn in_play_match_gives_the_live_ttl() {
        for status in [MatchStatus::InPlay, MatchStatus::Paused] {
            let matches = [fixture(status, saturday(11, 30))];

            assert_eq!(match_data_ttl(&matches, saturday(12, 0)), LIVE_TTL);
        }
    }

    #[test]
    fn scheduled_match_past_its_kickoff_gives_the_live_ttl() {
        let matches = [fixture(MatchStatus::Scheduled, saturday(11, 30))];

        assert_eq!(match_data_ttl(&matches, saturday(11, 45)), LIVE_TTL);
        assert_eq!(match_data_ttl(&matches, saturday(14, 30)), IDLE_TTL);
    }

    #[test]
    fn quiet_time_gives_the_idle_ttl() {
        let matches = [
            fixture(MatchStatus::Finished, saturday(11, 30)),
            fixture(MatchStatus::Scheduled, saturday(16, 30)),
        ];

        assert_eq!(match_data_ttl(&matches, saturday(14, 0)), IDLE_TTL);
        assert_eq!(match_data_ttl(&[], saturday(14, 0)), IDLE_TTL);
    }

    #[test]
    fn ttl_ends_at_the_next_kickoff() {
        let matches = [fixture(MatchStatus::Scheduled, saturday(15, 0))];

        assert_eq!(
            match_data_ttl(&matches, saturday(14, 56)),
            Duration::from_secs(4 * 60)
        );
        assert_eq!(match_data_ttl(&matches, saturday(14, 59)), LIVE_TTL);
    }
}
