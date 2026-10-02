use std::{cmp::Reverse, collections::HashMap};

use serde::Serialize;

use crate::{
    clubs::Club,
    domain::{Goals, Match, TableRow},
};

const POINTS_FOR_WIN: u32 = 3;
const POINTS_FOR_DRAW: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TableCheck {
    pub matches_official: bool,
    pub differences: Vec<TableDifference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TableDifference {
    pub team: Club,
    pub official: Option<TableRow>,
    pub computed: Option<TableRow>,
}

#[derive(Default)]
struct Record {
    won: u32,
    drawn: u32,
    lost: u32,
    goals_for: u32,
    goals_against: u32,
}

impl Record {
    fn add(&mut self, scored: u32, conceded: u32) {
        self.goals_for += scored;
        self.goals_against += conceded;
        match scored.cmp(&conceded) {
            std::cmp::Ordering::Greater => self.won += 1,
            std::cmp::Ordering::Equal => self.drawn += 1,
            std::cmp::Ordering::Less => self.lost += 1,
        }
    }

    fn into_row(self, team: Club) -> TableRow {
        TableRow {
            position: 0,
            team,
            played: self.won + self.drawn + self.lost,
            won: self.won,
            drawn: self.drawn,
            lost: self.lost,
            goals_for: self.goals_for,
            goals_against: self.goals_against,
            goal_difference: self.goals_for as i32 - self.goals_against as i32,
            points: self.won * POINTS_FOR_WIN + self.drawn * POINTS_FOR_DRAW,
        }
    }
}

pub fn compute_table(matches: &[Match]) -> Vec<TableRow> {
    let mut records: HashMap<Club, Record> = HashMap::new();
    for fixture in matches {
        records.entry(fixture.home).or_default();
        records.entry(fixture.away).or_default();
        if let Some(Goals { home, away }) = counted_score(fixture) {
            records.entry(fixture.home).or_default().add(home, away);
            records.entry(fixture.away).or_default().add(away, home);
        }
    }
    let mut rows: Vec<TableRow> = records
        .into_iter()
        .map(|(team, record)| record.into_row(team))
        .collect();
    rows.sort_by_key(|row| (Reverse(ranking_key(row)), row.team.name));
    assign_shared_positions(&mut rows);
    rows
}

fn counted_score(fixture: &Match) -> Option<Goals> {
    fixture
        .status
        .counts_in_table()
        .then_some(fixture.score.full_time)
        .flatten()
}

fn ranking_key(row: &TableRow) -> (u32, i32, u32) {
    (row.points, row.goal_difference, row.goals_for)
}

fn assign_shared_positions(rows: &mut [TableRow]) {
    let mut position = 0;
    let mut previous_key = None;
    for (index, row) in rows.iter_mut().enumerate() {
        let key = ranking_key(row);
        if previous_key != Some(key) {
            position = index as u32 + 1;
            previous_key = Some(key);
        }
        row.position = position;
    }
}

pub fn compare_tables(official: &[TableRow], computed: &[TableRow]) -> TableCheck {
    let mut differences: Vec<TableDifference> = official
        .iter()
        .filter_map(|official_row| {
            let computed_row = computed.iter().find(|row| row.team == official_row.team);
            match computed_row {
                Some(row) if rows_agree(official_row, row, computed) => None,
                _ => Some(TableDifference {
                    team: official_row.team,
                    official: Some(official_row.clone()),
                    computed: computed_row.cloned(),
                }),
            }
        })
        .collect();
    differences.extend(
        computed
            .iter()
            .filter(|row| {
                !official
                    .iter()
                    .any(|official_row| official_row.team == row.team)
            })
            .map(|row| TableDifference {
                team: row.team,
                official: None,
                computed: Some(row.clone()),
            }),
    );
    TableCheck {
        matches_official: differences.is_empty(),
        differences,
    }
}

fn rows_agree(official: &TableRow, computed: &TableRow, computed_table: &[TableRow]) -> bool {
    let same_numbers = (
        official.played,
        official.won,
        official.drawn,
        official.lost,
        official.goals_for,
        official.goals_against,
        official.points,
    ) == (
        computed.played,
        computed.won,
        computed.drawn,
        computed.lost,
        computed.goals_for,
        computed.goals_against,
        computed.points,
    );
    let clubs_sharing_position = computed_table
        .iter()
        .filter(|row| row.position == computed.position)
        .count() as u32;
    let last_shared_position = computed.position + clubs_sharing_position - 1;
    same_numbers && (computed.position..=last_shared_position).contains(&official.position)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::{
        clubs::club_for_name,
        domain::{MatchStatus, Score},
        football_data::fixtures::{fixture_matches, fixture_table},
    };

    fn result(home: &str, home_goals: u32, away: &str, away_goals: u32) -> Match {
        Match {
            id: 0,
            kickoff: Utc.with_ymd_and_hms(2026, 9, 1, 14, 0, 0).unwrap(),
            round: Some(1),
            status: MatchStatus::Finished,
            home: club_for_name(home).unwrap(),
            away: club_for_name(away).unwrap(),
            score: Score {
                full_time: Some(Goals {
                    home: home_goals,
                    away: away_goals,
                }),
                half_time: None,
            },
        }
    }

    fn summary(table: &[TableRow]) -> Vec<(u32, &'static str, u32)> {
        table
            .iter()
            .map(|row| (row.position, row.team.id, row.points))
            .collect()
    }

    #[test]
    fn wins_draws_and_losses_score_points() {
        let table = compute_table(&[
            result("Arsenal", 2, "Chelsea", 0),
            result("Chelsea", 1, "Fulham", 1),
            result("Fulham", 0, "Arsenal", 3),
        ]);

        assert_eq!(
            summary(&table),
            [(1, "arsenal", 6), (2, "chelsea", 1), (3, "fulham", 1)]
        );
        let arsenal = &table[0];
        assert_eq!(
            (arsenal.played, arsenal.won, arsenal.drawn, arsenal.lost),
            (2, 2, 0, 0)
        );
        assert_eq!(
            (
                arsenal.goals_for,
                arsenal.goals_against,
                arsenal.goal_difference
            ),
            (5, 0, 5)
        );
    }

    #[test]
    fn goal_difference_then_goals_scored_break_ties() {
        let table = compute_table(&[
            result("Arsenal", 1, "Chelsea", 0),
            result("Everton", 3, "Fulham", 2),
            result("Brentford", 2, "Leeds", 0),
        ]);

        assert_eq!(
            summary(&table)[..3],
            [(1, "brentford", 3), (2, "everton", 3), (3, "arsenal", 3)]
        );
    }

    #[test]
    fn a_full_tie_shares_the_position() {
        let table = compute_table(&[
            result("Arsenal", 2, "Chelsea", 1),
            result("Everton", 2, "Fulham", 1),
            result("Liverpool", 0, "Brentford", 0),
        ]);

        assert_eq!(
            summary(&table),
            [
                (1, "arsenal", 3),
                (1, "everton", 3),
                (3, "brentford", 1),
                (3, "liverpool", 1),
                (5, "chelsea", 0),
                (5, "fulham", 0),
            ]
        );
    }

    #[test]
    fn head_to_head_does_not_break_a_full_tie() {
        let table = compute_table(&[
            result("Arsenal", 1, "Chelsea", 0),
            result("Chelsea", 1, "Arsenal", 0),
        ]);

        assert_eq!(summary(&table), [(1, "arsenal", 3), (1, "chelsea", 3)]);
    }

    #[test]
    fn matches_without_a_final_score_do_not_count() {
        let mut postponed = result("Arsenal", 0, "Chelsea", 0);
        postponed.status = MatchStatus::Postponed;
        let mut live = result("Fulham", 1, "Everton", 0);
        live.status = MatchStatus::InPlay;
        let mut awarded = result("Leeds", 3, "Hull City", 0);
        awarded.status = MatchStatus::Awarded;

        let table = compute_table(&[postponed, live, awarded]);

        let played: u32 = table.iter().map(|row| row.played).sum();
        assert_eq!(played, 2);
        assert_eq!(table.len(), 6);
        assert_eq!(table[0].team.id, "leeds");
    }

    #[test]
    fn openfootball_2026_27_results_match_the_official_table() {
        let computed = compute_table(&fixture_matches());
        let official = fixture_table();

        let check = compare_tables(&official, &computed);

        assert_eq!(check.differences, []);
        assert!(check.matches_official);
        assert_eq!(summary(&computed), summary(&official));
    }

    #[test]
    fn a_different_row_is_reported() {
        let computed = compute_table(&fixture_matches());
        let mut official = fixture_table();
        official[4].points -= 1;

        let check = compare_tables(&official, &computed);

        assert!(!check.matches_official);
        assert_eq!(check.differences.len(), 1);
        assert_eq!(check.differences[0].team.id, "leeds");
    }

    #[test]
    fn a_missing_club_is_reported() {
        let computed = compute_table(&fixture_matches());
        let official = fixture_table()[..19].to_vec();

        let check = compare_tables(&official, &computed);

        assert_eq!(check.differences.len(), 1);
        assert_eq!(check.differences[0].team.id, "tottenham");
        assert_eq!(check.differences[0].official, None);
    }

    #[test]
    fn official_order_inside_a_full_tie_agrees() {
        let computed = compute_table(&[
            result("Arsenal", 2, "Chelsea", 1),
            result("Everton", 2, "Fulham", 1),
        ]);
        let mut official = computed.clone();
        official.swap(0, 1);
        official[1].position = 2;

        assert!(compare_tables(&official, &computed).matches_official);
    }
}
