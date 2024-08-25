use std::num::FpCategory;
use std::time::Duration;
use std::{error::Error, io};
use std::{fmt, io::Stdout};

use chrono::Local;
use metrics::Unit;
use ratatui::{
    backend::CrosstermBackend,
    crossterm::{
        event::KeyCode,
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    },
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Terminal,
};

mod input;
use self::input::InputEvents;

// Module name/crate name collision that we have to deal with.
#[path = "metrics.rs"]
mod metrics_inner;
use self::metrics_inner::{ClientState, MetricData};

mod selector;
use self::selector::Selector;

fn main() -> Result<(), Box<dyn Error>> {
    let terminal = init_terminal()?;
    let result = run(terminal);
    restore_terminal()?;
    result
}

fn run(mut terminal: Terminal<CrosstermBackend<Stdout>>) -> Result<(), Box<dyn Error>> {
    let address = std::env::args().nth(1).unwrap_or_else(|| "127.0.0.1:5000".to_owned());
    let client = metrics_inner::Client::new(address);
    let mut selector = Selector::new();
    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([Constraint::Length(4), Constraint::Percentage(90)].as_ref())
                .split(f.area());

            let current_dt = Local::now().format(" (%Y/%m/%d %I:%M:%S %p)").to_string();
            let client_state = match client.state() {
                ClientState::Disconnected(s) => {
                    let mut spans = vec![
                        Span::raw("state: "),
                        Span::styled("disconnected", Style::default().fg(Color::Red)),
                    ];

                    if let Some(s) = s {
                        spans.push(Span::raw(" "));
                        spans.push(Span::raw(s));
                    }

                    Line::from(spans)
                }
                ClientState::Connected => Line::from(vec![
                    Span::raw("state: "),
                    Span::styled("connected", Style::default().fg(Color::Green)),
                ]),
            };

            let header_block = Block::default()
                .title(vec![
                    Span::styled("metrics-observer", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(current_dt),
                ])
                .borders(Borders::ALL);

            let text = vec![
                client_state,
                Line::from(vec![
                    Span::styled("controls: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw("up/down = scroll, q = quit"),
                ]),
            ];
            let header = Paragraph::new(text).block(header_block).wrap(Wrap { trim: true });

            f.render_widget(header, chunks[0]);

            // Knock 5 off the line width to account for 3-character highlight symbol + borders.
            let line_width = chunks[1].width.saturating_sub(6) as usize;
            let mut items = Vec::new();
            let metrics = client.get_metrics();
            for (key, value, unit, _desc) in metrics {
                let inner_key = key.key();
                let name = inner_key.name();
                let labels = inner_key
                    .labels()
                    .map(|label| format!("{} = {}", label.key(), label.value()))
                    .collect::<Vec<_>>();
                let display_name = if labels.is_empty() {
                    name.to_string()
                } else {
                    format!("{} [{}]", name, labels.join(", "))
                };

                let display_value = match value {
                    MetricData::Counter(value) => {
                        format!("total: {}", u64_to_displayable(value, unit))
                    }
                    MetricData::Gauge(value) => {
                        format!("current: {}", f64_to_displayable(value, unit))
                    }
                    MetricData::Histogram(value) => {
                        let min = value.min();
                        let max = value.max();
                        let p50 = value.quantile(0.5).expect("sketch shouldn't exist if no values");
                        let p99 =
                            value.quantile(0.99).expect("sketch shouldn't exist if no values");
                        let p999 =
                            value.quantile(0.999).expect("sketch shouldn't exist if no values");

                        format!(
                            "min: {} p50: {} p99: {} p999: {} max: {}",
                            f64_to_displayable(min, unit),
                            f64_to_displayable(p50, unit),
                            f64_to_displayable(p99, unit),
                            f64_to_displayable(p999, unit),
                            f64_to_displayable(max, unit),
                        )
                    }
                };

                let name_length = display_name.chars().count();
                let value_length = display_value.chars().count();
                let space = line_width.saturating_sub(name_length).saturating_sub(value_length);

                let display = format!("{}{}{}", display_name, " ".repeat(space), display_value);
                items.push(ListItem::new(display));
            }
            selector.set_length(items.len());

            let metrics_block = Block::default().title("observed metrics").borders(Borders::ALL);

            let metrics = List::new(items)
                .block(metrics_block)
                .highlight_style(Style::default().add_modifier(Modifier::BOLD).fg(Color::LightCyan))
                .highlight_symbol(">> ");

            f.render_stateful_widget(metrics, chunks[1], selector.state());
        })?;

        // Poll the event queue for input events.  `next` will only block for 1 second,
        // so our screen is never stale by more than 1 second.
        if let Some(input) = InputEvents::next()? {
            match input.code {
                KeyCode::Char('q') => break,
                KeyCode::Up => selector.previous(),
                KeyCode::Down => selector.next(),
                KeyCode::PageUp => selector.top(),
                KeyCode::PageDown => selector.bottom(),
                _ => {}
            }
        }
    }
    Ok(())
}

