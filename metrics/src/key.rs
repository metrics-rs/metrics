use crate::{Label, ScopedString};
use std::{fmt, slice::Iter};

/// A metric key.
///
/// A key always includes a name, but can optional include multiple labels used to further describe
/// the metric.
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct Key {
    name: ScopedString,
    labels: Option<Vec<Label>>,
}

impl Key {
    /// Creates a `Key` from a name.
    pub fn from_name<N>(name: N) -> Self
    where
        N: Into<ScopedString>,
    {
        Key {
            name: name.into(),
            labels: None,
        }
    }

    /// Creates a `Key` from a name and vector of `Label`s.
    pub fn from_name_and_labels<N>(name: N, labels: Vec<Label>) -> Self
    where
        N: Into<ScopedString>,
    {
        Key {
            name: name.into(),
            labels: Some(labels),
        }
    }

    /// Name of this key.
    pub fn name(&self) -> &ScopedString {
        &self.name
    }

    /// Labels of this key, if they exist.
    pub fn labels(&self) -> Option<Iter<Label>> {
        self.labels.as_ref().map(|xs| xs.iter())
    }

    /// Consumes this `Key`, returning the name and any labels.
    pub fn into_parts(self) -> (ScopedString, Option<Vec<Label>>) {
        (self.name, self.labels)
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.labels {
            None => write!(f, "Key({})", self.name),
            Some(labels) => {
                write!(f, "Key({}", self.name)?;

                if !labels.is_empty() {
                    let mut first = true;
                    write!(f, ", [")?;
                    for label in labels {
                        if first {
                            write!(f, "{} = {}", label.0, label.1)?;
                            first = false;
                        } else {
                            write!(f, ", {} = {}", label.0, label.1)?;
                        }
                    }
                     write!(f, "]")?;
                }
                write!(f, ")")
            },
        }
    }
}

impl From<String> for Key {
    fn from(name: String) -> Key {
        Key::from_name(name)
    }
}

impl From<&'static str> for Key {
    fn from(name: &'static str) -> Key {
        Key::from_name(name)
    }
}

#[cfg(test)]
mod tests {
    use super::Key;
    use crate::Label;

    #[test]
    fn test_key_proper_display() {
        let key1 = Key::from_name("foobar");
        let result1 = key1.to_string();
        assert_eq!(result1, "Key(foobar)");

        let key2 = Key::from_name_and_labels("foobar", vec![Label::new("system", "http")]);
        let result2 = key2.to_string();
        assert_eq!(result2, "Key(foobar, [system = http])");

        let key3 = Key::from_name_and_labels("foobar", vec![
            Label::new("system", "http"),
            Label::new("user", "joe"),
        ]);
        let result3 = key3.to_string();
        assert_eq!(result3, "Key(foobar, [system = http, user = joe])");

        let key4 = Key::from_name_and_labels("foobar", vec![
            Label::new("black", "black"),
            Label::new("lives", "lives"),
            Label::new("matter", "matter"),
        ]);
        let result4 = key4.to_string();
        assert_eq!(result4, "Key(foobar, [black = black, lives = lives, matter = matter])");
    }
}