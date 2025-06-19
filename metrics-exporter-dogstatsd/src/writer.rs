use std::{
    ops::{Deref, DerefMut},
    vec::Drain,
};

use metrics::{Key, Label};

const SMALLEST_VALID_PAYLOAD: &[u8] = b"a:0|c\n";

#[derive(Clone, Copy)]
enum MetricType {
    Counter,
    Gauge,
    Histogram,
    Distribution,
}

impl MetricType {
    fn as_bytes(self) -> &'static [u8] {
        match self {
            MetricType::Counter => b"|c",
            MetricType::Gauge => b"|g",
            MetricType::Histogram => b"|h",
            MetricType::Distribution => b"|d",
        }
    }
}

#[derive(Clone, Copy)]
enum MetricValue {
    Integer(u64),
    FloatingPoint(f64),
}

struct MetricValueFormatter {
    int_writer: itoa::Buffer,
    float_writer: ryu::Buffer,
}

impl MetricValueFormatter {
    fn new() -> Self {
        Self { int_writer: itoa::Buffer::new(), float_writer: ryu::Buffer::new() }
    }

    fn format(&mut self, value: MetricValue) -> &str {
        match value {
            MetricValue::Integer(v) => self.int_writer.format(v),
            MetricValue::FloatingPoint(v) => self.float_writer.format(v),
        }
    }
}

pub struct WriteResult {
    payloads_written: u64,
    points_dropped: u64,
}

impl WriteResult {
    const fn success(payloads_written: u64) -> Self {
        Self { payloads_written, points_dropped: 0 }
    }

    const fn failure(points_dropped: u64) -> Self {
        Self { payloads_written: 0, points_dropped }
    }

    const fn new() -> Self {
        Self { payloads_written: 0, points_dropped: 0 }
    }

    fn increment_payloads_written(&mut self) {
        self.payloads_written += 1;
    }

    fn increment_points_dropped(&mut self) {
        self.points_dropped += 1;
    }

    fn increment_points_dropped_by(&mut self, count: u64) {
        self.points_dropped += count;
    }

    pub const fn any_failures(&self) -> bool {
        self.points_dropped != 0
    }

    pub const fn payloads_written(&self) -> u64 {
        self.payloads_written
    }

    pub const fn points_dropped(&self) -> u64 {
        self.points_dropped
    }
}

/// Writes payloads into larger buffers for more efficient network I/O.
///
/// DogStatsD metrics are always newline delimited, which means that multiple metrics can be sent in a single "payload",
/// and then trivially split apart by the remote server. This helps save on the number of system calls required to send
/// the metrics over the network, ultimately making writes more efficient.
///
/// A maximum payload length must be specified, which configures the writer's behavior around how it emits the
/// payloads. When iterating over the payloads, byte slices are returned containing the raw metrics. Each payload will
/// present a slice that contains one or more complete metrics while not exceeding the maximum payload length.
pub(super) struct PayloadWriter {
    max_payload_len: usize,
    payloads_buf: Vec<u8>,
    offsets: Vec<usize>,
    header_buf: Vec<u8>,
    values_buf: Vec<u8>,
    trailer_buf: Vec<u8>,
    with_length_prefix: bool,
    global_tags: Vec<Label>,
}

impl PayloadWriter {
    /// Creates a new `PayloadWriter` with the given maximum payload length.
    ///
    /// When `with_length_prefix` is `true`, the writer will prefix each payload with a 4-byte length prefix. This
    /// prefix does not count towards the payload length.
    pub fn new(max_payload_len: usize, with_length_prefix: bool) -> Self {
        // NOTE: This should also be handled in the builder, but we want to just double check here that we're getting a
        // properly sanitized value.
        assert!(
            u32::try_from(max_payload_len).is_ok(),
            "maximum payload length must be less than 2^32 bytes"
        );
        assert!(
            max_payload_len >= SMALLEST_VALID_PAYLOAD.len(),
            "maximum payload length is too small to allow any metrics to be written (must be {} or greater)",
            SMALLEST_VALID_PAYLOAD.len()
        );

        let mut writer = Self {
            max_payload_len,
            payloads_buf: Vec::new(),
            offsets: Vec::new(),
            header_buf: Vec::new(),
            values_buf: Vec::new(),
            trailer_buf: Vec::new(),
            with_length_prefix,
            global_tags: Vec::new(),
        };

        writer.prepare_for_write();
        writer
    }