fn init_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    Terminal::new(CrosstermBackend::new(io::stdout()))
}

fn restore_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)
}

fn u64_to_displayable(value: u64, unit: Option<Unit>) -> String {
    let unit = match unit {
        None => return value.to_string(),
        Some(inner) => inner,
    };

    if unit.is_data_based() {
        return u64_data_to_displayable(value, unit);
    }

    if unit.is_time_based() {
        return u64_time_to_displayable(value, unit);
    }

    let label = unit.as_canonical_label();
    format!("{}{}", value, label)
}

fn f64_to_displayable(value: f64, unit: Option<Unit>) -> String {
    let unit = match unit {
        None => return value.to_string(),
        Some(inner) => inner,
    };

    if unit.is_data_based() {
        return f64_data_to_displayable(value, unit);
    }

    if unit.is_time_based() {
        return f64_time_to_displayable(value, unit);
    }

    let label = unit.as_canonical_label();
    format!("{:.2}{}", value, label)
}

fn u64_data_to_displayable(value: u64, unit: Unit) -> String {
    f64_data_to_displayable(value as f64, unit)
}

fn f64_data_to_displayable(value: f64, unit: Unit) -> String {
    let delimiter = 1024_f64;
    let units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let unit_idx_max = units.len() as u32 - 1;
    let offset = match unit {
        Unit::Kibibytes => 1,
        Unit::Mebibytes => 2,
        Unit::Gibibytes => 3,
        Unit::Tebibytes => 4,
        _ => 0,
    };

    let mut exponent = (value.ln() / delimiter.ln()).floor() as u32;
    let mut unit_idx = exponent + offset;
    if unit_idx > unit_idx_max {
        exponent -= unit_idx - unit_idx_max;
        unit_idx = unit_idx_max;
    }
    let scaled = value / delimiter.powi(exponent as i32);

    let unit = units[unit_idx as usize];
    format!("{:.2} {}", scaled, unit)
}

fn u64_time_to_displayable(value: u64, unit: Unit) -> String {
    let dur = match unit {
        Unit::Nanoseconds => Duration::from_nanos(value),
        Unit::Microseconds => Duration::from_micros(value),
        Unit::Milliseconds => Duration::from_millis(value),
        Unit::Seconds => Duration::from_secs(value),
        // If it's not a time-based unit, then just format the value plainly.
        _ => return value.to_string(),
    };

    format!("{:?}", TruncatedDuration(dur))
}

fn f64_time_to_displayable(value: f64, unit: Unit) -> String {
    // Calculate how much we need to scale the value by, since `Duration` only takes f64 values if
    // they are at the seconds granularity, although obviously they could contain significant digits
    // for subsecond precision.
    let scaling_factor = match unit {
        Unit::Nanoseconds => Some(1_000_000_000.0),
        Unit::Microseconds => Some(1_000_000.0),
        Unit::Milliseconds => Some(1_000.0),
        Unit::Seconds => None,
        // If it's not a time-based unit, then just format the value plainly.
        _ => return value.to_string(),
    };

    let adjusted = match scaling_factor {
        Some(factor) => value / factor,
        None => value,
    };

    let sign = if adjusted < 0.0 { "-" } else { "" };
    let normalized = adjusted.abs();
    if !normalized.is_normal() && normalized.classify() != FpCategory::Zero {
        // We need a normalized number, but unlike `is_normal`, `Duration` is fine with a value that
        // is at zero, so we just exclude that here.
        return value.to_string();
    }

    let dur = Duration::from_secs_f64(normalized);

    format!("{}{:?}", sign, TruncatedDuration(dur))
}

