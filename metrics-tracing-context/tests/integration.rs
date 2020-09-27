use std::collections::HashSet;

use metrics::{counter, KeyData, Label};
use metrics_tracing_context::{LabelFilter, MetricsLayer, TracingContextLayer};
use metrics_util::{layers::Layer, DebugValue, DebuggingRecorder, MetricKind, Snapshotter};
use parking_lot::{const_mutex, Mutex, MutexGuard};
use tracing::dispatcher::{set_default, DefaultGuard, Dispatch};
use tracing::{span, Level};
use tracing_subscriber::{layer::SubscriberExt, Registry};

static TEST_MUTEX: Mutex<()> = const_mutex(());

struct TestGuard {
    _test_mutex_guard: MutexGuard<'static, ()>,
    _tracing_guard: DefaultGuard,
}

fn setup<F>(layer: TracingContextLayer<F>) -> (TestGuard, Snapshotter)
where
    F: LabelFilter + Clone + 'static,
{
    let test_mutex_guard = TEST_MUTEX.lock();
    let subscriber = Registry::default().with(MetricsLayer::new());
    let tracing_guard = set_default(&Dispatch::new(subscriber));

    let recorder = DebuggingRecorder::new();
    let snapshotter = recorder.snapshotter();
    let recorder = layer.layer(recorder);

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
    let (_guard, snapshotter) = setup(TracingContextLayer::all());

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
            KeyData::from_name_and_labels(
                "login_attempts",
                vec![
                    Label::new("service", "login_service"),
                    Label::new("user", "ferris"),
                    Label::new("user.email", "ferris@rust-lang.org"),
                ],
            )
            .into(),
            DebugValue::Counter(1),
        )]
    )
}

#[test]
fn test_macro_forms() {
    let (_guard, snapshotter) = setup(TracingContextLayer::all());

    let user = "ferris";
    let email = "ferris@rust-lang.org";
    let span = span!(Level::TRACE, "login", user, user.email = email);
    let _guard = span.enter();

    // No labels.
    counter!("login_attempts_no_labels", 1);
    // Static labels only.
    counter!("login_attempts_static_labels", 1, "service" => "login_service");
    // Dynamic labels only.
    let node_name = "localhost".to_string();
    counter!("login_attempts_dynamic_labels", 1, "node_name" => node_name.clone());
    // Static and dynamic.
    counter!("login_attempts_static_and_dynamic_labels", 1,
        "service" => "login_service", "node_name" => node_name.clone());

    let snapshot = snapshotter.snapshot();
    let snapshot: HashSet<_> = snapshot.into_iter().collect();

    assert_eq!(
        snapshot,
        vec![
            (
                MetricKind::Counter,
                KeyData::from_name_and_labels(
                    "login_attempts_no_labels",
                    vec![
                        Label::new("user", "ferris"),
                        Label::new("user.email", "ferris@rust-lang.org"),
                    ],
                )
                .into(),
                DebugValue::Counter(1),
            ),
            (
                MetricKind::Counter,
                KeyData::from_name_and_labels(
                    "login_attempts_static_labels",
                    vec![
                        Label::new("service", "login_service"),
                        Label::new("user", "ferris"),
                        Label::new("user.email", "ferris@rust-lang.org"),
                    ],
                )
                .into(),
                DebugValue::Counter(1),
            ),
            (
                MetricKind::Counter,
                KeyData::from_name_and_labels(
                    "login_attempts_dynamic_labels",
                    vec![
                        Label::new("node_name", "localhost"),
                        Label::new("user", "ferris"),
                        Label::new("user.email", "ferris@rust-lang.org"),
                    ],
                )
                .into(),
                DebugValue::Counter(1),
            ),
            (
                MetricKind::Counter,
                KeyData::from_name_and_labels(
                    "login_attempts_static_and_dynamic_labels",
                    vec![
                        Label::new("service", "login_service"),
                        Label::new("node_name", "localhost"),
                        Label::new("user", "ferris"),
                        Label::new("user.email", "ferris@rust-lang.org"),
                    ],
                )
                .into(),
                DebugValue::Counter(1),
            ),
        ]
        .into_iter()
        .collect()
    )
}