    /// Sets the global labels to apply to all metrics.
    pub fn with_global_labels(mut self, global_labels: &[Label]) -> Self {
        self.global_tags = global_labels.to_vec();
        self
    }

    fn last_offset(&self) -> usize {
        self.offsets.last().copied().unwrap_or(0)
    }

    /// Returns the number of bytes in the current payload.
    fn current_payload_len(&self) -> usize {
        // Figure out the last metric's offset, which we use to calculate the current uncommitted length.
        //
        // If there aren't any committed metrics, then the last offset is simply zero.
        let last_offset = self.last_offset();
        let maybe_length_prefix_len = if self.with_length_prefix { 4 } else { 0 };
        self.payloads_buf.len() - last_offset - maybe_length_prefix_len
    }

    /// Returns the number of uncommitted bytes.
    ///
    /// Uncommitted bytes are the bytes that have been written to the buffers but not yet committed to a payload.
    fn uncommitted_len(&self) -> usize {
        self.header_buf.len() + self.values_buf.len() + self.trailer_buf.len()
    }

    fn prepare_for_write(&mut self) {
        if self.with_length_prefix {
            // If we're adding length prefixes, we need to write the length of the payload first.
            //
            // We write a dummy length of zero for now, and then we'll go back and fill it in later.
            self.payloads_buf.extend_from_slice(&[0, 0, 0, 0]);
        }
    }

    /// Finalizes the current payload and starts a new one.
    ///
    /// This handles writing the length prefix if we're using them, tracking the necessary metadata about the current
    /// payload, and preparing the buffer for the next payload.
    ///
    /// If the current payload is empty, this method does nothing.
    fn finalize_current_payload(&mut self) {
        // If the current payload is empty, there's nothing to do.
        let current_payload_len = self.current_payload_len();
        if current_payload_len == 0 {
            return;
        }

        // If we're using length prefixes, we need to go back and fill in the length of the payload.
        if self.with_length_prefix {
            let current_last_offset = self.last_offset();

            // NOTE: We unwrap the conversion here because we know that `self.max_payload_len` is less than 2^32, and we
            // check above that `current_len` is less than or equal to `self.max_payload_len`.
            let current_payload_len_buf = u32::try_from(current_payload_len).unwrap().to_le_bytes();
            self.payloads_buf[current_last_offset..current_last_offset + 4]
                .copy_from_slice(&current_payload_len_buf[..]);
        }

        // Track the offset of the payload we just finalized.
        self.offsets.push(self.payloads_buf.len());

        // Initialize the buffer to start a new payload.
        self.prepare_for_write();
    }

    /// Commits the uncommitted metric to the current payload.
    ///
    /// If the uncommitted metric is larger than the maximum payload length, it will be discarded. If the current
    /// payload cannot fit the uncommitted metric without exceeding the maximum payload length, the current payload will
    /// first be finalized and a new one started before writing the uncommitted metric.
    ///
    /// Returns `true` if the uncommitted metric was successfully committed, or `false` if it was discarded.
    fn commit(&mut self) -> bool {
        // Make sure the uncommitted metric isn't larger than the maximum payload length by itself.
        //
        // If it is, then it has to be discarded regardless of whether or not the current payload is empty.
        let uncommitted_len = self.uncommitted_len();
        if uncommitted_len > self.max_payload_len {
            return false;
        }

        // Check if writing the uncommitted metric to the current payload would cause us to exceed the maximum payload
        // length. If so, then we'll first finalize the current payload and start a new one before continuing.
        let current_payload_len = self.current_payload_len();
        if current_payload_len + uncommitted_len > self.max_payload_len {
            self.finalize_current_payload();
        }

        // Write the uncommitted metric into the payload buffer.
        self.payloads_buf.extend_from_slice(&self.header_buf);
        self.payloads_buf.extend_from_slice(&self.values_buf);
        self.payloads_buf.extend_from_slice(&self.trailer_buf);

        // Clear out the value buffer since we don't want to double write, but leave the header/trailer because we might
        // be reusing it in a multi-value write.
        self.values_buf.clear();

        true
    }

    /// Returns `true` if `len` bytes could be written to the uncommitted metric without exceeding the maximum payload
    /// length.
    fn would_write_exceed_limit(&self, len: usize) -> bool {
        self.uncommitted_len() + len > self.max_payload_len
    }

