use itertools::Itertools;
use metrics::{counter, Key, KeyName, Label};
use metrics_tracing_context::{LabelFilter, MetricsLayer, TracingContextLayer};
use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};
use metrics_util::{layers::Layer, CompositeKey, MetricKind};
use parking_lot::{const_mutex, Mutex, MutexGuard};
use tracing::dispatcher::{set_default, DefaultGuard, Dispatch};
use tracing::{span, Level};
use tracing_subscriber::{layer::SubscriberExt, Registry};

use pretty_assertions::assert_eq;

static TEST_MUTEX: Mutex<()> = const_mutex(());
static LOGIN_ATTEMPTS: &str = "login_attempts";
static LOGIN_ATTEMPTS_NONE: &str = "login_attempts_no_labels";
static LOGIN_ATTEMPTS_STATIC: &str = "login_attempts_static_labels";
static LOGIN_ATTEMPTS_DYNAMIC: &str = "login_attempts_dynamic_labels";
static LOGIN_ATTEMPTS_BOTH: &str = "login_attempts_static_and_dynamic_labels";
static MY_COUNTER: &str = "my_counter";
static USER_EMAIL: &[Label] = &[
    Label::from_static_parts("user", "ferris"),
    Label::from_static_parts("user.email", "ferris@rust-lang.org"),
];
static USER_EMAIL_ATTEMPT: &[Label] = &[
    Label::from_static_parts("user", "ferris"),
    Label::from_static_parts("user.email", "ferris@rust-lang.org"),
    Label::from_static_parts("attempt", "42"),
];
static USER_ID: &[Label] = &[Label::from_static_parts("user.id", "42")];
static EMAIL_USER: &[Label] = &[
    Label::from_static_parts("user", "ferris"),
    Label::from_static_parts("user.email", "ferris@rust-lang.org"),
];
static SVC_ENV: &[Label] = &[
    Label::from_static_parts("service", "login_service"),
    Label::from_static_parts("env", "test"),
];
static SVC_USER_EMAIL: &[Label] = &[
    Label::from_static_parts("user", "ferris"),
    Label::from_static_parts("user.email", "ferris@rust-lang.org"),
    Label::from_static_parts("service", "login_service"),
];
static SVC_USER_EMAIL_ID: &[Label] = &[
    Label::from_static_parts("user", "ferris"),
    Label::from_static_parts("user.email", "ferris@rust-lang.org"),
    Label::from_static_parts("user.id", "42"),
    Label::from_static_parts("service", "login_service"),
];
static NODE_USER_EMAIL: &[Label] = &[
    Label::from_static_parts("user", "ferris"),
    Label::from_static_parts("user.email", "ferris@rust-lang.org"),
    Label::from_static_parts("node_name", "localhost"),
];
static SVC_NODE_USER_EMAIL: &[Label] = &[
    Label::from_static_parts("user", "ferris"),
    Label::from_static_parts("user.email", "ferris@rust-lang.org"),
    Label::from_static_parts("service", "login_service"),
    Label::from_static_parts("node_name", "localhost"),
];
static COMBINED_LABELS: &[Label] = &[
    Label::from_static_parts("shared_field", "inner"),
    Label::from_static_parts("inner_specific", "foo"),
    Label::from_static_parts("inner_specific_dynamic", "foo_dynamic"),
    Label::from_static_parts("outer_specific", "bar"),
    Label::from_static_parts("outer_specific_dynamic", "bar_dynamic"),
];
static SAME_CALLSITE_PATH_1: &[Label] = &[
    Label::from_static_parts("shared_field", "path1"),
    Label::from_static_parts("path1_specific", "foo"),
    Label::from_static_parts("path1_specific_dynamic", "foo_dynamic"),
];
static SAME_CALLSITE_PATH_2: &[Label] = &[
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

    unsafe { metrics::clear_recorder() };
    metrics::set_boxed_recorder(Box::new(recorder)).expect("failed to install recorder");

    let test_guard =
        TestGuard { _test_mutex_guard: test_mutex_guard, _tracing_guard: tracing_guard };
    (test_guard, snapshotter)
}

#[test]
fn test_basic_functionality() {
    let (_guard, snapshotter) = setup(TracingContextLayer::all());

    let user = "ferris";
    let email = "ferris@rust-lang.org";
    let span = span!(Level::TRACE, "login", user, user.email = email);
    let _guard = span.enter();

    counter!("login_attempts", "service" => "login_service").increment(1);

    let snapshot = snapshotter.snapshot().into_vec();

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
    );
}

