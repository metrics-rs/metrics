use criterion::{criterion_group, criterion_main, Criterion};
use metrics::{Key, KeyData, Label, NoopRecorder, Recorder, SharedString};
use metrics_tracing_context::{MetricsLayer, TracingContextLayer};
use metrics_util::layers::Layer;
use tracing::{
    dispatcher::{with_default, Dispatch},
    span, Level,
};
use tracing_subscriber::{layer::SubscriberExt, Registry};

fn layer_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("layer");
    group.bench_function("base case", |b| {
        let recorder = NoopRecorder;
        static KEY_NAME: [SharedString; 1] = [SharedString::const_str("key")];
        static KEY_LABELS: [Label; 1] = [Label::from_static_parts("foo", "bar")];
        static KEY_DATA: KeyData = KeyData::from_static_parts(&KEY_NAME, &KEY_LABELS);

        b.iter(|| {
            recorder.increment_counter(Key::Borrowed(&KEY_DATA), 1);
        })
    });
    group.bench_function("layer overhead", |b| {
        let recorder = passthrough::Layer {
            inner: NoopRecorder,
        };
        static KEY_NAME: [SharedString; 1] = [SharedString::const_str("key")];
        static KEY_LABELS: [Label; 1] = [Label::from_static_parts("foo", "bar")];
        static KEY_DATA: KeyData = KeyData::from_static_parts(&KEY_NAME, &KEY_LABELS);

        b.iter(|| {
            recorder.increment_counter(Key::Borrowed(&KEY_DATA), 1);
        })
    });
    group.bench_function("reassambly overhead", |b| {
        let recorder = reassambly::Layer {
            inner: NoopRecorder,
        };
        static KEY_NAME: [SharedString; 1] = [SharedString::const_str("key")];
        static KEY_LABELS: [Label; 1] = [Label::from_static_parts("foo", "bar")];
        static KEY_DATA: KeyData = KeyData::from_static_parts(&KEY_NAME, &KEY_LABELS);

        b.iter(|| {
            recorder.increment_counter(Key::Borrowed(&KEY_DATA), 1);
        })
    });
    group.bench_function("enhancement overhead", |b| {
        let recorder = enhancer::Layer {
            inner: NoopRecorder,
        };
        static KEY_NAME: [SharedString; 1] = [SharedString::const_str("key")];
        static KEY_LABELS: [Label; 1] = [Label::from_static_parts("foo", "bar")];
        static KEY_DATA: KeyData = KeyData::from_static_parts(&KEY_NAME, &KEY_LABELS);

        b.iter(|| {
            recorder.increment_counter(Key::Borrowed(&KEY_DATA), 1);
        })
    });
    group.bench_function("no integration", |b| {
        let subscriber = Registry::default();
        let dispatcher = Dispatch::new(subscriber);
        with_default(&dispatcher, || {
            let user = "ferris";
            let email = "ferris@rust-lang.org";
            let span = span!(Level::TRACE, "login", user, user.email = email);
            let _guard = span.enter();

            let recorder = NoopRecorder;
            static KEY_NAME: [SharedString; 1] = [SharedString::const_str("key")];
            static KEY_LABELS: [Label; 1] = [Label::from_static_parts("foo", "bar")];
            static KEY_DATA: KeyData = KeyData::from_static_parts(&KEY_NAME, &KEY_LABELS);

            b.iter(|| {
                recorder.increment_counter(Key::Borrowed(&KEY_DATA), 1);
            })
        })
    });
    group.bench_function("tracing layer only", |b| {
        let subscriber = Registry::default().with(MetricsLayer::all());
        let dispatcher = Dispatch::new(subscriber);
        with_default(&dispatcher, || {
            let user = "ferris";
            let email = "ferris@rust-lang.org";
            let span = span!(Level::TRACE, "login", user, user.email = email);
            let _guard = span.enter();

            let recorder = NoopRecorder;
            static KEY_NAME: [SharedString; 1] = [SharedString::const_str("key")];
            static KEY_LABELS: [Label; 1] = [Label::from_static_parts("foo", "bar")];
            static KEY_DATA: KeyData = KeyData::from_static_parts(&KEY_NAME, &KEY_LABELS);

            b.iter(|| {
                recorder.increment_counter(Key::Borrowed(&KEY_DATA), 1);
            })
        })
    });
    group.bench_function("metrics layer only", |b| {
        let subscriber = Registry::default();
        let dispatcher = Dispatch::new(subscriber);
        with_default(&dispatcher, || {
            let user = "ferris";
            let email = "ferris@rust-lang.org";
            let span = span!(Level::TRACE, "login", user, user.email = email);
            let _guard = span.enter();

            let tracing_layer = TracingContextLayer::all();
            let recorder = tracing_layer.layer(NoopRecorder);
            static KEY_NAME: [SharedString; 1] = [SharedString::const_str("key")];
            static KEY_LABELS: [Label; 1] = [Label::from_static_parts("foo", "bar")];
            static KEY_DATA: KeyData = KeyData::from_static_parts(&KEY_NAME, &KEY_LABELS);

            b.iter(|| {
                recorder.increment_counter(Key::Borrowed(&KEY_DATA), 1);
            })
        })
    });
    group.bench_function("full integration", |b| {
        let subscriber = Registry::default().with(MetricsLayer::all());
        let dispatcher = Dispatch::new(subscriber);
        with_default(&dispatcher, || {
            let user = "ferris";
            let email = "ferris@rust-lang.org";
            let span = span!(Level::TRACE, "login", user, user.email = email);
            let _guard = span.enter();

            let tracing_layer = TracingContextLayer::all();
            let recorder = tracing_layer.layer(NoopRecorder);
            static KEY_NAME: [SharedString; 1] = [SharedString::const_str("key")];
            static KEY_LABELS: [Label; 1] = [Label::from_static_parts("foo", "bar")];
            static KEY_DATA: KeyData = KeyData::from_static_parts(&KEY_NAME, &KEY_LABELS);

            b.iter(|| {
                recorder.increment_counter(Key::Borrowed(&KEY_DATA), 1);
            })
        })
    });
    group.finish();
}

