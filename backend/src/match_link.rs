use crate::{
    clubs::{UnknownTeam, club_for_name},
    domain::Match,
    highlightly::HighlightlyMatch,
};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct MatchLinks {
    pub pairs: Vec<(u64, u64)>,
    pub unknown_teams: Vec<UnknownTeam>,
}

pub fn link_matches(football_data: &[Match], highlightly: &[HighlightlyMatch]) -> MatchLinks {
    let mut links = MatchLinks::default();
    for other in highlightly {
        let teams = club_for_name(&other.home_team)
            .and_then(|home| club_for_name(&other.away_team).map(|away| (home, away)));
        match teams {
            Ok((home, away)) => links.pairs.extend(
                football_data
                    .iter()
                    .find(|fixture| {
                        fixture.home == home
                            && fixture.away == away
                            && fixture.kickoff.date_naive() == other.kickoff.date_naive()
                    })
                    .map(|fixture| (fixture.id, other.id)),
            ),
            Err(unknown) => links.unknown_teams.push(unknown),
        }
    }
    links
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, TimeZone, Utc};

    use super::*;
    use crate::football_data::fixtures::fixture_matches;

    fn other(id: u64, home: &str, away: &str, kickoff: DateTime<Utc>) -> HighlightlyMatch {
        HighlightlyMatch {
            id,
            kickoff,
            home_team: home.into(),
            away_team: away.into(),
            state: "Finished".into(),
            score: None,
        }
    }

    fn september(day: u32, hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, day, hour, 0, 0).unwrap()
    }

    #[test]
    fn links_by_date_and_teams() {
        let links = link_matches(
            &fixture_matches(),
            &[
                other(1, "Fulham", "Manchester United", september(20, 15)),
                other(2, "Bournemouth", "Liverpool", september(20, 13)),
            ],
        );

        assert_eq!(links.pairs, [(560050, 1), (560047, 2)]);
        assert!(links.unknown_teams.is_empty());
    }

    #[test]
    fn same_teams_on_another_date_do_not_link() {
        let links = link_matches(
            &fixture_matches(),
            &[other(1, "Fulham", "Manchester United", september(21, 15))],
        );

        assert!(links.pairs.is_empty());
    }

    #[test]
    fn swapped_home_and_away_do_not_link() {
        let links = link_matches(
            &fixture_matches(),
            &[other(1, "Manchester United", "Fulham", september(20, 15))],
        );

        assert!(links.pairs.is_empty());
    }

    #[test]
    fn unknown_team_names_are_collected() {
        let links = link_matches(
            &fixture_matches(),
            &[other(1, "Fulham", "West Ham", september(20, 15))],
        );

        assert_eq!(links.unknown_teams, [UnknownTeam("West Ham".into())]);
    }
}