    fn write_metric_header(&mut self, prefix: Option<&str>, key: &Key) {
        self.header_buf.clear();

        if let Some(prefix) = prefix {
            self.header_buf.extend_from_slice(prefix.as_bytes());
            self.header_buf.push(b'.');
        }

        self.header_buf.extend_from_slice(key.name().as_bytes());
    }

    fn write_metric_trailer(
        &mut self,
        key: &Key,
        metric_type: MetricType,
        maybe_timestamp: Option<u64>,
        maybe_sample_rate: Option<f64>,
    ) {
        self.trailer_buf.clear();

        self.trailer_buf.extend_from_slice(metric_type.as_bytes());

        // Write the sample rate if it's not 1.0, as that is the implied default.
        if let Some(sample_rate) = maybe_sample_rate {
            let mut float_writer = ryu::Buffer::new();
            let sample_rate_str = float_writer.format(sample_rate);

            self.trailer_buf.extend_from_slice(b"|@");
            self.trailer_buf.extend_from_slice(sample_rate_str.as_bytes());
        }

        // Write any tags that are present on the key first, and then additionally write any global tags.
        let tags = key.labels();
        let mut wrote_tag = false;
        for tag in tags.chain(self.global_tags.iter()) {
            // If we haven't written a tag yet, write out the tags prefix first.
            //
            // Otherwise, write a tag separator.
            if wrote_tag {
                self.trailer_buf.push(b',');
            } else {
                self.trailer_buf.extend_from_slice(b"|#");
                wrote_tag = true;
            }

            write_tag(&mut self.trailer_buf, tag);
        }

        if let Some(timestamp) = maybe_timestamp {
            let mut int_writer = itoa::Buffer::new();
            let ts_str = int_writer.format(timestamp);

            self.trailer_buf.extend_from_slice(b"|T");
            self.trailer_buf.extend_from_slice(ts_str.as_bytes());
        }

        // We always add a trailing newline, regardless of whether or not we're using a length prefix.
        self.trailer_buf.push(b'\n');
    }

    fn try_write_single(
        &mut self,
        key: &Key,
        metric_value: MetricValue,
        metric_type: MetricType,
        maybe_timestamp: Option<u64>,
        prefix: Option<&str>,
    ) -> WriteResult {
        // Write our metric header and trailer.
        self.write_metric_header(prefix, key);
        self.write_metric_trailer(key, metric_type, maybe_timestamp, None);

        let mut formatter = MetricValueFormatter::new();
        let metric_value_str = formatter.format(metric_value);

        // Check if the full metric length exceeds the maximum payload length.
        //
        // If it does, we return early.
        if self.would_write_exceed_limit(metric_value_str.len() + 1) {
            return WriteResult::failure(1);
        }

        // Write our value, and then commit the overall metric.
        self.values_buf.clear();
        self.values_buf.push(b':');
        self.values_buf.extend_from_slice(metric_value_str.as_bytes());

        if self.commit() {
            WriteResult::success(1)
        } else {
            WriteResult::failure(1)
        }
    }

    fn try_write_multiple<I>(
        &mut self,
        key: &Key,
        metric_values: I,
        metric_type: MetricType,
        maybe_sample_rate: Option<f64>,
        prefix: Option<&str>,
    ) -> WriteResult
    where
        I: Iterator<Item = MetricValue> + ExactSizeIterator,
    {
        // Write our metric header and trailer.
        self.write_metric_header(prefix, key);
        self.write_metric_trailer(key, metric_type, None, maybe_sample_rate);

        // Check if the full metric length exceeds the maximum payload length. Since we're dealing with multiple values,
        // we check this based on the smallest possible valid value: zero (`:0`).
        //
        // If zero would not fit, then nothing else will either, and we return early.
        if self.would_write_exceed_limit(2) {
            return WriteResult::failure(metric_values.len() as u64);
        }

        let mut result = WriteResult::new();
        let mut formatter = MetricValueFormatter::new();

        // Iterate over all of the values, trying to write each of them.
        //
        // We keep track of the overall size of the payload as we go, and if writing the current value would cause us to
        // exceed the maximum payload length, we commit what we have so far, and then move on. This allows us to
        // basically keep writing until we're done, while letting `commit` figure out where to separate things.
        let mut uncommitted_points = 0;
        for metric_value in metric_values {
            let metric_value_str = formatter.format(metric_value);

            // Do a sanity check to see if writing this value by itself would create a payload that exceeds the maximum
            // payload length, and skip it if so.
            if self.header_buf.len() + metric_value_str.len() + 1 + self.trailer_buf.len()
                > self.max_payload_len
            {
                result.increment_points_dropped();
                continue;
            }

            // See if we can write the value into our current values buffer without exceeding the maximum payload
            // length. If we can't, we'll commit what we have so far before continuing.
            if self.would_write_exceed_limit(metric_value_str.len() + 1) {
                // Try committing to the current payload.
                //
                // Reset the values buffer and our uncommitted points count no matter what.
                if self.commit() {
                    result.increment_payloads_written();
                } else {
                    result.increment_points_dropped_by(uncommitted_points);
                }

                uncommitted_points = 0;
            }

            // Write the value.
            self.values_buf.push(b':');
            self.values_buf.extend_from_slice(metric_value_str.as_bytes());

            uncommitted_points += 1;
        }

        // Commit any remaining uncommitted points.
        if uncommitted_points > 0 {
            if self.commit() {
                result.increment_payloads_written();
            } else {
                result.increment_points_dropped_by(uncommitted_points);
            }
        }

        result
    }

