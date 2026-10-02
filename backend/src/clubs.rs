use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct Club {
    pub id: &'static str,
    pub name: &'static str,
    pub short_name: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the team name {0:?} matches no Premier League 2026/27 club")]
pub struct UnknownTeam(pub String);

struct ClubNames {
    club: Club,
    aliases: &'static [&'static str],
}

const fn club(
    id: &'static str,
    name: &'static str,
    short_name: &'static str,
    aliases: &'static [&'static str],
) -> ClubNames {
    ClubNames {
        club: Club {
            id,
            name,
            short_name,
        },
        aliases,
    }
}

const PREMIER_LEAGUE_2026_27: [ClubNames; 20] = [
    club("arsenal", "Arsenal", "Arsenal", &[]),
    club("aston-villa", "Aston Villa", "Aston Villa", &[]),
    club("bournemouth", "Bournemouth", "Bournemouth", &[]),
    club("brentford", "Brentford", "Brentford", &[]),
    club(
        "brighton",
        "Brighton & Hove Albion",
        "Brighton",
        &["Brighton", "Brighton Hove", "Brighton and Hove"],
    ),
    club("chelsea", "Chelsea", "Chelsea", &[]),
    club("coventry", "Coventry City", "Coventry", &["Coventry"]),
    club("crystal-palace", "Crystal Palace", "Crystal Palace", &[]),
    club("everton", "Everton", "Everton", &[]),
    club("fulham", "Fulham", "Fulham", &[]),
    club("hull", "Hull City", "Hull", &["Hull"]),
    club("ipswich", "Ipswich Town", "Ipswich", &["Ipswich"]),
    club("leeds", "Leeds United", "Leeds", &["Leeds", "Leeds Utd"]),
    club("liverpool", "Liverpool", "Liverpool", &[]),
    club("man-city", "Manchester City", "Man City", &["Man City"]),
    club(
        "man-utd",
        "Manchester United",
        "Man Utd",
        &["Man Utd", "Man United", "Manchester Utd"],
    ),
    club("newcastle", "Newcastle United", "Newcastle", &["Newcastle"]),
    club(
        "nottm-forest",
        "Nottingham Forest",
        "Nott'm Forest",
        &["Nott'm Forest", "Nottingham", "Nottm Forest", "Forest"],
    ),
    club("sunderland", "Sunderland", "Sunderland", &[]),
    club(
        "tottenham",
        "Tottenham Hotspur",
        "Spurs",
        &["Tottenham", "Spurs"],
    ),
];

pub fn club_for_name(name: &str) -> Result<Club, UnknownTeam> {
    let wanted = normalise(name);
    PREMIER_LEAGUE_2026_27
        .iter()
        .find(|names| names.include(&wanted))
        .map(|names| names.club)
        .ok_or_else(|| UnknownTeam(name.to_owned()))
}

impl ClubNames {
    fn include(&self, normalised_name: &str) -> bool {
        std::iter::once(self.club.name)
            .chain(self.aliases.iter().copied())
            .any(|candidate| normalise(candidate) == normalised_name)
    }
}

fn normalise(name: &str) -> String {
    let letters_and_spaces: String = name
        .to_lowercase()
        .replace('&', " and ")
        .chars()
        .filter(|character| !matches!(character, '\'' | '’' | '.'))
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect();
    letters_and_spaces
        .split_whitespace()
        .filter(|word| !matches!(*word, "fc" | "afc"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id_for(name: &str) -> &'static str {
        club_for_name(name)
            .unwrap_or_else(|error| panic!("{error}"))
            .id
    }

    #[test]
    fn football_data_names_match() {
        let names = [
            ("AFC Bournemouth", "bournemouth"),
            ("Arsenal FC", "arsenal"),
            ("Aston Villa FC", "aston-villa"),
            ("Brentford FC", "brentford"),
            ("Brighton & Hove Albion FC", "brighton"),
            ("Chelsea FC", "chelsea"),
            ("Coventry City FC", "coventry"),
            ("Crystal Palace FC", "crystal-palace"),
            ("Everton FC", "everton"),
            ("Fulham FC", "fulham"),
            ("Hull City AFC", "hull"),
            ("Ipswich Town FC", "ipswich"),
            ("Leeds United FC", "leeds"),
            ("Liverpool FC", "liverpool"),
            ("Manchester City FC", "man-city"),
            ("Manchester United FC", "man-utd"),
            ("Newcastle United FC", "newcastle"),
            ("Nottingham Forest FC", "nottm-forest"),
            ("Sunderland AFC", "sunderland"),
            ("Tottenham Hotspur FC", "tottenham"),
        ];
        for (name, id) in names {
            assert_eq!(id_for(name), id, "{name}");
        }
    }

    #[test]
    fn football_data_short_names_match() {
        let names = [
            ("Brighton Hove", "brighton"),
            ("Man City", "man-city"),
            ("Man United", "man-utd"),
            ("Nottingham", "nottm-forest"),
            ("Tottenham", "tottenham"),
            ("Newcastle", "newcastle"),
        ];
        for (name, id) in names {
            assert_eq!(id_for(name), id, "{name}");
        }
    }

    #[test]
    fn premier_league_and_common_names_match() {
        let names = [
            ("Brighton and Hove Albion", "brighton"),
            ("Bournemouth", "bournemouth"),
            ("Manchester United", "man-utd"),
            ("Man Utd", "man-utd"),
            ("Nott'm Forest", "nottm-forest"),
            ("Spurs", "tottenham"),
            ("Brighton", "brighton"),
            ("Leeds", "leeds"),
            ("Coventry", "coventry"),
            ("Ipswich", "ipswich"),
            ("Hull City", "hull"),
            ("  LIVERPOOL  ", "liverpool"),
        ];
        for (name, id) in names {
            assert_eq!(id_for(name), id, "{name}");
        }
    }

    #[test]
    fn unknown_name_gives_an_error_that_names_the_team() {
        for name in ["West Ham United FC", "Manchester", "Hove", ""] {
            let error = club_for_name(name).unwrap_err();

            assert_eq!(error, UnknownTeam(name.to_owned()));
            assert!(error.to_string().contains(&format!("{name:?}")));
        }
    }

    #[test]
    fn every_club_has_a_unique_id_and_name() {
        let mut ids: Vec<_> = PREMIER_LEAGUE_2026_27
            .iter()
            .map(|names| names.club.id)
            .collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 20);

        for names in &PREMIER_LEAGUE_2026_27 {
            assert_eq!(id_for(names.club.name), names.club.id);
            for alias in names.aliases {
                assert_eq!(id_for(alias), names.club.id, "{alias}");
            }
        }
    }
}
