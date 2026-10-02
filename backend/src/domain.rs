use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::clubs::Club;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Match {
    pub id: u64,
    pub kickoff: DateTime<Utc>,
    pub round: Option<u32>,
    pub status: MatchStatus,
    pub home: Club,
    pub away: Club,
    pub score: Score,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchStatus {
    Scheduled,
    InPlay,
    Paused,
    Finished,
    Postponed,
    Suspended,
    Cancelled,
    Awarded,
    Unknown,
}

impl MatchStatus {
    pub fn is_in_play(self) -> bool {
        matches!(self, Self::InPlay | Self::Paused)
    }

    pub fn counts_in_table(self) -> bool {
        matches!(self, Self::Finished | Self::Awarded)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct Score {
    pub full_time: Option<Goals>,
    pub half_time: Option<Goals>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Goals {
    pub home: u32,
    pub away: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TableRow {
    pub position: u32,
    pub team: Club,
    pub played: u32,
    pub won: u32,
    pub drawn: u32,
    pub lost: u32,
    pub goals_for: u32,
    pub goals_against: u32,
    pub goal_difference: i32,
    pub points: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Scorer {
    pub player: String,
    pub team: Club,
    pub goals: u32,
    pub assists: Option<u32>,
    pub penalties: Option<u32>,
    pub played_matches: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatchLineups {
    pub home: TeamLineup,
    pub away: TeamLineup,
}

impl MatchLineups {
    pub fn is_complete(&self) -> bool {
        !self.home.starting_rows.is_empty() && !self.away.starting_rows.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TeamLineup {
    pub team: Club,
    pub formation: Option<String>,
    pub starting_rows: Vec<Vec<LineupPlayer>>,
    pub substitutes: Vec<LineupPlayer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LineupPlayer {
    pub name: String,
    pub number: Option<u32>,
    pub position: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatchEvent {
    pub team: Club,
    pub minute: u32,
    pub added_time: Option<u32>,
    pub kind: EventKind,
    pub player: Option<String>,
    pub assist: Option<String>,
    pub substituted: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Goal,
    OwnGoal,
    PenaltyGoal,
    MissedPenalty,
    YellowCard,
    RedCard,
    Substitution,
    VarGoalConfirmed,
    VarGoalCancelled,
    VarGoalCancelledOffside,
    VarPenalty,
    VarPenaltyCancelled,
    Other,
}