#[test]
fn test_basic_functionality_record() {
    let (_guard, snapshotter) = setup(TracingContextLayer::all());

    let user = "ferris";
    let email = "ferris@rust-lang.org";
    let span = span!(
        Level::TRACE,
        "login",
        user,
        user.email = email,
        user.id = tracing_core::field::Empty,
    );
    let _guard = span.enter();

    span.record("user.id", 42);
    counter!("login_attempts", "service" => "login_service").increment(1);

    let snapshot = snapshotter.snapshot().into_vec();

    assert_eq!(
        snapshot,
        vec![(
            CompositeKey::new(
                MetricKind::Counter,
                Key::from_static_parts(LOGIN_ATTEMPTS, SVC_USER_EMAIL_ID)
            ),
            None,
            None,
            DebugValue::Counter(1),
        )]
    );
}

#[test]
fn test_basic_functionality_then_record() {
    let (_guard, snapshotter) = setup(TracingContextLayer::all());

    let user = "ferris";
    let email = "ferris@rust-lang.org";
    let span = span!(
        Level::TRACE,
        "login",
        user,
        user.email = email,
        user.id = tracing_core::field::Empty,
    );
    let _guard = span.enter();
    let mut snapshots = vec![];
    {
        counter!("login_attempts", "service" => "login_service").increment(1);

        let snapshot = snapshotter.snapshot().into_vec();

        snapshots.push((
            CompositeKey::new(
                MetricKind::Counter,
                Key::from_static_parts(LOGIN_ATTEMPTS, SVC_USER_EMAIL),
            ),
            None,
            None,
            DebugValue::Counter(1),
        ));

        assert_eq!(snapshot, snapshots);
    }
    span.record("user.id", 42);
    {
        counter!("login_attempts", "service" => "login_service").increment(1);

        let snapshot = snapshotter.snapshot().into_vec();

        snapshots.push((
            CompositeKey::new(
                MetricKind::Counter,
                Key::from_static_parts(LOGIN_ATTEMPTS, SVC_USER_EMAIL_ID),
            ),
            None,
            None,
            DebugValue::Counter(1),
        ));

        assert_eq!(snapshot, snapshots);
    }
}

#[test]
fn test_rerecord() {
    static USER_ID_42: &[Label] = &[Label::from_static_parts("user.id", "42")];
    static USER_ID_123: &[Label] = &[Label::from_static_parts("user.id", "123")];

    let (_guard, snapshotter) = setup(TracingContextLayer::all());

    let span = span!(Level::TRACE, "login", user.id = tracing_core::field::Empty);
    let _guard = span.enter();

    span.record("user.id", 42);
    counter!("login_attempts").increment(1);

    span.record("user.id", 123);
    counter!("login_attempts").increment(1);

    let snapshot = snapshotter.snapshot().into_vec();

    assert_eq!(
        snapshot,
        vec![
            (
                CompositeKey::new(
                    MetricKind::Counter,
                    Key::from_static_parts(LOGIN_ATTEMPTS, USER_ID_42)
                ),
                None,
                None,
                DebugValue::Counter(1),
            ),
            (
                CompositeKey::new(
                    MetricKind::Counter,
                    Key::from_static_parts(LOGIN_ATTEMPTS, USER_ID_123)
                ),
                None,
                None,
                DebugValue::Counter(1),
            )
        ]
    );
}

#[test]
fn test_loop() {
    let (_guard, snapshotter) = setup(TracingContextLayer::all());

    let user = "ferris";
    let email = "ferris@rust-lang.org";
    let span = span!(
        Level::TRACE,
        "login",
        user,
        user.email = email,
        attempt = tracing_core::field::Empty,
    );
    let _guard = span.enter();

    for attempt in 1..=42 {
        span.record("attempt", attempt);
    }
    counter!("login_attempts").increment(1);

    let snapshot = snapshotter.snapshot().into_vec();

    assert_eq!(
        snapshot,
        vec![(
            CompositeKey::new(
                MetricKind::Counter,
                Key::from_static_parts(LOGIN_ATTEMPTS, USER_EMAIL_ATTEMPT)
            ),
            None,
            None,
            DebugValue::Counter(1),
        )]
    );
}

#[test]
fn test_record_does_not_overwrite() {
    static USER_ID_42: &[Label] = &[Label::from_static_parts("user.id", "42")];

    let (_guard, snapshotter) = setup(TracingContextLayer::all());

    let span = span!(Level::TRACE, "login", user.id = tracing_core::field::Empty);
    let _guard = span.enter();

    span.record("user.id", 666);
    counter!("login_attempts", "user.id" => "42").increment(1);

    let snapshot = snapshotter.snapshot().into_vec();

    assert_eq!(
        snapshot,
        vec![(
            CompositeKey::new(
                MetricKind::Counter,
                Key::from_static_parts(LOGIN_ATTEMPTS, USER_ID_42)
            ),
            None,
            None,
            DebugValue::Counter(1),
        )]
    );
}

