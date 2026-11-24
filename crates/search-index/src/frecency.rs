//! Frecency (Frequency + Recency) Calculation Engine

use std::time::{SystemTime, UNIX_EPOCH};

const FOUR_HOURS_SECS: u64 = 4 * 3600;
const TWENTY_FOUR_HOURS_SECS: u64 = 24 * 3600;
const SEVEN_DAYS_SECS: u64 = 7 * 86400;
const THIRTY_DAYS_SECS: u64 = 30 * 86400;

/// Computes frecency score based on usage frequency, recency, and pinned status.
pub fn calculate_frecency(
    use_count: u32,
    last_used_epoch: u64,
    current_epoch: u64,
    is_pinned: bool,
) -> f32 {
    let age_secs = current_epoch.saturating_sub(last_used_epoch);

    let recency_weight = if age_secs <= FOUR_HOURS_SECS {
        100.0
    } else if age_secs <= TWENTY_FOUR_HOURS_SECS {
        80.0
    } else if age_secs <= SEVEN_DAYS_SECS {
        60.0
    } else if age_secs <= THIRTY_DAYS_SECS {
        40.0
    } else {
        20.0
    };

    let base_score = (use_count as f32).min(50.0) * recency_weight;
    let pin_bonus = if is_pinned { 200.0 } else { 0.0 };

    // Normalize into [0.0, 1.0] scale where 5000 max score maps to ~1.0
    ((base_score + pin_bonus) / 5200.0).clamp(0.0, 1.0)
}

/// Current system time in Unix epoch seconds.
pub fn current_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