criterion_group!(benches, layer_benchmark);
criterion_main!(benches);

/// A simple passthrough layer.
/// Allows us to estimate the penalty of the layer.
mod passthrough {
    use super::*;
    use metrics::{GaugeValue, Unit};

    pub struct Layer<R> {
        pub inner: R,
    }

    impl<R> Recorder for Layer<R>
    where
        R: Recorder,
    {
        fn register_counter(
            &self,
            key: Key,
            unit: Option<Unit>,
            description: Option<&'static str>,
        ) {
            self.inner.register_counter(key, unit, description)
        }

        fn register_gauge(&self, key: Key, unit: Option<Unit>, description: Option<&'static str>) {
            self.inner.register_gauge(key, unit, description)
        }

        fn register_histogram(
            &self,
            key: Key,
            unit: Option<Unit>,
            description: Option<&'static str>,
        ) {
            self.inner.register_histogram(key, unit, description)
        }

        fn increment_counter(&self, key: Key, value: u64) {
            self.inner.increment_counter(key, value);
        }

        fn update_gauge(&self, key: Key, value: GaugeValue) {
            self.inner.update_gauge(key, value);
        }

        fn record_histogram(&self, key: Key, value: f64) {
            self.inner.record_histogram(key, value);
        }
    }
}

/// A layer with a dummy layer implementation that only reassambles the Key.
/// Allows us to estimate the penalty of the said reassambly.
mod reassambly {
    use super::*;
    use metrics::{GaugeValue, Unit};

    pub struct Layer<R> {
        pub inner: R,
    }

    impl<R> Layer<R> {
        fn enhance_key(&self, key: Key) -> Key {
            let (name, labels) = key.into_owned().into_parts();
            // nothing here, just reassembly
            KeyData::from_parts(name, labels).into()
        }
    }

    impl<R> Recorder for Layer<R>
    where
        R: Recorder,
    {
        fn register_counter(
            &self,
            key: Key,
            unit: Option<Unit>,
            description: Option<&'static str>,
        ) {
            self.inner.register_counter(key, unit, description)
        }

        fn register_gauge(&self, key: Key, unit: Option<Unit>, description: Option<&'static str>) {
            self.inner.register_gauge(key, unit, description)
        }

        fn register_histogram(
            &self,
            key: Key,
            unit: Option<Unit>,
            description: Option<&'static str>,
        ) {
            self.inner.register_histogram(key, unit, description)
        }

        fn increment_counter(&self, key: Key, value: u64) {
            let key = self.enhance_key(key);
            self.inner.increment_counter(key, value);
        }

        fn update_gauge(&self, key: Key, value: GaugeValue) {
            let key = self.enhance_key(key);
            self.inner.update_gauge(key, value);
        }

        fn record_histogram(&self, key: Key, value: f64) {
            let key = self.enhance_key(key);
            self.inner.record_histogram(key, value);
        }
    }
}

/// A simple layer that adds an owned string layer.
/// Allows us to estimate the penalty of a layer with this operation.
mod enhancer {
    use super::*;
    use metrics::{GaugeValue, Unit};

    pub struct Layer<R> {
        pub inner: R,
    }

    impl<R> Layer<R> {
        fn enhance_labels(&self, labels: &mut Vec<Label>) {
            // Key is static, value is not.
            labels.push(Label::new("mykey", "myval".to_owned()));
        }

        fn enhance_key(&self, key: Key) -> Key {
            let (name, mut labels) = key.into_owned().into_parts();
            self.enhance_labels(&mut labels);
            KeyData::from_parts(name, labels).into()
        }
    }

    impl<R> Recorder for Layer<R>
    where
        R: Recorder,
    {
        fn register_counter(
            &self,
            key: Key,
            unit: Option<Unit>,
            description: Option<&'static str>,
        ) {
            self.inner.register_counter(key, unit, description)
        }

        fn register_gauge(&self, key: Key, unit: Option<Unit>, description: Option<&'static str>) {
            self.inner.register_gauge(key, unit, description)
        }

        fn register_histogram(
            &self,
            key: Key,
            unit: Option<Unit>,
            description: Option<&'static str>,
        ) {
            self.inner.register_histogram(key, unit, description)
        }

        fn increment_counter(&self, key: Key, value: u64) {
            let key = self.enhance_key(key);
            self.inner.increment_counter(key, value);
        }

        fn update_gauge(&self, key: Key, value: GaugeValue) {
            let key = self.enhance_key(key);
            self.inner.update_gauge(key, value);
        }

        fn record_histogram(&self, key: Key, value: f64) {
            let key = self.enhance_key(key);
            self.inner.record_histogram(key, value);
        }
    }
}