    /// Writes a counter payload.
    pub fn write_counter(
        &mut self,
        key: &Key,
        value: u64,
        timestamp: Option<u64>,
        prefix: Option<&str>,
    ) -> WriteResult {
        self.try_write_single(
            key,
            MetricValue::Integer(value),
            MetricType::Counter,
            timestamp,
            prefix,
        )
    }

    /// Writes a gauge payload.
    pub fn write_gauge(
        &mut self,
        key: &Key,
        value: f64,
        timestamp: Option<u64>,
        prefix: Option<&str>,
    ) -> WriteResult {
        self.try_write_single(
            key,
            MetricValue::FloatingPoint(value),
            MetricType::Gauge,
            timestamp,
            prefix,
        )
    }

    /// Writes a histogram payload.
    pub fn write_histogram<I>(
        &mut self,
        key: &Key,
        values: I,
        maybe_sample_rate: Option<f64>,
        prefix: Option<&str>,
    ) -> WriteResult
    where
        I: IntoIterator<Item = f64>,
        I::IntoIter: ExactSizeIterator,
    {
        let metric_values = values.into_iter().map(MetricValue::FloatingPoint);
        self.try_write_multiple(
            key,
            metric_values,
            MetricType::Histogram,
            maybe_sample_rate,
            prefix,
        )
    }

    /// Writes a distribution payload.
    pub fn write_distribution<I>(
        &mut self,
        key: &Key,
        values: I,
        maybe_sample_rate: Option<f64>,
        prefix: Option<&str>,
    ) -> WriteResult
    where
        I: IntoIterator<Item = f64>,
        I::IntoIter: ExactSizeIterator,
    {
        let metric_values = values.into_iter().map(MetricValue::FloatingPoint);
        self.try_write_multiple(
            key,
            metric_values,
            MetricType::Distribution,
            maybe_sample_rate,
            prefix,
        )
    }

    /// Returns a consuming iterator over all payloads written by this writer.
    ///
    /// The iterator will yield payloads in the order they were written, and the payloads will be cleared from the
    /// writer when the iterator is dropped.
    pub fn payloads(&mut self) -> Payloads<'_> {
        // Finalize the current payload, and clear the intermediate buffers.
        //
        // Between this method, and the logic in `Payloads`, the writer should be completely cleared out after
        // `Payloads` is dropped.
        self.finalize_current_payload();
        self.header_buf.clear();
        self.values_buf.clear();
        self.trailer_buf.clear();

        Payloads::new(&mut self.payloads_buf, &mut self.offsets)
    }
}

/// Iterator over all payloads written by a `PayloadWriter`.
pub struct Payloads<'a> {
    payloads_buf: ConsumingBufferSwap<'a, u8>,
    start: usize,
    offsets: Drain<'a, usize>,
}

impl<'a> Payloads<'a> {
    fn new(payload_buf: &'a mut Vec<u8>, offsets: &'a mut Vec<usize>) -> Self {
        Self {
            payloads_buf: ConsumingBufferSwap::new(payload_buf),
            start: 0,
            offsets: offsets.drain(..),
        }
    }

    /// Returns the number of remaining payloads.
    pub fn len(&self) -> usize {
        self.offsets.len()
    }

    /// Returns the next payload.
    ///
    /// If there are no more payloads, `None` is returned.
    pub fn next_payload(&mut self) -> Option<&[u8]> {
        let offset = self.offsets.next()?;

        let offset_buf = &self.payloads_buf[self.start..offset];
        self.start = offset;

        Some(offset_buf)
    }
}