#[test]
fn test_no_labels() {
    let (_guard, snapshotter) = setup(TracingContextLayer::all());

    let span = span!(Level::TRACE, "login");
    let _guard = span.enter();

    counter!("login_attempts", 1);

    let snapshot = snapshotter.snapshot();

    assert_eq!(
        snapshot,
        vec![(
            MetricKind::Counter,
            KeyData::from_name("login_attempts").into(),
            DebugValue::Counter(1),
        )]
    )
}

#[test]
fn test_multiple_paths_to_the_same_callsite() {
    let (_guard, snapshotter) = setup(TracingContextLayer::all());

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
            path2_specific = "bar",
            path2_specific_dynamic,
        );
        let _guard = span.enter();
        shared_fn();
    };

    path1();
    path2();

    let snapshot = snapshotter.snapshot();
    let snapshot: HashSet<_> = snapshot.into_iter().collect();

    assert_eq!(
        snapshot,
        vec![
            (
                MetricKind::Counter,
                KeyData::from_name_and_labels(
                    "my_counter",
                    vec![
                        Label::new("shared_field", "path1"),
                        Label::new("path1_specific", "foo"),
                        Label::new("path1_specific_dynamic", "foo_dynamic"),
                    ],
                )
                .into(),
                DebugValue::Counter(1),
            ),
            (
                MetricKind::Counter,
                KeyData::from_name_and_labels(
                    "my_counter",
                    vec![
                        Label::new("shared_field", "path2"),
                        Label::new("path2_specific", "bar"),
                        Label::new("path2_specific_dynamic", "bar_dynamic"),
                    ],
                )
                .into(),
                DebugValue::Counter(1),
            )
        ]
        .into_iter()
        .collect()
    )
}

#[test]
fn test_nested_spans() {
    let (_guard, snapshotter) = setup(TracingContextLayer::all());

    let inner = || {
        let inner_specific_dynamic = "foo_dynamic";
        let span = span!(
            Level::TRACE,
            "inner",
            shared_field = "inner",
            inner_specific = "foo",
            inner_specific_dynamic,
        );
        let _guard = span.enter();

        counter!("my_counter", 1);
    };

    let outer = || {
        let outer_specific_dynamic = "bar_dynamic";
        let span = span!(
            Level::TRACE,
            "outer",
            shared_field = "outer",
            outer_specific = "bar",
            outer_specific_dynamic,
        );
        let _guard = span.enter();
        inner();
    };

    outer();

    let snapshot = snapshotter.snapshot();
    let snapshot: HashSet<_> = snapshot.into_iter().collect();

    assert_eq!(
        snapshot,
        vec![(
            MetricKind::Counter,
            KeyData::from_name_and_labels(
                "my_counter",
                vec![
                    Label::new("shared_field", "inner"),
                    Label::new("inner_specific", "foo"),
                    Label::new("inner_specific_dynamic", "foo_dynamic"),
                    Label::new("shared_field", "outer"),
                    Label::new("outer_specific", "bar"),
                    Label::new("outer_specific_dynamic", "bar_dynamic"),
                ],
            )
            .into(),
            DebugValue::Counter(1),
        ),]
        .into_iter()
        .collect()
    )
}

#[derive(Clone)]
struct OnlyUser;

impl LabelFilter for OnlyUser {
    fn should_include_label(&self, label: &Label) -> bool {
        label.key() == "user"
    }
}

#[test]
fn test_label_filtering() {
    let (_guard, snapshotter) = setup(TracingContextLayer::new(OnlyUser));

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
            KeyData::from_name_and_labels(
                "login_attempts",
                vec![
                    Label::new("service", "login_service"),
                    Label::new("user", "ferris"),
                ],
            )
            .into(),
            DebugValue::Counter(1),
        )]
    )
}
