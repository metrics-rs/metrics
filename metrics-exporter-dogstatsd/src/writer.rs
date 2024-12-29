use metrics::Key;

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

pub(super) struct PayloadWriter {
    max_payload_len: usize,
    buf: Vec<u8>,
    trailer_buf: Vec<u8>,
    offsets: Vec<usize>,
    with_length_prefix: bool,
}

impl PayloadWriter {
    /// Creates a new `PayloadWriter` with the given maximum payload length.
    pub fn new(max_payload_len: usize, with_length_prefix: bool) -> Self {
        // NOTE: This should also be handled in the builder, but we want to just double check here that we're getting a
        // properly sanitized value.
        assert!(
            u32::try_from(max_payload_len).is_ok(),
            "maximum payload length must be less than 2^32 bytes"
        );

        let mut writer = Self {
            max_payload_len,
            buf: Vec::new(),
            trailer_buf: Vec::new(),
            offsets: Vec::new(),
            with_length_prefix,
        };

        writer.prepare_for_write();
        writer
    }

    fn last_offset(&self) -> usize {
        self.offsets.last().copied().unwrap_or(0)
    }

    fn current_len(&self) -> usize {
        // Figure out the last metric's offset, which we use to calculate the current uncommitted length.
        //
        // If there aren't any committed metrics, then the last offset is simply zero.
        let last_offset = self.last_offset();
        let maybe_length_prefix_len = if self.with_length_prefix { 4 } else { 0 };
        self.buf.len() - last_offset - maybe_length_prefix_len
    }

    fn prepare_for_write(&mut self) {
        if self.with_length_prefix {
            // If we're adding length prefixes, we need to write the length of the payload first.
            //
            // We write a dummy length of zero for now, and then we'll go back and fill it in later.
            self.buf.extend_from_slice(&[0, 0, 0, 0]);
        }
    }

    fn commit(&mut self) -> bool {
        let current_last_offset = self.last_offset();
        let current_len = self.current_len();
        if current_len > self.max_payload_len {
            // If the current metric is too long, we need to truncate everything we just wrote to get us back to the end
            // of the last metric, since the previous parts of the buffer are still valid and could be flushed.
            self.buf.truncate(self.last_offset());

            return false;
        }

        // Track the new offset.
        self.offsets.push(self.buf.len());

        // If we're dealing with length-delimited payloads, go back to the beginning of this payload and fill in the
        // length of it.
        if self.with_length_prefix {
            // NOTE: We unwrap the conversion here because we know that `self.max_payload_len` is less than 2^32, and we
            // check above that `current_len` is less than or equal to `self.max_payload_len`.
            let current_len_buf = u32::try_from(current_len).unwrap().to_le_bytes();
            self.buf[current_last_offset..current_last_offset + 4]
                .copy_from_slice(&current_len_buf[..]);
        }

        // Initialize the buffer for the next payload.
        self.prepare_for_write();

        true
    }

    fn write_trailing(&mut self, key: &Key, timestamp: Option<u64>) {
        write_metric_trailer(key, timestamp, &mut self.buf, None);
    }

    /// Writes a counter payload.
    pub fn write_counter(&mut self, key: &Key, value: u64, timestamp: Option<u64>) -> WriteResult {
        let mut int_writer = itoa::Buffer::new();
        let value_str = int_writer.format(value);

        self.buf.extend_from_slice(key.name().as_bytes());
        self.buf.push(b':');
        self.buf.extend_from_slice(value_str.as_bytes());
        self.buf.extend_from_slice(b"|c");

        self.write_trailing(key, timestamp);

        if self.commit() {
            WriteResult::success(1)
        } else {
            WriteResult::failure(1)
        }
    }

    /// Writes a gauge payload.
    pub fn write_gauge(&mut self, key: &Key, value: f64, timestamp: Option<u64>) -> WriteResult {
        let mut float_writer = ryu::Buffer::new();
        let value_str = float_writer.format(value);

        self.buf.extend_from_slice(key.name().as_bytes());
        self.buf.push(b':');
        self.buf.extend_from_slice(value_str.as_bytes());
        self.buf.extend_from_slice(b"|g");

        self.write_trailing(key, timestamp);

        if self.commit() {
            WriteResult::success(1)
        } else {
            WriteResult::failure(1)
        }
    }