// Helper type for "pre-pooping our pants".
//
// This type is used when a `Vec<T>` is meant to be drained during an operation, such that the buffer is entirely empty
// after the operation is finished. Since it's safe to "forget" a value and not have its drop logic called, how do we
// ensure that we don't leave the buffer in an indeterminate state without drop logic? We pre-poop our pants.
//
// By swapping out the buffer with an empty one, we ensure that the end state -- the buffer is cleared -- is established
// as soon as we create `ConsumingBufferSwap`. When the drop logic is called, we replace the original buffer, which lets
// use reuse the allocation. At worst, if the drop logic doesn't run, then the buffer is still empty.
//
// https://faultlore.com/blah/everyone-poops/
struct ConsumingBufferSwap<'a, T> {
    source: &'a mut Vec<T>,
    original: Vec<T>,
}

impl<'a, T> ConsumingBufferSwap<'a, T> {
    fn new(source: &'a mut Vec<T>) -> Self {
        let original = std::mem::take(source);
        Self { source, original }
    }
}

impl<'a, T> Drop for ConsumingBufferSwap<'a, T> {
    fn drop(&mut self) {
        // Clear out the original buffer to reset it, and then return it to the source.
        self.original.clear();
        std::mem::swap(self.source, &mut self.original);
    }
}

impl<'a, T> Deref for ConsumingBufferSwap<'a, T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        &self.original
    }
}

impl<'a, T> DerefMut for ConsumingBufferSwap<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.original
    }
}

fn write_tag(buf: &mut Vec<u8>, label: &Label) {
    // If the label value is empty, we treat it as a bare label. This means all we write is something like
    // `label_name`, instead of a more naive form, like `label_name:`.
    buf.extend_from_slice(label.key().as_bytes());
    if label.value().is_empty() {
        return;
    }

    buf.push(b':');
    buf.extend_from_slice(label.value().as_bytes());
}

#[cfg(test)]
mod tests {
    use metrics::{Key, Label};
    use proptest::{collection::vec as arb_vec, prelude::*, prop_oneof, proptest};

    use crate::writer::SMALLEST_VALID_PAYLOAD;
    const SMALLEST_VALID_PAYLOAD_LEN: usize = SMALLEST_VALID_PAYLOAD.len();

    use super::PayloadWriter;

    #[derive(Debug)]
    enum InputMetric {
        Counter(Key, u64, Option<u64>),
        Gauge(Key, f64, Option<u64>),
        Histogram(Key, Vec<f64>),
    }

    fn arb_label() -> impl Strategy<Value = Label> {
        let key_regex = "[a-z]{4,12}";
        let value_regex = "[a-z0-9]{8,16}";

        let bare_tag = key_regex.prop_map(|k| Label::new(k, ""));
        let kv_tag = (key_regex, value_regex).prop_map(|(k, v)| Label::new(k, v));

        prop_oneof![bare_tag, kv_tag,]
    }

    fn arb_key() -> impl Strategy<Value = Key> {
        let name_regex = "[a-zA-Z0-9]{8,32}";
        (name_regex, arb_vec(arb_label(), 0..4))
            .prop_map(|(name, labels)| Key::from_parts(name, labels))
    }

    fn arb_metric() -> impl Strategy<Value = InputMetric> {
        let counter = (arb_key(), any::<u64>(), any::<Option<u64>>())
            .prop_map(|(k, v, ts)| InputMetric::Counter(k, v, ts));
        let gauge = (arb_key(), any::<f64>(), any::<Option<u64>>())
            .prop_map(|(k, v, ts)| InputMetric::Gauge(k, v, ts));
        let histogram = (arb_key(), arb_vec(any::<f64>(), 1..64))
            .prop_map(|(k, v)| InputMetric::Histogram(k, v));

        prop_oneof![counter, gauge, histogram,]
    }

    fn string_from_writer(writer: &mut PayloadWriter) -> String {
        let buf = buf_from_writer(writer);

        // SAFETY: It's a test.
        unsafe { String::from_utf8_unchecked(buf) }
    }

    fn buf_from_writer(writer: &mut PayloadWriter) -> Vec<u8> {
        let mut payloads = writer.payloads();
        let mut buf = Vec::new();
        while let Some(payload) = payloads.next_payload() {
            buf.extend_from_slice(payload);
        }

        buf
    }

