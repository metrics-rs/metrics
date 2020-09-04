use metrics::{counter, Key, Label};
use metrics_tracing_context::{MetricsLayer, TracingContextLayer};
use metrics_util::{layers::Layer, DebugValue, DebuggingRecorder, MetricKind, Snapshotter};
use tracing::dispatcher::{set_global_default, Dispatch};
use tracing::{span, Level};
use tracing_subscriber::{layer::SubscriberExt, Registry};

fn setup() -> Snapshotter {
    let subscriber = Registry::default().with(MetricsLayer::new());
    set_global_default(Dispatch::new(subscriber)).unwrap();

    let recorder = DebuggingRecorder::new();
    let snapshotter = recorder.snapshotter();
    let recorder = TracingContextLayer.layer(recorder);

    metrics::set_boxed_recorder(Box::new(recorder)).expect("failed to install recorder");

    snapshotter
}

#[test]
fn test_basic_functionality() {
    let snapshotter = setup();

    let user = "ferris";
    let email = "ferris@rust-lang.org";
    let span = span!(Level::TRACE, "login", user, user.email = email);
    let _guard = span.enter();

    counter!("login_attempts", 1, "service" => "login_service");

    let snapshot = snapshotter.snapshot();

    assert_eq!(
        snapshot,
        vec![(
            MetricKind::Counter,
            Key::from_name_and_labels(
                "login_attempts",
                vec![
                    Label::new("service", "login_service"),
                    Label::new("user", "ferris"),
                    Label::new("user.email", "ferris@rust-lang.org"),
                ],
            ),
            DebugValue::Counter(1),
        )]
    )
}

#[test]
fn test_no_labels() {
    let snapshotter = setup();

    let span = span!(Level::TRACE, "login");
    let _guard = span.enter();

    counter!("login_attempts", 1);

    let snapshot = snapshotter.snapshot();

    assert_eq!(
        snapshot,
        vec![(
            MetricKind::Counter,
            Key::from_name("login_attempts",),
            DebugValue::Counter(1),
        )]
    )
}

#[test]
fn test_multiple_paths_to_the_same_callsite() {
    let snapshotter = setup();

    let shared_fn = || {
        counter!("my_counter", 1);
    };

    let path1 = || {
        let path1_specific_dynamic = "foo_dynamic";
        let span = span!(
            Level::TRACE,
            "path1",
            shared_field = "path1",
            path1_specific = "foo",
            path1_specific_dynamic,
        );
        let _guard = span.enter();
        shared_fn();
    };

    let path2 = || {
        let path2_specific_dynamic = "bar_dynamic";
        let span = span!(
            Level::TRACE,
            "path2",
            shared_field = "path2",
            path2_specific = "foo",
            path2_specific_dynamic,
        );
        let _guard = span.enter();
        shared_fn();
    };

    path1();
    path2();

    let snapshot = snapshotter.snapshot();

    assert_eq!(
        snapshot,
        vec![
            (
                MetricKind::Counter,
                Key::from_name_and_labels(
                    "my_counter",
                    vec![
                        Label::new("shared_field", "path1"),
                        Label::new("path1_specific", "foo"),
                        Label::new("path1_specific_dynamic", "foo_dynamic"),
                    ],
                ),
                DebugValue::Counter(1),
            ),
            (
                MetricKind::Counter,
                Key::from_name_and_labels(
                    "my_counter",
                    vec![
                        Label::new("shared_field", "path2"),
                        Label::new("path2_specific", "bar"),
                        Label::new("path2_specific_dynamic", "bar_dynamic"),
                    ],
                ),
                DebugValue::Counter(1),
            )
        ]
    )
}