    /// Writes a histogram payload.
    pub fn write_histogram<I>(
        &mut self,
        key: &Key,
        values: I,
        maybe_sample_rate: Option<f64>,
    ) -> WriteResult
    where
        I: IntoIterator<Item = f64>,
        I::IntoIter: ExactSizeIterator,
    {
        self.write_hist_dist_inner(key, values, b'h', maybe_sample_rate)
    }

    /// Writes a distribution payload.
    pub fn write_distribution<I>(
        &mut self,
        key: &Key,
        values: I,
        maybe_sample_rate: Option<f64>,
    ) -> WriteResult
    where
        I: IntoIterator<Item = f64>,
        I::IntoIter: ExactSizeIterator,
    {
        self.write_hist_dist_inner(key, values, b'd', maybe_sample_rate)
    }

    fn write_hist_dist_inner<I>(
        &mut self,
        key: &Key,
        values: I,
        metric_type: u8,
        maybe_sample_rate: Option<f64>,
    ) -> WriteResult
    where
        I: IntoIterator<Item = f64>,
        I::IntoIter: ExactSizeIterator,
    {
        let mut float_writer = ryu::Buffer::new();
        let mut result = WriteResult::new();
        let values = values.into_iter();

        // Pre-render our metric trailer, which includes the timestamp and tags.
        //
        // We do this for efficiency reasons, but also to calculate the minimum payload length.
        self.trailer_buf.clear();
        write_metric_trailer(key, None, &mut self.trailer_buf, maybe_sample_rate);

        // Calculate the minimum payload length, which is the key name, the metric trailer, and the metric type
        // substring (`|<metric type>`). This is the minimum amount of space we need to write out the metric without
        // including the value itself.
        //
        // If the minimum payload length exceeds the maximum payload length, we can't write the metric at all, so we
        // return an error.
        let minimum_payload_len = key.name().len() + self.trailer_buf.len() + 2;
        if minimum_payload_len + 2 > self.max_payload_len {
            // The extra two we add above simulates the smallest possible value string, which would be `:0`.
            return WriteResult::failure(values.len() as u64);
        }

        // Iterate over each value, writing it out to the buffer in a streaming fashion.
        //
        // We track a shadow "current length" because we want to make sure we don't write an additional value if it
        // would cause the payload to exceed the maximum payload length... but since we have to write values in the
        // middle of the payload, rather than just at the end... we can't use `current_len()` since we haven't yet
        // written tags, the timestamp, etc.
        let mut needs_name = true;
        let mut current_len = minimum_payload_len;
        for value in values {
            let value_str = float_writer.format(value);

            // Skip the value if it's not even possible to fit it by itself.
            if minimum_payload_len + value_str.len() + 1 > self.max_payload_len {
                result.increment_points_dropped();
                continue;
            }

            // Figure out if we can write the value to the current metric payload.
            //
            // If we can't fit it into the current buffer, then we have to first commit our current buffer.
            if current_len + value_str.len() + 1 > self.max_payload_len {
                // Write the metric type and then the trailer.
                self.buf.push(b'|');
                self.buf.push(metric_type);
                self.buf.extend_from_slice(&self.trailer_buf);

                assert!(self.commit(), "should not fail to commit histogram metric at this stage");

                result.increment_payloads_written();

                // Reset the current length to the minimum payload length, since we're starting a new metric.
                needs_name = true;
                current_len = minimum_payload_len;
            }

            // Write the metric name if it hasn't been written yet.
            if needs_name {
                self.buf.extend_from_slice(key.name().as_bytes());
                needs_name = false;
            }

            // Write the value.
            self.buf.push(b':');
            self.buf.extend_from_slice(value_str.as_bytes());

            // Track the length of the value we just wrote.
            current_len += value_str.len() + 1;
        }

        // If we have any remaining uncommitted values, finalize them and commit.
        if self.current_len() != 0 {
            self.buf.push(b'|');
            self.buf.push(metric_type);
            self.buf.extend_from_slice(&self.trailer_buf);

            assert!(self.commit(), "should not fail to commit histogram metric at this stage");

            result.increment_payloads_written();
        }

        result
    }