    #[test]
    fn counter() {
        // Cases are defined as: metric key, metric value, metric timestamp, expected output.
        let cases = [
            (Key::from("test_counter"), 91919, None, None, &[][..], "test_counter:91919|c\n"),
            (
                Key::from("test_counter"),
                666,
                Some(345678),
                None,
                &[],
                "test_counter:666|c|T345678\n",
            ),
            (
                Key::from_parts("test_counter", &[("bug", "boop")]),
                12345,
                None,
                None,
                &[],
                "test_counter:12345|c|#bug:boop\n",
            ),
            (
                Key::from_parts("test_counter", &[("foo", "bar"), ("baz", "quux")]),
                777,
                Some(234567),
                None,
                &[],
                "test_counter:777|c|#foo:bar,baz:quux|T234567\n",
            ),
            (
                Key::from_parts("test_counter", &[("foo", "bar"), ("baz", "quux")]),
                777,
                Some(234567),
                Some("server1"),
                &[],
                "server1.test_counter:777|c|#foo:bar,baz:quux|T234567\n",
            ),
            (
                Key::from_parts("test_counter", &[("foo", "bar"), ("baz", "quux")]),
                777,
                Some(234567),
                None,
                &[Label::new("gfoo", "bar"), Label::new("gbaz", "quux")][..],
                "test_counter:777|c|#foo:bar,baz:quux,gfoo:bar,gbaz:quux|T234567\n",
            ),
            (
                Key::from_parts("test_counter", &[("foo", "bar"), ("baz", "quux")]),
                777,
                Some(234567),
                Some("server1"),
                &[Label::new("gfoo", "bar"), Label::new("gbaz", "quux")][..],
                "server1.test_counter:777|c|#foo:bar,baz:quux,gfoo:bar,gbaz:quux|T234567\n",
            ),
        ];

        for (key, value, ts, prefix, global_labels, expected) in cases {
            let mut writer = PayloadWriter::new(8192, false).with_global_labels(global_labels);
            let result = writer.write_counter(&key, value, ts, prefix);
            assert_eq!(result.payloads_written(), 1);

            let actual = string_from_writer(&mut writer);
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn gauge() {
        // Cases are defined as: metric key, metric value, metric timestamp, expected output.
        let cases = [
            (Key::from("test_gauge"), 42.0, None, None, &[][..], "test_gauge:42.0|g\n"),
            (
                Key::from("test_gauge"),
                1967.0,
                Some(345678),
                None,
                &[],
                "test_gauge:1967.0|g|T345678\n",
            ),
            (
                Key::from_parts("test_gauge", &[("foo", "bar"), ("baz", "quux")]),
                3.13232,
                None,
                None,
                &[],
                "test_gauge:3.13232|g|#foo:bar,baz:quux\n",
            ),
            (
                Key::from_parts("test_gauge", &[("foo", "bar"), ("baz", "quux")]),
                3.13232,
                Some(234567),
                None,
                &[],
                "test_gauge:3.13232|g|#foo:bar,baz:quux|T234567\n",
            ),
            (
                Key::from_parts("test_gauge", &[("foo", "bar"), ("baz", "quux")]),
                3.13232,
                Some(234567),
                Some("server1"),
                &[],
                "server1.test_gauge:3.13232|g|#foo:bar,baz:quux|T234567\n",
            ),
            (
                Key::from_parts("test_gauge", &[("foo", "bar"), ("baz", "quux")]),
                3.13232,
                Some(234567),
                None,
                &[Label::new("gfoo", "bar"), Label::new("gbaz", "quux")][..],
                "test_gauge:3.13232|g|#foo:bar,baz:quux,gfoo:bar,gbaz:quux|T234567\n",
            ),
            (
                Key::from_parts("test_gauge", &[("foo", "bar"), ("baz", "quux")]),
                3.13232,
                Some(234567),
                Some("server1"),
                &[Label::new("gfoo", "bar"), Label::new("gbaz", "quux")][..],
                "server1.test_gauge:3.13232|g|#foo:bar,baz:quux,gfoo:bar,gbaz:quux|T234567\n",
            ),
        ];

        for (key, value, ts, prefix, global_labels, expected) in cases {
            let mut writer = PayloadWriter::new(8192, false).with_global_labels(global_labels);
            let result = writer.write_gauge(&key, value, ts, prefix);
            assert_eq!(result.payloads_written(), 1);

            let actual = string_from_writer(&mut writer);
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn histogram() {
        // Cases are defined as: metric key, metric values, metric timestamp, expected output.
        let cases = [
            (Key::from("test_histogram"), &[22.22][..], None, &[][..], "test_histogram:22.22|h\n"),
            (
                Key::from_parts("test_histogram", &[("foo", "bar"), ("baz", "quux")]),
                &[88.0][..],
                None,
                &[],
                "test_histogram:88.0|h|#foo:bar,baz:quux\n",
            ),
            (
                Key::from("test_histogram"),
                &[22.22, 33.33, 44.44][..],
                None,
                &[],
                "test_histogram:22.22:33.33:44.44|h\n",
            ),
            (
                Key::from_parts("test_histogram", &[("foo", "bar"), ("baz", "quux")]),
                &[88.0, 66.6, 123.4][..],
                None,
                &[],
                "test_histogram:88.0:66.6:123.4|h|#foo:bar,baz:quux\n",
            ),
            (
                Key::from_parts("test_histogram", &[("foo", "bar"), ("baz", "quux")]),
                &[88.0, 66.6, 123.4][..],
                Some("server1"),
                &[],
                "server1.test_histogram:88.0:66.6:123.4|h|#foo:bar,baz:quux\n",
            ),
            (
                Key::from_parts("test_histogram", &[("foo", "bar"), ("baz", "quux")]),
                &[88.0, 66.6, 123.4][..],
                None,
                &[Label::new("gfoo", "bar"), Label::new("gbaz", "quux")][..],
                "test_histogram:88.0:66.6:123.4|h|#foo:bar,baz:quux,gfoo:bar,gbaz:quux\n",
            ),
            (
                Key::from_parts("test_histogram", &[("foo", "bar"), ("baz", "quux")]),
                &[88.0, 66.6, 123.4][..],
                Some("server1"),
                &[Label::new("gfoo", "bar"), Label::new("gbaz", "quux")][..],
                "server1.test_histogram:88.0:66.6:123.4|h|#foo:bar,baz:quux,gfoo:bar,gbaz:quux\n",
            ),
        ];

        for (key, values, prefix, global_labels, expected) in cases {
            let mut writer = PayloadWriter::new(8192, false).with_global_labels(global_labels);
            let result = writer.write_histogram(&key, values.iter().copied(), None, prefix);
            assert_eq!(result.payloads_written(), 1);

            let actual = string_from_writer(&mut writer);
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn distribution() {
        // Cases are defined as: metric key, metric values, metric timestamp, expected output.
        let cases = [
            (Key::from("test_distribution"), &[22.22][..], None, &[][..], "test_distribution:22.22|d\n"),
            (
                Key::from_parts("test_distribution", &[("foo", "bar"), ("baz", "quux")]),
                &[88.0][..],
                None,
                &[],
                "test_distribution:88.0|d|#foo:bar,baz:quux\n",
            ),
            (
                Key::from("test_distribution"),
                &[22.22, 33.33, 44.44][..],
                None,
                &[],
                "test_distribution:22.22:33.33:44.44|d\n",
            ),
            (
                Key::from_parts("test_distribution", &[("foo", "bar"), ("baz", "quux")]),
                &[88.0, 66.6, 123.4][..],
                None,
                &[],
                "test_distribution:88.0:66.6:123.4|d|#foo:bar,baz:quux\n",
            ),
            (
                Key::from_parts("test_distribution", &[("foo", "bar"), ("baz", "quux")]),
                &[88.0, 66.6, 123.4][..],
                Some("server1"),
                &[],
                "server1.test_distribution:88.0:66.6:123.4|d|#foo:bar,baz:quux\n",
            ),
            (
                Key::from_parts("test_distribution", &[("foo", "bar"), ("baz", "quux")]),
                &[88.0, 66.6, 123.4][..],
                None,
                &[Label::new("gfoo", "bar"), Label::new("gbaz", "quux")][..],
                "test_distribution:88.0:66.6:123.4|d|#foo:bar,baz:quux,gfoo:bar,gbaz:quux\n",
            ),
            (
                Key::from_parts("test_distribution", &[("foo", "bar"), ("baz", "quux")]),
                &[88.0, 66.6, 123.4][..],
                Some("server1"),
                &[Label::new("gfoo", "bar"), Label::new("gbaz", "quux")][..],
                "server1.test_distribution:88.0:66.6:123.4|d|#foo:bar,baz:quux,gfoo:bar,gbaz:quux\n",
            ),
        ];

        for (key, values, prefix, global_labels, expected) in cases {
            let mut writer = PayloadWriter::new(8192, false).with_global_labels(global_labels);
            let result = writer.write_distribution(&key, values.iter().copied(), None, prefix);
            assert_eq!(result.payloads_written(), 1);

            let actual = string_from_writer(&mut writer);
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn length_prefix() {
        let prefixed = |buf: &str| {
            let mut prefixed_buf = Vec::with_capacity(buf.len() + 4);
            prefixed_buf.extend_from_slice(&(buf.len() as u32).to_le_bytes());
            prefixed_buf.extend_from_slice(buf.as_bytes());
            prefixed_buf
        };

        // Cases are defined as: metric key, metric values, metric timestamp, expected output.
        let cases = [
            (Key::from("test_distribution"), &[22.22][..], prefixed("test_distribution:22.22|d\n")),
            (
                Key::from_parts("test_distribution", &[("foo", "bar"), ("baz", "quux")]),
                &[88.0][..],
                prefixed("test_distribution:88.0|d|#foo:bar,baz:quux\n"),
            ),
            (
                Key::from("test_distribution"),
                &[22.22, 33.33, 44.44][..],
                prefixed("test_distribution:22.22:33.33:44.44|d\n"),
            ),
            (
                Key::from_parts("test_distribution", &[("foo", "bar"), ("baz", "quux")]),
                &[88.0, 66.6, 123.4][..],
                prefixed("test_distribution:88.0:66.6:123.4|d|#foo:bar,baz:quux\n"),
            ),
        ];

        for (key, values, expected) in cases {
            let mut writer = PayloadWriter::new(8192, true);
            let result = writer.write_distribution(&key, values.iter().copied(), None, None);
            assert_eq!(result.payloads_written(), 1);

            let actual = buf_from_writer(&mut writer);
            assert_eq!(actual, expected);
        }
    }

    proptest! {
        #[test]
        fn property_test_gauntlet(payload_limit in SMALLEST_VALID_PAYLOAD_LEN..16384usize, inputs in arb_vec(arb_metric(), 1..128)) {
            // TODO: Parameterize reservoir size so we can exercise the sample rate stuff.

            let mut writer = PayloadWriter::new(payload_limit, false);
            let mut total_input_points: u64 = 0;
            let mut payloads_written = 0;
            let mut points_dropped = 0;

            for input in inputs {
                match input {
                    InputMetric::Counter(key, value, ts) => {
                        total_input_points += 1;

                        let result = writer.write_counter(&key, value, ts, None);
                        payloads_written += result.payloads_written();
                        points_dropped += result.points_dropped();
                    },
                    InputMetric::Gauge(key, value, ts) => {
                        total_input_points += 1;

                        let result = writer.write_gauge(&key, value, ts, None);
                        payloads_written += result.payloads_written();
                        points_dropped += result.points_dropped();
                    },
                    InputMetric::Histogram(key, values) => {
                        total_input_points += values.len() as u64;

                        let result = writer.write_histogram(&key, values, None, None);
                        payloads_written += result.payloads_written();
                        points_dropped += result.points_dropped();
                    },
                }
            }

            let mut payloads = writer.payloads();
            let mut payloads_emitted = 0;
            let mut points_emitted: u64 = 0;
            while let Some(payload) = payloads.next_payload() {
                assert!(payload.len() <= payload_limit);

                // Payloads from the writer are meant to be full, sendable chunks that contain only valid metrics. From
                // our perspective, payloads are successfully-written individual metrics, so we take the writer payload,
                // and split it into individual lines, which gives us metric payloads.
                let payload_lines = std::str::from_utf8(payload).unwrap().lines();

                // For each payload line, we increment the number of payloads emitted and we also extract the number of
                // points contained in the metric payload.
                for payload_line in payload_lines {
                    payloads_emitted += 1;

                    // We don't care about the actual values in the payload, just the number of them.
                    //
                    // Split the name/points by taking everything in front of the first pipe character, and then split
                    // by colon, and remove the first element which is the metric name.
                    let num_points = payload_line.split('|')
                        .next().unwrap()
                        .split(':')
                        .skip(1)
                        .count();
                    assert!(num_points > 0);

                    points_emitted += num_points as u64;
                }
            }

            prop_assert_eq!(payloads_written, payloads_emitted);
            prop_assert_eq!(total_input_points, points_dropped + points_emitted);
        }
    }
}