#[test]
fn test_macro_forms() {
    let (_guard, snapshotter) = setup(TracingContextLayer::all());

    let user = "ferris";
    let email = "ferris@rust-lang.org";
    let span = span!(Level::TRACE, "login", user, user.email = email);
    let _guard = span.enter();

    // No labels.
    counter!("login_attempts_no_labels").increment(1);
    // Static labels only.
    counter!("login_attempts_static_labels", "service" => "login_service").increment(1);
    // Dynamic labels only.
    let node_name = "localhost".to_string();
    counter!("login_attempts_dynamic_labels", "node_name" => node_name.clone()).increment(1);
    // Static and dynamic.
    counter!(
        "login_attempts_static_and_dynamic_labels",
        "service" => "login_service",
        "node_name" => node_name,
    )
    .increment(1);

    let snapshot = snapshotter.snapshot().into_vec();

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
                    Key::from_static_parts(LOGIN_ATTEMPTS_STATIC, SVC_USER_EMAIL),
                ),
                None,
                None,
                DebugValue::Counter(1),
            ),
            (
                CompositeKey::new(
                    MetricKind::Counter,
                    Key::from_static_parts(LOGIN_ATTEMPTS_DYNAMIC, NODE_USER_EMAIL),
                ),
                None,
                None,
                DebugValue::Counter(1),
            ),
            (
                CompositeKey::new(
                    MetricKind::Counter,
                    Key::from_static_parts(LOGIN_ATTEMPTS_BOTH, SVC_NODE_USER_EMAIL),
                ),
                None,
                None,
                DebugValue::Counter(1),
            ),
        ]
    );
}

#[test]
fn test_no_labels() {
    let (_guard, snapshotter) = setup(TracingContextLayer::all());

    let span = span!(Level::TRACE, "login");
    let _guard = span.enter();

    counter!("login_attempts").increment(1);

    let snapshot = snapshotter.snapshot().into_vec();

    assert_eq!(
        snapshot,
        vec![(
            CompositeKey::new(MetricKind::Counter, Key::from_static_name(LOGIN_ATTEMPTS)),
            None,
            None,
            DebugValue::Counter(1),
        )]
    );
}

#[test]
fn test_no_labels_record() {
    let (_guard, snapshotter) = setup(TracingContextLayer::all());

    let span = span!(Level::TRACE, "login", user.id = tracing_core::field::Empty);
    let _guard = span.enter();

    span.record("user.id", 42);
    counter!("login_attempts").increment(1);

    let snapshot = snapshotter.snapshot().into_vec();

    assert_eq!(
        snapshot,
        vec![(
            CompositeKey::new(MetricKind::Counter, Key::from_static_parts(LOGIN_ATTEMPTS, USER_ID)),
            None,
            None,
            DebugValue::Counter(1),
        )]
    );
}

#[test]
fn test_multiple_paths_to_the_same_callsite() {
    let (_guard, snapshotter) = setup(TracingContextLayer::all());

    let shared_fn = || {
        counter!("my_counter").increment(1);
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

    let snapshot = snapshotter.snapshot().into_vec();

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
    );
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

        counter!("my_counter").increment(1);
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

    let snapshot = snapshotter.snapshot().into_vec();

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
    fn should_include_label(&self, _name: &KeyName, label: &Label) -> bool {
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

    counter!("login_attempts", "user.email" => "ferris@rust-lang.org").increment(1);

    let snapshot = snapshotter.snapshot().into_vec();

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
    );
}

#[test]
fn test_label_allowlist() {
    let (_guard, snapshotter) = setup(TracingContextLayer::only_allow(["env", "service"]));

    let user = "ferris";
    let email = "ferris@rust-lang.org";
    let span = span!(
        Level::TRACE,
        "login",
        user,
        user.email_span = email,
        service = "login_service",
        env = "test"
    );
    let _guard = span.enter();

    counter!("login_attempts").increment(1);

    let snapshot = snapshotter.snapshot().into_vec();

    assert_eq!(
        snapshot,
        vec![(
            CompositeKey::new(MetricKind::Counter, Key::from_static_parts(LOGIN_ATTEMPTS, SVC_ENV)),
            None,
            None,
            DebugValue::Counter(1),
        )]
    );
}

#[test]
fn test_all_permutations() {
    let perms = (0..9).map(|_| [false, true]).multi_cartesian_product();

    for v in perms {
        let [metric_has_labels, in_span, span_has_fields, span_field_same_as_metric, span_has_parent, parent_field_same_as_span, span_field_is_empty, record_field, emit_before_recording] =
            v[..]
        else {
            unreachable!("{:?}, {}", v, v.len());
        };

        test(
            metric_has_labels,
            in_span,
            span_has_fields,
            span_field_same_as_metric,
            span_has_parent,
            parent_field_same_as_span,
            span_field_is_empty,
            record_field,
            emit_before_recording,
        );
    }
}

