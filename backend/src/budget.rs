use chrono::NaiveDate;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "the app used its daily budget of {limit} Highlightly requests; the budget resets at 00:00 UTC"
)]
pub struct BudgetExhausted {
    pub limit: u32,
}

#[derive(Debug)]
pub struct DailyBudget {
    limit: u32,
    day: Option<NaiveDate>,
    used: u32,
}

impl DailyBudget {
    pub fn new(limit: u32) -> Self {
        Self {
            limit,
            day: None,
            used: 0,
        }
    }

    pub fn try_spend(&mut self, today: NaiveDate) -> Result<(), BudgetExhausted> {
        if self.day != Some(today) {
            self.day = Some(today);
            self.used = 0;
        }
        if self.used >= self.limit {
            return Err(BudgetExhausted { limit: self.limit });
        }
        self.used += 1;
        Ok(())
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
        assert_eq!(budget.try_spend(day(10)), Err(BudgetExhausted { limit: 3 }));
    }

    #[test]
    fn a_new_utc_day_resets_the_count() {
        let mut budget = DailyBudget::new(1);

        assert_eq!(budget.try_spend(day(10)), Ok(()));
        assert!(budget.try_spend(day(10)).is_err());
        assert_eq!(budget.try_spend(day(11)), Ok(()));
    }

    #[test]
    fn error_message_says_when_the_budget_resets() {
        let message = BudgetExhausted { limit: 90 }.to_string();

        assert!(message.contains("90"));
        assert!(message.contains("00:00 UTC"));
    }
}
