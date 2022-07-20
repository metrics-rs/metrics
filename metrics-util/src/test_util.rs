use metrics::{Counter, Gauge, Histogram, Key, KeyName, Recorder, SharedString, Unit};
use mockall::{
    mock,
    predicate::{self, eq},
    Predicate,
};

#[derive(Clone)]
pub enum RecorderOperation {
    DescribeCounter(KeyName, Option<Unit>, SharedString),
    DescribeGauge(KeyName, Option<Unit>, SharedString),
    DescribeHistogram(KeyName, Option<Unit>, SharedString),
    RegisterCounter(Key, Counter),
    RegisterGauge(Key, Gauge),
    RegisterHistogram(Key, Histogram),
}

impl RecorderOperation {
    fn apply_to_mock(self, mock: &mut MockBasicRecorder) {
        match self {
            RecorderOperation::DescribeCounter(key_name, unit, desc) => {
                expect_describe_counter(mock, key_name, unit, desc)
            }
            RecorderOperation::DescribeGauge(key_name, unit, desc) => {
                expect_describe_gauge(mock, key_name, unit, desc)
            }
            RecorderOperation::DescribeHistogram(key_name, unit, desc) => {
                expect_describe_histogram(mock, key_name, unit, desc)
            }
            RecorderOperation::RegisterCounter(key, counter) => {
                expect_register_counter(mock, key, counter)
            }
            RecorderOperation::RegisterGauge(key, gauge) => expect_register_gauge(mock, key, gauge),
            RecorderOperation::RegisterHistogram(key, histogram) => {
                expect_register_histogram(mock, key, histogram)
            }
        }
    }

    pub fn apply_to_recorder<R>(self, recorder: &R)
    where
        R: Recorder,
    {
        match self {
            RecorderOperation::DescribeCounter(key_name, unit, desc) => {
                recorder.describe_counter(key_name, unit, desc);
            }
            RecorderOperation::DescribeGauge(key_name, unit, desc) => {
                recorder.describe_gauge(key_name, unit, desc);
            }
            RecorderOperation::DescribeHistogram(key_name, unit, desc) => {
                recorder.describe_histogram(key_name, unit, desc);
            }
            RecorderOperation::RegisterCounter(key, _) => {
                let _ = recorder.register_counter(&key);
            }
            RecorderOperation::RegisterGauge(key, _) => {
                let _ = recorder.register_gauge(&key);
            }
            RecorderOperation::RegisterHistogram(key, _) => {
                let _ = recorder.register_histogram(&key);
            }
        }
    }
}

mock! {
    pub BasicRecorder {}

    impl Recorder for BasicRecorder {
        fn describe_counter(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString);
        fn describe_gauge(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString);
        fn describe_histogram(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString);
        fn register_counter(&self, key: &Key) -> Counter;
        fn register_gauge(&self, key: &Key) -> Gauge;
        fn register_histogram(&self, key: &Key) -> Histogram;
    }
}

impl MockBasicRecorder {
    pub fn from_operations<O>(operations: O) -> Self
    where
        O: IntoIterator<Item = RecorderOperation>,
    {
        let mut recorder = Self::new();
        for operation in operations.into_iter() {
            operation.apply_to_mock(&mut recorder);
        }
        recorder
    }
}

pub fn expect_describe_counter(
    mock: &mut MockBasicRecorder,
    key_name: KeyName,
    unit: Option<Unit>,
    description: SharedString,
) {
    mock.expect_describe_counter()
        .times(1)
        .with(eq(key_name), eq(unit), eq(description))
        .return_const(());
}

pub fn expect_describe_gauge(
    mock: &mut MockBasicRecorder,
    key_name: KeyName,
    unit: Option<Unit>,
    description: SharedString,
) {
    mock.expect_describe_gauge()
        .times(1)
        .with(eq(key_name), eq(unit), eq(description))
        .return_const(());
}

pub fn expect_describe_histogram(
    mock: &mut MockBasicRecorder,
    key_name: KeyName,
    unit: Option<Unit>,
    description: SharedString,
) {
    mock.expect_describe_histogram()
        .times(1)
        .with(eq(key_name), eq(unit), eq(description))
        .return_const(());
}

pub fn expect_register_counter(mock: &mut MockBasicRecorder, key: Key, counter: Counter) {
    mock.expect_register_counter().times(1).with(ref_eq(key)).return_const(counter);
}

pub fn expect_register_gauge(mock: &mut MockBasicRecorder, key: Key, gauge: Gauge) {
    mock.expect_register_gauge().times(1).with(ref_eq(key)).return_const(gauge);
}

pub fn expect_register_histogram(mock: &mut MockBasicRecorder, key: Key, histogram: Histogram) {
    mock.expect_register_histogram().times(1).with(ref_eq(key)).return_const(histogram);
}

fn ref_eq<T: PartialEq>(value: T) -> impl Predicate<T> {
    predicate::function(move |item: &T| item == &value)
}
