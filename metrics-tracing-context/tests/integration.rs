use metrics::{counter, Key, Label};
use metrics_tracing_context::{MetricsLayer, TracingContextLayer};
use metrics_util::{layers::Layer, DebugValue, DebuggingRecorder, MetricKind, Snapshotter};
use parking_lot::{Mutex, MutexGuard, const_mutex};
use tracing::dispatcher::{set_default, DefaultGuard, Dispatch};
use tracing::{span, Level};
use tracing_subscriber::{layer::SubscriberExt, Registry};

static TEST_MUTEX: Mutex<()> = const_mutex(());

struct TestGuard {
    _test_mutex_guard: MutexGuard<'static, ()>,
    _tracing_guard: DefaultGuard,
}

fn setup(fields: Option<Vec<String>>) -> (TestGuard, Snapshotter) {
    let test_mutex_guard = TEST_MUTEX.lock();
    let subscriber = Registry::default().with(MetricsLayer::new());
    let tracing_guard = set_default(&Dispatch::new(subscriber));

    let recorder = DebuggingRecorder::new();
    let snapshotter = recorder.snapshotter();
    let fields = fields.unwrap_or_else(|| Vec::new());
    let tracing_layer = TracingContextLayer::with_global_fields(fields);
    let recorder = tracing_layer.layer(recorder);

    metrics::clear_recorder();
    metrics::set_boxed_recorder(Box::new(recorder)).expect("failed to install recorder");

    let test_guard = TestGuard {
        _test_mutex_guard: test_mutex_guard,
        _tracing_guard: tracing_guard,
    };
    (test_guard, snapshotter)
}

#[test]
fn test_basic_functionality() {
    let (_guard, snapshotter) = setup(None);

    let user = "ferris";
    let email = "ferris@rust-lang.org";
    let span = span!(Level::TRACE, "login", user, user.email = email);
    let _guard = span.enter();

    counter!("login_attempts", 1, "service" => "login_service", "user", "user.email");

    let snapshot = snapshotter.snapshot();

    assert_eq!(
        snapshot,
        vec![(
            MetricKind::Counter,
            Key::from_name_and_labels(
                "login_attempts",
                vec![
                    Label::from_static("service", "login_service"),
                    Label::from_dynamic_with_value("user", "ferris"),
                    Label::from_dynamic_with_value("user.email", "ferris@rust-lang.org"),
                ],
            ),
            DebugValue::Counter(1),
        )]
    )
}

#[test]
fn test_no_labels() {
    let (_guard, snapshotter) = setup(None);

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
    let (_guard, snapshotter) = setup(None);

    let shared_fn = || {
        counter!("my_counter", 1, "shared_field", "dynamic_field");
    };

    let path1 = || {
        let dynamic_field = "foo_dynamic";
        let span = span!(
            Level::TRACE,
            "path1",
            shared_field = "path1",
            dynamic_field,
        );
        let _guard = span.enter();
        shared_fn();
    };

    let path2 = || {
        let dynamic_field = "bar_dynamic";
        let span = span!(
            Level::TRACE,
            "path2",
            shared_field = "path2",
            dynamic_field,
        );
        let _guard = span.enter();
        shared_fn();
    };

    path1();
    path2();

    let mut snapshot = snapshotter.snapshot();
    snapshot.sort();

    assert_eq!(
        snapshot,
        vec![
            (
                MetricKind::Counter,
                Key::from_name_and_labels(
                    "my_counter",
                    vec![
                        Label::from_dynamic_with_value("shared_field", "path1"),
                        Label::from_dynamic_with_value("dynamic_field", "foo_dynamic"),
                    ],
                ),
                DebugValue::Counter(1),
            ),
            (
                MetricKind::Counter,
                Key::from_name_and_labels(
                    "my_counter",
                    vec![
                        Label::from_dynamic_with_value("shared_field", "path2"),
                        Label::from_dynamic_with_value("dynamic_field", "bar_dynamic"),
                    ],
                ),
                DebugValue::Counter(1),
            )
        ]
    )
}

#[test]
fn test_global_fields() {
    let fields = vec![String::from("common"), String::from("rare")];
    let (_guard, snapshotter) = setup(Some(fields));

    let shared_fn1 = || {
        counter!("my_counter", 1, "common");
    };

    let shared_fn2 = || {
        counter!("my_counter", 1);
    };

    let common1 = || {
        let span = span!(Level::TRACE, "span1", common = "common1");
        let _guard = span.enter();
        shared_fn1();
    };

    let common2 = || {
        let span = span!(Level::TRACE, "span2", common = "common2", rare = "rare2");
        let _guard = span.enter();
        shared_fn1();
    };

    let common3 = || {
        let span = span!(Level::TRACE, "span3", common = "common3");
        let _guard = span.enter();
        shared_fn2();
    };

    let common4 = || {
        let span = span!(Level::TRACE, "span4", common = "common4", rare = "rare4");
        let _guard = span.enter();
        shared_fn2();
    };

    common1();
    common2();
    common3();
    common4();

    let mut snapshot = snapshotter.snapshot();
    snapshot.sort();

    assert_eq!(
        snapshot,
        vec![
            (
                MetricKind::Counter,
                Key::from_name_and_labels(
                    "my_counter",
                    vec![
                        Label::from_dynamic_with_value("common", "common1"),
                        Label::from_dynamic("rare"),
                    ],
                ),
                DebugValue::Counter(1),
            ),
            (
                MetricKind::Counter,
                Key::from_name_and_labels(
                    "my_counter",
                    vec![
                        Label::from_dynamic_with_value("common", "common2"),
                        Label::from_dynamic_with_value("rare", "rare2"),
                    ],
                ),
                DebugValue::Counter(1),
            ),
            (
                MetricKind::Counter,
                Key::from_name_and_labels(
                    "my_counter",
                    vec![
                        Label::from_dynamic_with_value("common", "common3"),
                        Label::from_dynamic("rare"),
                    ],
                ),
                DebugValue::Counter(1),
            ),
            (
                MetricKind::Counter,
                Key::from_name_and_labels(
                    "my_counter",
                    vec![
                        Label::from_dynamic_with_value("common", "common4"),
                        Label::from_dynamic_with_value("rare", "rare4"),
                    ],
                ),
                DebugValue::Counter(1),
            )
        ]
    )
}