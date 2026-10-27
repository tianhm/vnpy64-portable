//! One depth row on the board.

#[derive(Clone, Debug, PartialEq)]
pub struct Quote {
    pub instrument: String,
    pub last: Option<f64>,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub bid_vol: i32,
    pub ask_vol: i32,
    pub volume: i32,
    pub open_interest: f64,
    pub pre_settlement: Option<f64>,
    pub update_time: String,
}

impl Quote {
    pub fn placeholder(instrument: &str) -> Self {
        Self {
            instrument: instrument.to_owned(),
            last: None,
            bid: None,
            ask: None,
            bid_vol: 0,
            ask_vol: 0,
            volume: 0,
            open_interest: 0.0,
            pre_settlement: None,
            update_time: "--".to_owned(),
        }
    }

    pub fn change(&self) -> Option<f64> {
        price_change(self.last, self.pre_settlement)
    }
}

/// CTP uses `DBL_MAX` for a price that is not set.
pub fn valid_price(value: f64) -> Option<f64> {
    if value.is_finite() && value.abs() <= 1e20 {
        Some(value)
    } else {
        None
    }
}

pub fn price_change(last: Option<f64>, pre_settlement: Option<f64>) -> Option<f64> {
    match (last, pre_settlement) {
        (Some(last), Some(pre)) if pre != 0.0 => Some(last - pre),
        _ => None,
    }
}

pub fn format_price(value: Option<f64>) -> String {
    match value {
        Some(value) => format!("{value:.2}"),
        None => "--".to_owned(),
    }
}

pub fn format_change(value: Option<f64>) -> String {
    match value {
        Some(value) if value > 0.0 => format!("+{value:.2}"),
        Some(value) => format!("{value:.2}"),
        None => "--".to_owned(),
    }
}

pub fn format_oi(value: f64) -> String {
    if value.is_finite() {
        format!("{value:.0}")
    } else {
        "--".to_owned()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Up,
    Down,
    Flat,
}

pub fn move_of(change: Option<f64>) -> Move {
    match change {
        Some(value) if value > 0.0 => Move::Up,
        Some(value) if value < 0.0 => Move::Down,
        _ => Move::Flat,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_uses_pre_settlement() {
        assert_eq!(price_change(Some(101.0), Some(100.0)), Some(1.0));
        assert_eq!(price_change(Some(99.0), Some(100.0)), Some(-1.0));
        assert_eq!(price_change(Some(1.0), Some(0.0)), None);
        assert_eq!(price_change(None, Some(1.0)), None);
    }

    #[test]
    fn dbl_max_is_not_a_price() {
        assert!(valid_price(f64::MAX).is_none());
        assert!(valid_price(f64::NAN).is_none());
        assert!(valid_price(f64::INFINITY).is_none());
        assert_eq!(valid_price(3500.5), Some(3500.5));
        assert_eq!(format_price(valid_price(f64::MAX)), "--");
        assert_eq!(format_change(Some(1.2)), "+1.20");
        assert_eq!(format_change(Some(-1.0)), "-1.00");
    }
}
