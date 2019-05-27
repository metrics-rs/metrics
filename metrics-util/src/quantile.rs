/// A quantile that has both the raw value and a human-friendly display label.
///
/// We work with quantiles for optimal floating-point precison over percentiles, but most of the
/// time, monitoring systems show us percentiles, and usually in an abbreviated form: `p99`.
///
/// On top of holding the quantile value, we calculate the familiar "p99" style of label, doing the
/// appropriate percentile conversion.  Thus, if you have a quantile of `0.99`, the resulting label
/// is `p99`, and if you have a quantile of `0.999`, the resulting label is `p999`.
///
/// There are two special cases, where we label `0.0` and `1.0` as `min` and `max`, respectively.
#[derive(Debug, Clone, PartialEq)]
pub struct Quantile(f64, String);

impl Quantile {
    /// Creates a new [`Quantile`] from a floating-point value.
    ///
    /// All values are clamped between 0.0 and 1.0.
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
            }
        };

        Quantile(clamped, label)
    }

    /// Gets the human-friendly display label.
    pub fn label(&self) -> &str {
        self.1.as_str()
    }

    /// Gets the raw quantile value.
    pub fn value(&self) -> f64 {
        self.0
    }
}

/// Parses a slice of floating-point values into a vector of [`Quantile`]s.
pub fn parse_quantiles(quantiles: &[f64]) -> Vec<Quantile> {
    quantiles.iter().map(|f| Quantile::new(*f)).collect()
}

#[cfg(test)]
mod tests {
    use super::{parse_quantiles, Quantile};

    #[test]
    fn test_quantiles() {
        let min = Quantile::new(0.0);
        assert_eq!(min.value(), 0.0);
        assert_eq!(min.label(), "min");

        let max = Quantile::new(1.0);
        assert_eq!(max.value(), 1.0);
        assert_eq!(max.label(), "max");

        let p99 = Quantile::new(0.99);
        assert_eq!(p99.value(), 0.99);
        assert_eq!(p99.label(), "p99");

        let p999 = Quantile::new(0.999);
        assert_eq!(p999.value(), 0.999);
        assert_eq!(p999.label(), "p999");

        let p9999 = Quantile::new(0.9999);
        assert_eq!(p9999.value(), 0.9999);
        assert_eq!(p9999.label(), "p9999");

        let under = Quantile::new(-1.0);
        assert_eq!(under.value(), 0.0);
        assert_eq!(under.label(), "min");

        let over = Quantile::new(1.2);
        assert_eq!(over.value(), 1.0);
        assert_eq!(over.label(), "max");
    }

    #[test]
    fn test_parse_quantiles() {
        let empty = vec![];
        let result = parse_quantiles(&empty);
        assert_eq!(result.len(), 0);

        let normal = vec![0.0, 0.5, 0.99, 0.999, 1.0];
        let result = parse_quantiles(&normal);
        assert_eq!(result.len(), 5);
        assert_eq!(result[0], Quantile::new(0.0));
        assert_eq!(result[1], Quantile::new(0.5));
        assert_eq!(result[2], Quantile::new(0.99));
        assert_eq!(result[3], Quantile::new(0.999));
        assert_eq!(result[4], Quantile::new(1.0));
    }
}
