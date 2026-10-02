use chrono::NaiveDate;

pub const PROVIDER_RESERVE: u32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quota {
    Unknown,
    Remaining(u32),
    UsedUp,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BudgetExhausted {
    #[error(
        "the app used its daily budget of {limit} Highlightly requests; the budget resets at 00:00 UTC"
    )]
    LocalLimit { limit: u32 },
    #[error(
        "Highlightly reports {remaining} requests left today; the app keeps the last {PROVIDER_RESERVE} in reserve until 00:00 UTC"
    )]
    ProviderReserve { remaining: u32 },
    #[error("Highlightly reports that the daily request limit is used up; it resets at 00:00 UTC")]
    ProviderLimit,
}

#[derive(Debug)]
pub struct DailyBudget {
    limit: u32,
    day: Option<NaiveDate>,
    used: u32,
    stopped: Option<BudgetExhausted>,
}

impl DailyBudget {
    pub fn new(limit: u32) -> Self {
        Self {
            limit,
            day: None,
            used: 0,
            stopped: None,
        }
    }

    pub fn try_spend(&mut self, today: NaiveDate) -> Result<(), BudgetExhausted> {
        self.start_day(today);
        if let Some(reason) = &self.stopped {
            return Err(reason.clone());
        }
        if self.used >= self.limit {
            return Err(BudgetExhausted::LocalLimit { limit: self.limit });
        }
        self.used += 1;
        Ok(())
    }

    pub fn observe(&mut self, quota: Quota, today: NaiveDate) {
        self.start_day(today);
        match quota {
            Quota::Remaining(remaining) if remaining <= PROVIDER_RESERVE => {
                self.stopped = Some(BudgetExhausted::ProviderReserve { remaining });
            }
            Quota::UsedUp => self.stopped = Some(BudgetExhausted::ProviderLimit),
            Quota::Remaining(_) | Quota::Unknown => {}
        }
    }

    fn start_day(&mut self, today: NaiveDate) {
        if self.day != Some(today) {
            self.day = Some(today);
            self.used = 0;
            self.stopped = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(number: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, number).unwrap()
    }

    #[test]
    fn spends_up_to_the_limit() {
        let mut budget = DailyBudget::new(3);

        for _ in 0..3 {
            assert_eq!(budget.try_spend(day(10)), Ok(()));
        }
        assert_eq!(
            budget.try_spend(day(10)),
            Err(BudgetExhausted::LocalLimit { limit: 3 })
        );
    }

    #[test]
    fn a_new_utc_day_resets_the_count() {
        let mut budget = DailyBudget::new(1);

        assert_eq!(budget.try_spend(day(10)), Ok(()));
        assert!(budget.try_spend(day(10)).is_err());
        assert_eq!(budget.try_spend(day(11)), Ok(()));
    }

    #[test]
    fn a_low_provider_count_stops_calls_until_the_next_day() {
        let mut budget = DailyBudget::new(90);

        budget.observe(Quota::Remaining(11), day(10));
        assert_eq!(budget.try_spend(day(10)), Ok(()));
        budget.observe(Quota::Remaining(10), day(10));

        assert_eq!(
            budget.try_spend(day(10)),
            Err(BudgetExhausted::ProviderReserve { remaining: 10 })
        );
        assert_eq!(budget.try_spend(day(11)), Ok(()));
    }

    #[test]
    fn a_used_up_provider_limit_stops_calls_until_the_next_day() {
        let mut budget = DailyBudget::new(90);

        budget.observe(Quota::UsedUp, day(10));

        assert_eq!(
            budget.try_spend(day(10)),
            Err(BudgetExhausted::ProviderLimit)
        );
        assert_eq!(budget.try_spend(day(11)), Ok(()));
    }

    #[test]
    fn an_unknown_quota_changes_nothing() {
        let mut budget = DailyBudget::new(1);

        budget.observe(Quota::Unknown, day(10));

        assert_eq!(budget.try_spend(day(10)), Ok(()));
    }

    #[test]
    fn error_messages_say_when_the_budget_resets() {
        for reason in [
            BudgetExhausted::LocalLimit { limit: 90 },
            BudgetExhausted::ProviderReserve { remaining: 7 },
            BudgetExhausted::ProviderLimit,
        ] {
            assert!(reason.to_string().contains("00:00 UTC"), "{reason}");
        }
        assert!(
            BudgetExhausted::LocalLimit { limit: 90 }
                .to_string()
                .contains("90")
        );
    }
}
