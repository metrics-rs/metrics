use std::{error::Error, io};

use chrono::Local;
use termion::{event::Key, input::MouseTerminal, raw::IntoRawMode, screen::AlternateScreen};
use tui::{
    backend::TermionBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Paragraph, Wrap, List, ListItem},
    Terminal,
};

mod input;
use self::input::InputEvents;

mod metrics;
use self::metrics::{ClientState, MetricData};

mod selector;
use self::selector::Selector;

fn main() -> Result<(), Box<dyn Error>> {
    let stdout = io::stdout().into_raw_mode()?;
    let stdout = MouseTerminal::from(stdout);
    let stdout = AlternateScreen::from(stdout);
    let backend = TermionBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut events = InputEvents::new();
    let client = metrics::Client::new("127.0.0.1:5000".to_string());
    let mut selector = Selector::new();

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints(
                    [
                        Constraint::Length(4),
                        Constraint::Percentage(90)
                    ].as_ref()
                )
                .split(f.size());

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

                    Spans::from(spans)
                },
                ClientState::Connected => Spans::from(vec![
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
                client_state.into(),
                Spans::from(vec![
                    Span::styled("controls: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw("up/down = scroll, q = quit"),
                ]),
            ];
            let header = Paragraph::new(text)
                .block(header_block)
                .wrap(Wrap { trim: true });

            f.render_widget(header, chunks[0]);

            // Knock 5 off the line width to account for 3-character highlight symbol + borders.
            let line_width = chunks[1].width.saturating_sub(6) as usize;
            let items = client.with_metrics(|metrics| {
                let mut items = Vec::new();
                for (key, value) in metrics.iter() {
                    let inner_key = key.key();
                    let name = inner_key.name();
                    let labels = inner_key.labels().map(|label| format!("{} = {}", label.key(), label.value())).collect::<Vec<_>>();
                    let display_name = if labels.is_empty() {
                        name.to_string()
                    } else {
                        format!("{} [{}]", name, labels.join(", "))
                    };

                    let display_value = match value {
                        MetricData::Counter(value) => format!("total: {}", value),
                        MetricData::Gauge(value) => format!("current: {}", value),
                        MetricData::Histogram(value) => {
                            let min = value.min();
                            let max = value.max();
                            let p50 = value.value_at_quantile(0.5);
                            let p99 = value.value_at_quantile(0.99);
                            let p999 = value.value_at_quantile(0.999);

                            format!("min: {} p50: {} p99: {} p999: {} max: {}",
                                min, p50, p99, p999, max)
                        },
                    };

                    let name_length = display_name.chars().count();
                    let value_length = display_value.chars().count();
                    let space = line_width.saturating_sub(name_length).saturating_sub(value_length);

                    let display = format!("{}{}{}", display_name, " ".repeat(space), display_value);
                    items.push(ListItem::new(display));
                }
                items
            });
            selector.set_length(items.len());

            let metrics_block = Block::default()
                .title("observed metrics")
                .borders(Borders::ALL);

            let metrics = List::new(items)
                .block(metrics_block)
                .highlight_symbol(">> ");
            
            f.render_stateful_widget(metrics, chunks[1], selector.state());
        })?;

        // Poll the event queue for input events.  `next` will only block for 1 second,
        // so our screen is never stale by more than 1 second.
        if let Some(input) = events.next()? {
            match input {
                Key::Char('q') => break,
                Key::Up => selector.previous(),
                Key::Down => selector.next(),
                Key::PageUp => selector.top(),
                Key::PageDown => selector.bottom(),
                _ => {},
            }
        }
    }

    Ok(())
}