#[allow(clippy::fn_params_excessive_bools, clippy::too_many_arguments, clippy::too_many_lines)]
fn test(
    metric_has_labels: bool,
    in_span: bool,
    span_has_fields: bool,
    span_field_same_as_metric: bool,
    span_has_parent: bool,
    parent_field_same_as_span: bool,
    span_field_is_empty: bool,
    record_field: bool,
    emit_before_recording: bool,
) {
    let (_guard, snapshotter) = setup(TracingContextLayer::all());

    {
        let parent = if span_field_same_as_metric && parent_field_same_as_span {
            tracing::trace_span!("outer", user.email = "changed@domain.com")
        } else {
            tracing::trace_span!("outer", user.id = 999)
        };

        let _parent_guard = span_has_parent.then(|| parent.enter());

        let span = if span_has_fields {
            match (span_field_same_as_metric, span_field_is_empty) {
                (false, false) => tracing::trace_span!("login", user.id = 666),
                (false, true) => {
                    tracing::trace_span!("login", user.id = tracing_core::field::Empty)
                }
                (true, false) => tracing::trace_span!("login", user.email = "user@domain.com"),
                (true, true) => {
                    tracing::trace_span!("login", user.email = tracing_core::field::Empty)
                }
            }
        } else {
            tracing::trace_span!("login")
        };

        let _guard = in_span.then(|| span.enter());

        let inc = || {
            if metric_has_labels {
                counter!("login_attempts", "user.email" => "ferris@rust-lang.org").increment(1);
            } else {
                counter!("login_attempts").increment(1);
            }
        };

        if emit_before_recording {
            inc();
        }

        if record_field {
            span.record("user.id", 42);
        }

        inc();
    }

    let snapshot = snapshotter.snapshot().into_vec();

    let mut expected = vec![];

    if in_span
        && span_has_fields
        && !span_field_same_as_metric
        && record_field
        && emit_before_recording
    {
        expected.push((
            CompositeKey::new(
                MetricKind::Counter,
                Key::from_parts(
                    LOGIN_ATTEMPTS,
                    IntoIterator::into_iter([
                        (span_has_parent || !span_field_is_empty).then(|| {
                            Label::new("user.id", if span_field_is_empty { "999" } else { "666" })
                        }),
                        metric_has_labels.then(|| Label::new("user.email", "ferris@rust-lang.org")),
                    ])
                    .flatten()
                    .collect::<Vec<_>>(),
                ),
            ),
            None,
            None,
            DebugValue::Counter(1),
        ));
    }

    let in_span_with_metric_field =
        in_span && span_has_fields && span_field_same_as_metric && !span_field_is_empty;
    let has_other_labels = !(!span_has_parent
        && (!in_span
            || (span_field_same_as_metric || !span_has_fields)
            || (!record_field && span_field_is_empty)))
        && !(span_field_same_as_metric && parent_field_same_as_span)
        && !in_span_with_metric_field;

    expected.push((
        CompositeKey::new(
            MetricKind::Counter,
            Key::from_parts(
                LOGIN_ATTEMPTS,
                IntoIterator::into_iter([
                    (metric_has_labels && !has_other_labels)
                        .then(|| Label::new("user.email", "ferris@rust-lang.org")),
                    (!metric_has_labels
                        && (in_span_with_metric_field
                            || span_field_same_as_metric
                                && span_has_parent
                                && parent_field_same_as_span))
                        .then(|| {
                            if in_span_with_metric_field {
                                Label::new("user.email", "user@domain.com")
                            } else {
                                Label::new("user.email", "changed@domain.com")
                            }
                        }),
                    if in_span && span_has_fields && !span_field_same_as_metric && record_field {
                        Some(Label::new("user.id", "42"))
                    } else if in_span
                        && span_has_fields
                        && !span_field_same_as_metric
                        && !span_field_is_empty
                        && !record_field
                    {
                        Some(Label::new("user.id", "666"))
                    } else if (!in_span || !span_has_fields || span_field_same_as_metric)
                        && (!span_field_same_as_metric || !parent_field_same_as_span)
                        && span_has_parent
                        || span_has_parent
                            && span_field_is_empty
                            && !record_field
                            && !span_field_same_as_metric
                    {
                        Some(Label::new("user.id", "999"))
                    } else {
                        None
                    },
                    (metric_has_labels && has_other_labels)
                        .then(|| Label::new("user.email", "ferris@rust-lang.org")),
                ])
                .flatten()
                .collect::<Vec<_>>(),
            ),
        ),
        None,
        None,
        DebugValue::Counter(
            if !emit_before_recording
                || in_span && span_has_fields && !span_field_same_as_metric && record_field
            {
                1
            } else {
                2
            },
        ),
    ));

    assert_eq!(snapshot, expected);
}
