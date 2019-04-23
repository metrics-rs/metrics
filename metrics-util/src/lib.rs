//! Helper types and functions used within the metrics ecosystem.

/// A quantile that has both the raw value and a human-friendly display label.
#[derive(Clone)]
pub struct Quantile(f64, String);

impl Quantile {
    /// Creates a new `Quantile` from a floating-point value.
    ///
    /// All values clamped between 0.0 and 1.0.
    pub fn new(quantile: f64) -> Quantile {
        let clamped = quantile.max(0.0);
        let clamped = clamped.min(1.0);
        let display = clamped * 100.0;

        let raw_label = format!("{}", clamped);
        let label = match raw_label.as_str() {
            "0" => "min".to_string(),
            "1" => "max".to_string(),
            _ => {
                let raw = format!("p{}", display);
                raw.replace(".", "")
            },
        };

        Quantile(clamped, label)
    }

    /// Gets the human-friendly display label for this quantile.
    pub fn label(&self) -> &str {
        self.1.as_str()
    }

    /// Gets the raw value for this quantile.
    pub fn value(&self) -> f64 {
        self.0
    }
}

/// Parses a list of floating-point values into a list of `Quantile`s.
pub fn parse_quantiles(quantiles: &[f64]) -> Vec<Quantile> {
    quantiles.iter()
        .map(|f| Quantile::new(*f))
        .collect()
}