    /// Returns a consuming iterator over all payloads written by this writer.
    ///
    /// The iterator will yield payloads in the order they were written, and the payloads will be cleared from the
    /// writer when the iterator is dropped.
    pub fn payloads(&mut self) -> Payloads<'_> {
        Payloads { buf: &mut self.buf, start: 0, offsets: self.offsets.drain(..) }
    }
}

/// Iterator over all payloads written by a `PayloadWriter`.
pub struct Payloads<'a> {
    buf: &'a mut Vec<u8>,
    start: usize,
    offsets: std::vec::Drain<'a, usize>,
}

impl<'a> Payloads<'a> {
    /// Returns the number of remaining payloads.
    pub fn len(&self) -> usize {
        self.offsets.len()
    }

    /// Returns the next payload.
    ///
    /// If there are no more payloads, `None` is returned.
    pub fn next_payload(&mut self) -> Option<&[u8]> {
        let offset = self.offsets.next()?;

        let offset_buf = &self.buf[self.start..offset];
        self.start = offset;

        Some(offset_buf)
    }
}

impl<'a> Drop for Payloads<'a> {
    fn drop(&mut self) {
        self.buf.clear();
    }
}

fn write_metric_trailer(
    key: &Key,
    maybe_timestamp: Option<u64>,
    buf: &mut Vec<u8>,
    maybe_sample_rate: Option<f64>,
) {
    // Write the sample rate if it's not 1.0, as that is the implied default.
    if let Some(sample_rate) = maybe_sample_rate {
        let mut float_writer = ryu::Buffer::new();
        let sample_rate_str = float_writer.format(sample_rate);

        buf.extend_from_slice(b"|@");
        buf.extend_from_slice(sample_rate_str.as_bytes());
    }

    // Write the metric tags first.
    let tags = key.labels();
    let mut wrote_tag = false;
    for tag in tags {
        // If we haven't written a tag yet, write out the tags prefix first.
        //
        // Otherwise, write a tag separator.
        if wrote_tag {
            buf.push(b',');
        } else {
            buf.extend_from_slice(b"|#");
            wrote_tag = true;
        }

        // Write the tag.
        //
        // If the tag value is empty, we treat it as a bare tag, which means we only write something like `tag_name`
        // instead of `tag_name:`.
        buf.extend_from_slice(tag.key().as_bytes());
        if tag.value().is_empty() {
            continue;
        }

        buf.push(b':');
        buf.extend_from_slice(tag.value().as_bytes());
    }

    // Write the timestamp if present.
    if let Some(timestamp) = maybe_timestamp {
        let mut int_writer = itoa::Buffer::new();
        let ts_str = int_writer.format(timestamp);

        buf.extend_from_slice(b"|T");
        buf.extend_from_slice(ts_str.as_bytes());
    }

    // Finally, add the newline.
    buf.push(b'\n');
}