struct TruncatedDuration(Duration);

impl fmt::Debug for TruncatedDuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        /// Formats a floating point number in decimal notation.
        ///
        /// The number is given as the `integer_part` and a fractional part.
        /// The value of the fractional part is `fractional_part / divisor`. So
        /// `integer_part` = 3, `fractional_part` = 12 and `divisor` = 100
        /// represents the number `3.012`. Trailing zeros are omitted.
        ///
        /// `divisor` must not be above 100_000_000. It also should be a power
        /// of 10, everything else doesn't make sense. `fractional_part` has
        /// to be less than `10 * divisor`!
        fn fmt_decimal(
            f: &mut fmt::Formatter<'_>,
            mut integer_part: u64,
            mut fractional_part: u32,
            mut divisor: u32,
            precision: usize,
        ) -> fmt::Result {
            // Encode the fractional part into a temporary buffer. The buffer
            // only need to hold 9 elements, because `fractional_part` has to
            // be smaller than 10^9. The buffer is prefilled with '0' digits
            // to simplify the code below.
            let mut buf = [b'0'; 9];
            let precision = if precision > 9 { 9 } else { precision };

            // The next digit is written at this position
            let mut pos = 0;

            // We keep writing digits into the buffer while there are non-zero
            // digits left and we haven't written enough digits yet.
            while fractional_part > 0 && pos < precision {
                // Write new digit into the buffer
                buf[pos] = b'0' + (fractional_part / divisor) as u8;

                fractional_part %= divisor;
                divisor /= 10;
                pos += 1;
            }

            // If a precision < 9 was specified, there may be some non-zero
            // digits left that weren't written into the buffer. In that case we
            // need to perform rounding to match the semantics of printing
            // normal floating point numbers. However, we only need to do work
            // when rounding up. This happens if the first digit of the
            // remaining ones is >= 5.
            if fractional_part > 0 && fractional_part >= divisor * 5 {
                // Round up the number contained in the buffer. We go through
                // the buffer backwards and keep track of the carry.
                let mut rev_pos = pos;
                let mut carry = true;
                while carry && rev_pos > 0 {
                    rev_pos -= 1;

                    // If the digit in the buffer is not '9', we just need to
                    // increment it and can stop then (since we don't have a
                    // carry anymore). Otherwise, we set it to '0' (overflow)
                    // and continue.
                    if buf[rev_pos] < b'9' {
                        buf[rev_pos] += 1;
                        carry = false;
                    } else {
                        buf[rev_pos] = b'0';
                    }
                }

                // If we still have the carry bit set, that means that we set
                // the whole buffer to '0's and need to increment the integer
                // part.
                if carry {
                    integer_part += 1;
                }
            }

            // If we haven't emitted a single fractional digit and the precision
            // wasn't set to a non-zero value, we don't print the decimal point.
            if pos == 0 {
                write!(f, "{}", integer_part)
            } else {
                // SAFETY: We are only writing ASCII digits into the buffer and it was
                // initialized with '0's, so it contains valid UTF8.
                let s = unsafe { std::str::from_utf8_unchecked(&buf[..pos]) };
                let s = s.trim_end_matches('0');

                write!(f, "{}.{}", integer_part, s)
            }
        }

        // Print leading '+' sign if requested
        if f.sign_plus() {
            write!(f, "+")?;
        }

        let secs = self.0.as_secs();
        let sub_nanos = self.0.subsec_nanos();
        let nanos = self.0.as_nanos();

        if secs > 0 {
            fmt_decimal(f, secs, sub_nanos, 100_000_000, 3)?;
            f.write_str("s")
        } else if nanos >= 1_000_000 {
            fmt_decimal(f, nanos as u64 / 1_000_000, (nanos % 1_000_000) as u32, 100_000, 2)?;
            f.write_str("ms")
        } else if nanos >= 1_000 {
            fmt_decimal(f, nanos as u64 / 1_000, (nanos % 1_000) as u32, 100, 1)?;
            f.write_str("µs")
        } else {
            fmt_decimal(f, nanos as u64, 0, 1, 0)?;
            f.write_str("ns")
        }
    }
}
