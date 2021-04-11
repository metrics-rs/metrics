use metrics::{counter, Key, Label, SharedString};
use metrics_tracing_context::{LabelFilter, MetricsLayer, TracingContextLayer};
use metrics_util::{
    layers::Layer, CompositeKey, DebugValue, DebuggingRecorder, MetricKind, Snapshotter,
};
use parking_lot::{const_mutex, Mutex, MutexGuard};
use tracing::dispatcher::{set_default, DefaultGuard, Dispatch};
use tracing::{span, Level};
use tracing_subscriber::{layer::SubscriberExt, Registry};

static TEST_MUTEX: Mutex<()> = const_mutex(());
static LOGIN_ATTEMPTS: &'static [SharedString] = &[SharedString::const_str("login_attempts")];
static LOGIN_ATTEMPTS_NONE: &'static [SharedString] =
    &[SharedString::const_str("login_attempts_no_labels")];
static LOGIN_ATTEMPTS_STATIC: &'static [SharedString] =
    &[SharedString::const_str("login_attempts_static_labels")];
static LOGIN_ATTEMPTS_DYNAMIC: &'static [SharedString] =
    &[SharedString::const_str("login_attempts_dynamic_labels")];
static LOGIN_ATTEMPTS_BOTH: &'static [SharedString] = &[SharedString::const_str(
    "login_attempts_static_and_dynamic_labels",
)];
static MY_COUNTER: &'static [SharedString] = &[SharedString::const_str("my_counter")];
static USER_EMAIL: &'static [Label] = &[
    Label::from_static_parts("user", "ferris"),
    Label::from_static_parts("user.email", "ferris@rust-lang.org"),
];
static EMAIL_USER: &'static [Label] = &[
    Label::from_static_parts("user.email", "ferris@rust-lang.org"),
    Label::from_static_parts("user", "ferris"),
];
static SVC_USER_EMAIL: &'static [Label] = &[
    Label::from_static_parts("service", "login_service"),
    Label::from_static_parts("user", "ferris"),
    Label::from_static_parts("user.email", "ferris@rust-lang.org"),
];
static NODE_USER_EMAIL: &'static [Label] = &[
    Label::from_static_parts("node_name", "localhost"),
    Label::from_static_parts("user", "ferris"),
    Label::from_static_parts("user.email", "ferris@rust-lang.org"),
];
static SVC_NODE_USER_EMAIL: &'static [Label] = &[
    Label::from_static_parts("service", "login_service"),
    Label::from_static_parts("node_name", "localhost"),
    Label::from_static_parts("user", "ferris"),
    Label::from_static_parts("user.email", "ferris@rust-lang.org"),
];
static COMBINED_LABELS: &'static [Label] = &[
    Label::from_static_parts("shared_field", "inner"),
    Label::from_static_parts("inner_specific", "foo"),
    Label::from_static_parts("inner_specific_dynamic", "foo_dynamic"),
    Label::from_static_parts("shared_field", "outer"),
    Label::from_static_parts("outer_specific", "bar"),
    Label::from_static_parts("outer_specific_dynamic", "bar_dynamic"),
];
static SAME_CALLSITE_PATH_1: &'static [Label] = &[
    Label::from_static_parts("shared_field", "path1"),
    Label::from_static_parts("path1_specific", "foo"),
    Label::from_static_parts("path1_specific_dynamic", "foo_dynamic"),
];
static SAME_CALLSITE_PATH_2: &'static [Label] = &[
    Label::from_static_parts("shared_field", "path2"),
    Label::from_static_parts("path2_specific", "bar"),
    Label::from_static_parts("path2_specific_dynamic", "bar_dynamic"),
];

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
            CompositeKey::new(
                MetricKind::Counter,
                Key::from_static_parts(LOGIN_ATTEMPTS, SVC_USER_EMAIL)
            ),
            None,
            None,
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

    assert_eq!(
        snapshot,
        vec![
            (
                CompositeKey::new(
                    MetricKind::Counter,
                    Key::from_static_parts(LOGIN_ATTEMPTS_NONE, USER_EMAIL)
                ),
                None,
                None,
                DebugValue::Counter(1),
            ),
            (
                CompositeKey::new(
                    MetricKind::Counter,
                    Key::from_static_parts(
                        LOGIN_ATTEMPTS_STATIC,
                        SVC_USER_EMAIL
                    ),
                ),
                None,
                None,
                DebugValue::Counter(1),
            ),
            (
                CompositeKey::new(
                    MetricKind::Counter,
                    Key::from_static_parts(
                        LOGIN_ATTEMPTS_DYNAMIC,
                        NODE_USER_EMAIL
                    ),
                ),
                None,
                None,
                DebugValue::Counter(1),
            ),
            (
                CompositeKey::new(
                    MetricKind::Counter,
                    Key::from_static_parts(
                        LOGIN_ATTEMPTS_BOTH,
                        SVC_NODE_USER_EMAIL
                    ),
                ),
                None,
                None,
                DebugValue::Counter(1),
            ),
        ]
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
            CompositeKey::new(
                MetricKind::Counter,
                Key::from_static_name(LOGIN_ATTEMPTS),
            ),
            None,
            None,
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

    assert_eq!(
        snapshot,
        vec![
            (
                CompositeKey::new(
                    MetricKind::Counter,
                    Key::from_static_parts(MY_COUNTER, SAME_CALLSITE_PATH_1),
                ),
                None,
                None,
                DebugValue::Counter(1),
            ),
            (
                CompositeKey::new(
                    MetricKind::Counter,
                    Key::from_static_parts(MY_COUNTER, SAME_CALLSITE_PATH_2),
                ),
                None,
                None,
                DebugValue::Counter(1),
            )
        ]
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

    assert_eq!(
        snapshot,
        vec![(
            CompositeKey::new(
                MetricKind::Counter,
                Key::from_static_parts(MY_COUNTER, COMBINED_LABELS)
            ),
            None,
            None,
            DebugValue::Counter(1),
        )]
    );
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
    let span = span!(Level::TRACE, "login", user, user.email_span = email);
    let _guard = span.enter();

    counter!("login_attempts", 1, "user.email" => "ferris@rust-lang.org");

    let snapshot = snapshotter.snapshot();

    assert_eq!(
        snapshot,
        vec![(
            CompositeKey::new(
                MetricKind::Counter,
                Key::from_static_parts(LOGIN_ATTEMPTS, EMAIL_USER)
            ),
            None,
            None,
            DebugValue::Counter(1),
        )]
    )
}