#[cfg(test)]
mod tests {
    use metrics::{Key, Label};
    use proptest::{
        collection::vec as arb_vec,
        prelude::{any, Strategy},
        prop_oneof, proptest,
    };

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
            (Key::from("test_counter"), 91919, None, "test_counter:91919|c\n"),
            (Key::from("test_counter"), 666, Some(345678), "test_counter:666|c|T345678\n"),
            (
                Key::from_parts("test_counter", &[("bug", "boop")]),
                12345,
                None,
                "test_counter:12345|c|#bug:boop\n",
            ),
            (
                Key::from_parts("test_counter", &[("foo", "bar"), ("baz", "quux")]),
                777,
                Some(234567),
                "test_counter:777|c|#foo:bar,baz:quux|T234567\n",
            ),
        ];

        for (key, value, ts, expected) in cases {
            let mut writer = PayloadWriter::new(8192, false);
            let result = writer.write_counter(&key, value, ts);
            assert_eq!(result.payloads_written(), 1);

            let actual = string_from_writer(&mut writer);
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn gauge() {
        // Cases are defined as: metric key, metric value, metric timestamp, expected output.
        let cases = [
            (Key::from("test_gauge"), 42.0, None, "test_gauge:42.0|g\n"),
            (Key::from("test_gauge"), 1967.0, Some(345678), "test_gauge:1967.0|g|T345678\n"),
            (
                Key::from_parts("test_gauge", &[("foo", "bar"), ("baz", "quux")]),
                3.13232,
                None,
                "test_gauge:3.13232|g|#foo:bar,baz:quux\n",
            ),
            (
                Key::from_parts("test_gauge", &[("foo", "bar"), ("baz", "quux")]),
                3.13232,
                Some(234567),
                "test_gauge:3.13232|g|#foo:bar,baz:quux|T234567\n",
            ),
        ];

        for (key, value, ts, expected) in cases {
            let mut writer = PayloadWriter::new(8192, false);
            let result = writer.write_gauge(&key, value, ts);
            assert_eq!(result.payloads_written(), 1);

            let actual = string_from_writer(&mut writer);
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn histogram() {
        // Cases are defined as: metric key, metric values, metric timestamp, expected output.
        let cases = [
            (Key::from("test_histogram"), &[22.22][..], "test_histogram:22.22|h\n"),
            (
                Key::from_parts("test_histogram", &[("foo", "bar"), ("baz", "quux")]),
                &[88.0][..],
                "test_histogram:88.0|h|#foo:bar,baz:quux\n",
            ),
            (
                Key::from("test_histogram"),
                &[22.22, 33.33, 44.44][..],
                "test_histogram:22.22:33.33:44.44|h\n",
            ),
            (
                Key::from_parts("test_histogram", &[("foo", "bar"), ("baz", "quux")]),
                &[88.0, 66.6, 123.4][..],
                "test_histogram:88.0:66.6:123.4|h|#foo:bar,baz:quux\n",
            ),
        ];

        for (key, values, expected) in cases {
            let mut writer = PayloadWriter::new(8192, false);
            let result = writer.write_histogram(&key, values.iter().copied(), None);
            assert_eq!(result.payloads_written(), 1);

            let actual = string_from_writer(&mut writer);
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn distribution() {
        // Cases are defined as: metric key, metric values, metric timestamp, expected output.
        let cases = [
            (Key::from("test_distribution"), &[22.22][..], "test_distribution:22.22|d\n"),
            (
                Key::from_parts("test_distribution", &[("foo", "bar"), ("baz", "quux")]),
                &[88.0][..],
                "test_distribution:88.0|d|#foo:bar,baz:quux\n",
            ),
            (
                Key::from("test_distribution"),
                &[22.22, 33.33, 44.44][..],
                "test_distribution:22.22:33.33:44.44|d\n",
            ),
            (
                Key::from_parts("test_distribution", &[("foo", "bar"), ("baz", "quux")]),
                &[88.0, 66.6, 123.4][..],
                "test_distribution:88.0:66.6:123.4|d|#foo:bar,baz:quux\n",
            ),
        ];

        for (key, values, expected) in cases {
            let mut writer = PayloadWriter::new(8192, false);
            let result = writer.write_distribution(&key, values.iter().copied(), None);
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
            let result = writer.write_distribution(&key, values.iter().copied(), None);
            assert_eq!(result.payloads_written(), 1);

            let actual = buf_from_writer(&mut writer);
            assert_eq!(actual, expected);
        }
    }

    proptest! {
        #[test]
        fn property_test_gauntlet(payload_limit in 0..16384usize, inputs in arb_vec(arb_metric(), 1..128)) {
            // TODO: parameterize reservoir size so we can exercise the sample rate stuff

            let mut writer = PayloadWriter::new(payload_limit, false);
            let mut total_input_points: u64 = 0;
            let mut payloads_written = 0;
            let mut points_dropped = 0;

            for input in inputs {
                match input {
                    InputMetric::Counter(key, value, ts) => {
                        total_input_points += 1;

                        let result = writer.write_counter(&key, value, ts);
                        payloads_written += result.payloads_written();
                        points_dropped += result.points_dropped();
                    },
                    InputMetric::Gauge(key, value, ts) => {
                        total_input_points += 1;

                        let result = writer.write_gauge(&key, value, ts);
                        payloads_written += result.payloads_written();
                        points_dropped += result.points_dropped();
                    },
                    InputMetric::Histogram(key, values) => {
                        total_input_points += values.len() as u64;

                        let result = writer.write_histogram(&key, values, None);
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

            assert_eq!(payloads_written, payloads_emitted);
            assert_eq!(total_input_points, points_dropped + points_emitted);
        }
    }
}
