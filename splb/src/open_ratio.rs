//! Mixer tail gate. Fast vs quality is a product profile, not a self-race.
pub fn open_ratio() -> f64 {
    std::env::var("PCC_OPEN_RATIO")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|x: &f64| *x > 0.0 && *x <= 1.0)
        .unwrap_or(crate::OPEN_RATIO)
}
