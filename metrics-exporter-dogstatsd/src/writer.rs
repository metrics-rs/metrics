use std::vec::Drain;

use metrics::{Key, Label};

const SMALLEST_VALID_PAYLOAD: &[u8] = b"a:0|c\n";
const MAX_TAG_LENGTH: usize = 200;
const MAX_METRIC_NAME_LENGTH: usize = 200;

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
    global_labels: Vec<Label>,
    global_tags_buf: Vec<u8>,
    sanitize: bool,
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
            global_labels: Vec::new(),
            global_tags_buf: Vec::new(),
            sanitize: true,
        };

        writer.prepare_for_write();
        writer
    }

    /// Sets the global labels to apply to all metrics.
    #[must_use]
    pub fn with_global_labels(mut self, global_labels: &[Label]) -> Self {
        self.global_labels = global_labels.to_vec();
        self.render_global_tags();
        self
    }

    /// Sets whether metric names and labels are sanitized according to Datadog's rules.
    ///
    /// See [`DogStatsDBuilder::with_sanitization`](crate::DogStatsDBuilder::with_sanitization) for
    /// the rules this applies.
    #[must_use]
    pub fn with_sanitization(mut self, enabled: bool) -> Self {
        self.sanitize = enabled;
        self.render_global_tags();
        self
    }

    /// Renders the global labels into their final on-the-wire form.
    ///
    /// Global tags are identical for every metric, so sanitizing them once here keeps that work off
    /// the write path entirely, where it would otherwise be repeated for every global label on
    /// every metric written.
    fn render_global_tags(&mut self) {
        self.global_tags_buf.clear();

        for label in &self.global_labels {
            let start = self.global_tags_buf.len();
            let needs_separator = start != 0;
            if needs_separator {
                self.global_tags_buf.push(b',');
            }

            if !write_tag(&mut self.global_tags_buf, label, self.sanitize) {
                self.global_tags_buf.truncate(start);
            }
        }
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

        if !self.sanitize {
            if let Some(prefix) = prefix {
                self.header_buf.extend_from_slice(prefix.as_bytes());
                self.header_buf.push(b'.');
            }

            self.header_buf.extend_from_slice(key.name().as_bytes());
            return;
        }

        // Sanitize the prefix and the name as independent elements joined by a `.`, for the same
        // reason tag keys and values are kept separate: a prefix that sanitizes away must not be
        // able to swallow the separator and take the metric name's leading character with it.
        //
        // Only the first element carries the leading-character requirement, since that rule applies
        // to the metric name as a whole.
        let mut written = 0;
        if let Some(prefix) = prefix {
            written =
                write_sanitized_name(&mut self.header_buf, prefix, MAX_METRIC_NAME_LENGTH, true);
            if written != 0 && written < MAX_METRIC_NAME_LENGTH {
                self.header_buf.push(b'.');
                written += 1;
            }
        }

        write_sanitized_name(
            &mut self.header_buf,
            key.name(),
            MAX_METRIC_NAME_LENGTH - written,
            written == 0,
        );
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

        // Write any tags that are present on the key first, and then additionally write any global
        // tags, which were already sanitized and joined when they were configured.
        let mut wrote_tag = false;
        for tag in key.labels() {
            let trailer_len = self.trailer_buf.len();
            // If we haven't written a tag yet, write out the tags prefix first.
            //
            // Otherwise, write a tag separator.
            if wrote_tag {
                self.trailer_buf.push(b',');
            } else {
                self.trailer_buf.extend_from_slice(b"|#");
            }

            if write_tag(&mut self.trailer_buf, tag, self.sanitize) {
                wrote_tag = true;
            } else {
                self.trailer_buf.truncate(trailer_len);
            }
        }

        if !self.global_tags_buf.is_empty() {
            if wrote_tag {
                self.trailer_buf.push(b',');
            } else {
                self.trailer_buf.extend_from_slice(b"|#");
            }

            self.trailer_buf.extend_from_slice(&self.global_tags_buf);
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

#[allow(clippy::doc_link_with_quotes)]
/// Iterator over all payloads written by a `PayloadWriter`.
///
/// The source payload buffer is immediately drained of consumed data during the creation of this iterator (also known as
/// ["pre-pooping our pants"][everyone_poops]). This ensures that the end state - the payload buffer contains only
/// preserved bytes (like length prefixes) - is established immediately.
///
/// [everyone_poops]: https://faultlore.com/blah/everyone-poops/
pub struct Payloads<'a> {
    buf: Vec<u8>,
    start: usize,
    offsets: Drain<'a, usize>,
}

impl<'a> Payloads<'a> {
    fn new(payload_buf: &'a mut Vec<u8>, offsets: &'a mut Vec<usize>) -> Self {
        // When draining payloads, we need to preserve any bytes that come after the last offset.
        // These bytes (like length prefixes) are not part of the current payloads but are needed
        // for the next write operation.
        let drain_size = offsets.last().copied().unwrap_or(0);
        Self {
            buf: payload_buf.drain(0..drain_size).collect(),
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

        let offset_buf = &self.buf[self.start..offset];
        self.start = offset;

        Some(offset_buf)
    }
}

/// Characters allowed verbatim in a tag, beyond alphanumerics and combining marks.
const fn is_allowed_tag_punctuation(c: char) -> bool {
    matches!(c, '_' | '-' | ':' | '.' | '/')
}

/// Characters allowed verbatim in a metric name, beyond ASCII alphanumerics.
const fn is_allowed_name_punctuation(c: char) -> bool {
    matches!(c, '_' | '.')
}

/// Returns `true` if `c` is a Unicode combining mark.
///
/// Combining marks reach us two ways: `char::to_lowercase` can expand one character into a base
/// character plus a mark (`İ` becomes `i` followed by U+0307), and decomposed input carries marks
/// directly (`e` followed by U+0301 for `é`). Marks are not `char::is_alphanumeric`, so treating
/// them as invalid would rewrite them to underscores and mangle otherwise-valid Unicode tags.
///
/// This covers the general-purpose combining blocks rather than every script-specific mark, which
/// would require a full Unicode category table.
const fn is_combining_mark(c: char) -> bool {
    matches!(
        c as u32,
        0x0300..=0x036F     // Combining Diacritical Marks
        | 0x1AB0..=0x1AFF   // Combining Diacritical Marks Extended
        | 0x1DC0..=0x1DFF   // Combining Diacritical Marks Supplement
        | 0x20D0..=0x20F0   // Combining Diacritical Marks for Symbols
        | 0xFE20..=0xFE2F   // Combining Half Marks
    )
}

/// Returns the length of `input` if it already satisfies every tag rule, and `None` if it needs to
/// be rewritten.
///
/// This exists purely so that the common case -- a tag that is already conformant -- can be copied
/// in bulk instead of being reassembled one character at a time.
fn conformant_tag_len(
    input: &str,
    budget: usize,
    require_leading_alphabetic: bool,
) -> Option<usize> {
    let bytes = input.as_bytes();
    if bytes.len() > budget || bytes[0] == b'_' || bytes[bytes.len() - 1] == b'_' {
        return None;
    }

    if require_leading_alphabetic && !bytes[0].is_ascii_lowercase() {
        return None;
    }

    let mut last_was_underscore = false;
    for &byte in bytes {
        let conformant = byte.is_ascii_lowercase()
            || byte.is_ascii_digit()
            || matches!(byte, b'_' | b'-' | b':' | b'.' | b'/');
        if !conformant || (byte == b'_' && last_was_underscore) {
            return None;
        }
        last_was_underscore = byte == b'_';
    }

    Some(bytes.len())
}

/// Writes `input` to `buf` as a sanitized tag element -- either a tag key or a tag value -- and
/// returns the number of characters written.
///
/// At most `budget` characters are written. Invalid characters are replaced with underscores, runs
/// of underscores are collapsed, and leading and trailing underscores are trimmed. Trimming happens
/// per element rather than across the whole tag so that `env` and `Env ` don't map to the two
/// distinct tag names `env` and `env_`.
///
/// When `require_leading_alphabetic` is set -- the case for tag keys, since Datadog requires a tag
/// to start with a letter -- an underscore is *prepended* rather than the offending character being
/// dropped or replaced, so that keys such as `2xx` and `4xx` stay distinct instead of both
/// collapsing to `xx` and silently merging two time series.
fn write_sanitized_tag(
    buf: &mut Vec<u8>,
    input: &str,
    budget: usize,
    require_leading_alphabetic: bool,
) -> usize {
    if budget == 0 || input.is_empty() {
        return 0;
    }

    if let Some(len) = conformant_tag_len(input, budget, require_leading_alphabetic) {
        buf.extend_from_slice(input.as_bytes());
        return len;
    }

    let mut written = 0;
    let mut pending_underscore = false;

    'input: for character in input.chars() {
        // Classify the input character rather than each character `to_lowercase` expands it into,
        // so that a base character carries its combining marks along with it.
        let allowed = character.is_alphanumeric()
            || is_allowed_tag_punctuation(character)
            || is_combining_mark(character);

        // Defer underscores instead of writing them, which collapses runs and trims a trailing run
        // without having to walk back over what we've already written. Nothing is pending while the
        // element is still empty, which trims a leading run.
        if !allowed || character == '_' {
            pending_underscore = written > 0;
            continue;
        }

        if pending_underscore {
            if written == budget {
                break;
            }
            buf.push(b'_');
            written += 1;
            pending_underscore = false;
        }

        if written == 0 && require_leading_alphabetic && !character.is_alphabetic() {
            if written == budget {
                break;
            }
            buf.push(b'_');
            written += 1;
        }

        if character.is_ascii() {
            if written == budget {
                break;
            }
            buf.push(character.to_ascii_lowercase() as u8);
            written += 1;
        } else {
            let mut encoded = [0; 4];
            for lowered in character.to_lowercase() {
                if written == budget {
                    break 'input;
                }
                buf.extend_from_slice(lowered.encode_utf8(&mut encoded).as_bytes());
                written += 1;
            }
        }
    }

    written
}

/// Returns the length of `input` if it already satisfies every metric name rule, and `None` if it
/// needs to be rewritten.
fn conformant_name_len(
    input: &str,
    budget: usize,
    require_leading_alphabetic: bool,
) -> Option<usize> {
    let bytes = input.as_bytes();
    if bytes.len() > budget {
        return None;
    }

    if require_leading_alphabetic && !bytes[0].is_ascii_alphabetic() {
        return None;
    }

    bytes
        .iter()
        .all(|&byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.'))
        .then_some(bytes.len())
}

/// Writes `input` to `buf` as a sanitized metric name element -- either the global prefix or the
/// metric name itself -- and returns the number of characters written.
///
/// Datadog's rules for metric names are not the rules for tags: names are case-sensitive, ASCII
/// only, and may contain only alphanumerics, underscores and periods. So this preserves case, and
/// replaces every other character -- including any non-ASCII character -- one-for-one with an
/// underscore. Replacements are deliberately not collapsed, so that a name cannot lose an
/// underscore the caller wrote on purpose.
fn write_sanitized_name(
    buf: &mut Vec<u8>,
    input: &str,
    budget: usize,
    require_leading_alphabetic: bool,
) -> usize {
    if budget == 0 || input.is_empty() {
        return 0;
    }

    if let Some(len) = conformant_name_len(input, budget, require_leading_alphabetic) {
        buf.extend_from_slice(input.as_bytes());
        return len;
    }

    let mut written = 0;
    for character in input.chars() {
        if written == budget {
            break;
        }

        if written == 0 && require_leading_alphabetic && !character.is_ascii_alphabetic() {
            buf.push(b'_');
            written += 1;
            if written == budget {
                break;
            }
        }

        let byte = if character.is_ascii_alphanumeric() || is_allowed_name_punctuation(character) {
            character as u8
        } else {
            b'_'
        };
        buf.push(byte);
        written += 1;
    }

    written
}

/// Writes `label` to `buf` as a tag, returning `false` if no tag was written at all.
///
/// The caller relies on the return value to roll back the tag prefix or separator it wrote
/// speculatively, so a label that sanitizes away leaves no trace in the payload.
fn write_tag(buf: &mut Vec<u8>, label: &Label, sanitize: bool) -> bool {
    let start = buf.len();

    if !sanitize {
        // If the label value is empty, we treat it as a bare label. This means all we write is
        // something like `label_name`, instead of a more naive form, like `label_name:`.
        buf.extend_from_slice(label.key().as_bytes());
        if !label.value().is_empty() {
            buf.push(b':');
            buf.extend_from_slice(label.value().as_bytes());
        }
        return buf.len() != start;
    }

    // Sanitize the key and the value as independent elements. Running them through a single shared
    // character stream would let a key that sanitizes away promote the value into the tag-name
    // position, let a value that sanitizes away leave a dangling `:`, and let a long key silently
    // consume its own value.
    let key_len = write_sanitized_tag(buf, label.key(), MAX_TAG_LENGTH, true);
    if key_len == 0 {
        // With no tag name there is no tag to write. Emitting the value on its own would put it in
        // the tag-name namespace, turning `("2", "us-east-1")` into a tag named `us-east-1`.
        buf.truncate(start);
        return false;
    }

    // A value is only worth writing if the separator and at least one character of value fit.
    // Otherwise the key is emitted as a bare tag, rather than one ending in a dangling `:`.
    let remaining = MAX_TAG_LENGTH - key_len;
    if !label.value().is_empty() && remaining >= 2 {
        buf.push(b':');
        if write_sanitized_tag(buf, label.value(), remaining - 1, false) == 0 {
            buf.pop();
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use metrics::{Key, Label};
    use proptest::{collection::vec as arb_vec, prelude::*, prop_oneof, proptest};

    use crate::writer::SMALLEST_VALID_PAYLOAD;
    const SMALLEST_VALID_PAYLOAD_LEN: usize = SMALLEST_VALID_PAYLOAD.len();

    use super::{write_tag, PayloadWriter, MAX_METRIC_NAME_LENGTH, MAX_TAG_LENGTH};

    #[derive(Debug)]
    enum InputMetric {
        Counter(Key, u64, Option<u64>),
        Gauge(Key, f64, Option<u64>),
        Histogram(Key, Vec<f64>),
    }

    fn arb_label() -> impl Strategy<Value = Label> {
        let key_regex = "[a-z]{4,12}";
        let value_regex = "[a-z0-9]{8,16}";

        // Labels that are already conformant, which take the bulk-copy path through the sanitizer.
        let bare_tag = key_regex.prop_map(|k| Label::new(k, ""));
        let kv_tag = (key_regex, value_regex).prop_map(|(k, v)| Label::new(k, v));

        // Labels the sanitizer has to rewrite, so that the payload length accounting is exercised
        // against tags that shrink, that grow by a prepended character, or that vanish entirely.
        // Without these, sanitization is a strict no-op under the gauntlet.
        let dirty_regex = r"[a-zA-Z0-9_|:, -]{0,16}";
        let dirty_tag = (dirty_regex, dirty_regex).prop_map(|(k, v)| Label::new(k, v));
        let long_tag = ("[a-zA-Z]{190,210}", value_regex).prop_map(|(k, v)| Label::new(k, v));
        let empty_after_sanitizing =
            r"[|,: ]{0,4}".prop_map(|k: String| Label::new(k, "dropped".to_string()));
        let multibyte_tag = (key_regex, r"[\x{00c0}-\x{00ff}\x{3040}-\x{30ff}]{1,210}")
            .prop_map(|(k, v)| Label::new(k, v));

        prop_oneof![bare_tag, kv_tag, dirty_tag, long_tag, empty_after_sanitizing, multibyte_tag,]
    }

    fn arb_key() -> impl Strategy<Value = Key> {
        // Names cover both the bulk-copy path and names that have to be rewritten, including ones
        // needing a prepended leading character and ones long enough to be truncated.
        let name_regex = "[a-zA-Z0-9]{8,32}";
        let dirty_name_regex = r"[a-zA-Z0-9_.|:\n\x{00e9}]{1,32}";
        let long_name_regex = "[a-zA-Z]{190,210}";
        let name = prop_oneof![name_regex, dirty_name_regex, long_name_regex];

        (name, arb_vec(arb_label(), 0..4)).prop_map(|(name, labels)| Key::from_parts(name, labels))
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
                Some(345_678),
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
                Some(234_567),
                None,
                &[],
                "test_counter:777|c|#foo:bar,baz:quux|T234567\n",
            ),
            (
                Key::from_parts("test_counter", &[("foo", "bar"), ("baz", "quux")]),
                777,
                Some(234_567),
                Some("server1"),
                &[],
                "server1.test_counter:777|c|#foo:bar,baz:quux|T234567\n",
            ),
            (
                Key::from_parts("test_counter", &[("foo", "bar"), ("baz", "quux")]),
                777,
                Some(234_567),
                None,
                &[Label::new("gfoo", "bar"), Label::new("gbaz", "quux")][..],
                "test_counter:777|c|#foo:bar,baz:quux,gfoo:bar,gbaz:quux|T234567\n",
            ),
            (
                Key::from_parts("test_counter", &[("foo", "bar"), ("baz", "quux")]),
                777,
                Some(234_567),
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
    fn sanitizes_labels_by_default() {
        let key = Key::from_parts("test_counter", &[("tag", "Foo|Bar")]);
        let mut writer = PayloadWriter::new(8192, false);

        let result = writer.write_counter(&key, 1, None, None);
        assert_eq!(result.payloads_written(), 1);

        let actual = string_from_writer(&mut writer);
        assert_eq!(actual, "test_counter:1|c|#tag:foo_bar\n");
    }

    #[test]
    fn sanitizes_labels_using_datadog_rules() {
        let key =
            Key::from_parts("test_counter", &[("_Env NAME", "Staging|East___"), ("RÉGION", "気")]);
        let mut writer = PayloadWriter::new(8192, false);

        let result = writer.write_counter(&key, 1, None, None);
        assert_eq!(result.payloads_written(), 1);

        let actual = string_from_writer(&mut writer);
        assert_eq!(actual, "test_counter:1|c|#env_name:staging_east,région:気\n");
    }

    #[test]
    fn sanitizes_label_keys_and_values_independently() {
        // Cases are defined as: label key, label value, expected tag.
        //
        // Each of these regresses a way that sanitizing the key, the separator and the value as one
        // fused character stream corrupted the tag.
        let cases = [
            // A key that needs a leading character must not let the value slide into the tag-name
            // position, which is what dropping the invalid character used to cause.
            ("123", "foo", "_123:foo"),
            ("2", "us-east-1", "_2:us-east-1"),
            // A value that sanitizes away leaves a bare tag, not a dangling separator.
            ("env", "___", "env"),
            ("env", "|||", "env"),
            ("env", " ", "env"),
            ("env", "\u{1F600}", "env"),
            // Underscores are trimmed per element, so a key's trailing run cannot survive by
            // sitting next to the separator.
            ("Env ", "prod", "env:prod"),
            ("Env", "prod", "env:prod"),
            ("env", " prod", "env:prod"),
            ("env_", "_prod", "env:prod"),
        ];

        for (label_key, label_value, expected) in cases {
            let mut buf = Vec::new();
            let label = Label::new(label_key, label_value);
            assert!(write_tag(&mut buf, &label, true));
            assert_eq!(
                String::from_utf8(buf).unwrap(),
                expected,
                "key={label_key:?} value={label_value:?}"
            );
        }
    }

    #[test]
    fn sanitizing_label_keys_preserves_distinctness() {
        // Stripping a leading invalid character instead of prepending to it used to collapse all of
        // these onto the single tag name `xx`, silently merging three distinct time series.
        let tags = ["2xx", "4xx", "5xx", "xx"].map(|label_key| {
            let mut buf = Vec::new();
            assert!(write_tag(&mut buf, &Label::new(label_key, "yes"), true));
            String::from_utf8(buf).unwrap()
        });

        assert_eq!(tags, ["_2xx:yes", "_4xx:yes", "_5xx:yes", "xx:yes"]);
    }

    #[test]
    fn sanitizing_labels_preserves_combining_marks() {
        // `char::to_lowercase` expands `İ` into `i` plus a combining dot, and decomposed input
        // carries its marks directly. Neither may be rewritten to an underscore.
        let cases = [
            ("tag", "\u{130}stanbul", "tag:i\u{307}stanbul"),
            ("tag", "e\u{301}clair", "tag:e\u{301}clair"),
        ];

        for (label_key, label_value, expected) in cases {
            let mut buf = Vec::new();
            assert!(write_tag(&mut buf, &Label::new(label_key, label_value), true));
            assert_eq!(String::from_utf8(buf).unwrap(), expected, "value={label_value:?}");
        }
    }

    #[test]
    fn limits_sanitized_labels_to_two_hundred_characters() {
        let label = Label::new("tag", "x".repeat(250));
        let mut buf = Vec::new();

        assert!(write_tag(&mut buf, &label, true));

        let tag = String::from_utf8(buf).unwrap();
        assert_eq!(tag.chars().count(), 200);
        assert!(tag.starts_with("tag:"));
    }

    #[test]
    fn limits_sanitized_labels_by_characters_not_bytes() {
        // The limit is a character limit, so a multi-byte tag is legitimately longer than 200 bytes
        // on the wire. This pins that down rather than leaving it to an ASCII-only test.
        let label = Label::new("tag", "日".repeat(250));
        let mut buf = Vec::new();

        assert!(write_tag(&mut buf, &label, true));

        let tag = String::from_utf8(buf).unwrap();
        assert_eq!(tag.chars().count(), 200);
        assert_eq!(tag.len(), 4 + (196 * 3));
    }

    #[test]
    fn omits_value_that_does_not_fit_rather_than_truncating_to_separator() {
        // A key long enough to crowd out its own value yields a bare tag. It must not end in a
        // dangling `:`, and the value must not be silently half-written.
        for key_len in [198, 199, 205] {
            let label = Label::new("k".repeat(key_len), "value");
            let mut buf = Vec::new();

            assert!(write_tag(&mut buf, &label, true));

            let tag = String::from_utf8(buf).unwrap();
            assert!(
                !tag.ends_with(':'),
                "key_len={key_len} produced a dangling separator: {tag:?}"
            );
            assert!(tag.chars().count() <= MAX_TAG_LENGTH, "key_len={key_len} exceeded the limit");
        }
    }

    #[test]
    fn omits_labels_that_are_empty_after_sanitizing() {
        // Only a key with nothing left to salvage drops the tag; the value is never promoted into
        // the key's place to fill the gap.
        for (label_key, label_value) in [("", "___"), ("", "bar"), ("|||", "bar")] {
            let key = Key::from_parts("test_counter", &[(label_key, label_value)]);
            let mut writer = PayloadWriter::new(8192, false);

            let result = writer.write_counter(&key, 1, None, None);
            assert_eq!(result.payloads_written(), 1);

            let actual = string_from_writer(&mut writer);
            assert_eq!(actual, "test_counter:1|c\n", "key={label_key:?} value={label_value:?}");
        }
    }

    #[test]
    fn sanitizes_metric_names_using_datadog_rules() {
        // Cases are defined as: global prefix, metric name, expected payload.
        let cases = [
            // Metric names are case-sensitive in Datadog, unlike tags, so case is preserved.
            (None, "MyApp.Requests", "MyApp.Requests:1|c\n"),
            // The invalid-payload failure mode that motivated sanitizing in the first place.
            (None, "foo|bar", "foo_bar:1|c\n"),
            // A newline in a name would otherwise inject a second, caller-chosen metric line.
            (None, "a\nevil.metric:99|c", "a_evil.metric_99_c:1|c\n"),
            // Unicode is not supported in names, and each character is replaced one-for-one.
            (None, "métrique", "m_trique:1|c\n"),
            // A name must start with a letter, and prepending keeps otherwise-distinct names apart.
            (None, "9lives", "_9lives:1|c\n"),
            (None, "8lives", "_8lives:1|c\n"),
            // The prefix and the name are sanitized as separate elements joined by a `.`.
            (Some("my|prefix"), "some|name", "my_prefix.some_name:1|c\n"),
            (Some("9prefix"), "name", "_9prefix.name:1|c\n"),
        ];

        for (prefix, name, expected) in cases {
            let key = Key::from(name);
            let mut writer = PayloadWriter::new(8192, false);

            let result = writer.write_counter(&key, 1, None, prefix);
            assert_eq!(result.payloads_written(), 1, "name={name:?}");

            let actual = string_from_writer(&mut writer);
            assert_eq!(actual, expected, "prefix={prefix:?} name={name:?}");
        }
    }

    #[test]
    fn limits_sanitized_metric_names_to_two_hundred_characters() {
        let key = Key::from("n".repeat(250));
        let mut writer = PayloadWriter::new(8192, false);

        let result = writer.write_counter(&key, 1, None, Some("p".repeat(150).as_str()));
        assert_eq!(result.payloads_written(), 1);

        let actual = string_from_writer(&mut writer);
        let name = actual.split(':').next().unwrap();
        assert_eq!(name.chars().count(), MAX_METRIC_NAME_LENGTH);
    }

    #[test]
    fn sanitizes_global_labels() {
        let global_labels = [Label::new("Global Tag", "Value|X"), Label::new("|||", "dropped")];
        let key = Key::from_parts("test_counter", &[("a", "B")]);
        let mut writer = PayloadWriter::new(8192, false).with_global_labels(&global_labels);

        let result = writer.write_counter(&key, 1, None, None);
        assert_eq!(result.payloads_written(), 1);

        let actual = string_from_writer(&mut writer);
        assert_eq!(actual, "test_counter:1|c|#a:b,global_tag:value_x\n");
    }

    #[test]
    fn sanitizes_global_labels_regardless_of_builder_order() {
        // The global labels are rendered eagerly, so both setters have to re-render rather than
        // whichever one happens to be called last winning.
        let global_labels = [Label::new("Global Tag", "Value|X")];
        let key = Key::from("test_counter");

        let mut sanitize_last = PayloadWriter::new(8192, false)
            .with_global_labels(&global_labels)
            .with_sanitization(false);
        let mut sanitize_first = PayloadWriter::new(8192, false)
            .with_sanitization(false)
            .with_global_labels(&global_labels);

        let expected = "test_counter:1|c|#Global Tag:Value|X\n";
        for writer in [&mut sanitize_last, &mut sanitize_first] {
            let result = writer.write_counter(&key, 1, None, None);
            assert_eq!(result.payloads_written(), 1);
            assert_eq!(string_from_writer(writer), expected);
        }
    }

    #[test]
    fn allows_sanitization_to_be_disabled() {
        let key = Key::from_parts("test|counter", &[("tag", "Foo|Bar")]);
        let mut writer = PayloadWriter::new(8192, false).with_sanitization(false);

        let result = writer.write_counter(&key, 1, None, None);
        assert_eq!(result.payloads_written(), 1);

        let actual = string_from_writer(&mut writer);
        assert_eq!(actual, "test|counter:1|c|#tag:Foo|Bar\n");
    }

    #[test]
    fn omits_tags_prefix_when_unsanitized_labels_are_empty() {
        // Even with sanitization disabled, an entirely empty label has nothing to write, and the
        // speculative `|#` has to be rolled back rather than left dangling on the payload.
        let key = Key::from_parts("test_counter", &[("", "")]);
        let mut writer = PayloadWriter::new(8192, false).with_sanitization(false);

        let result = writer.write_counter(&key, 1, None, None);
        assert_eq!(result.payloads_written(), 1);

        let actual = string_from_writer(&mut writer);
        assert_eq!(actual, "test_counter:1|c\n");
    }

    #[test]
    fn gauge() {
        // Cases are defined as: metric key, metric value, metric timestamp, expected output.
        let cases = [
            (Key::from("test_gauge"), 42.0, None, None, &[][..], "test_gauge:42.0|g\n"),
            (
                Key::from("test_gauge"),
                1967.0,
                Some(345_678),
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
                Some(234_567),
                None,
                &[],
                "test_gauge:3.13232|g|#foo:bar,baz:quux|T234567\n",
            ),
            (
                Key::from_parts("test_gauge", &[("foo", "bar"), ("baz", "quux")]),
                3.13232,
                Some(234_567),
                Some("server1"),
                &[],
                "server1.test_gauge:3.13232|g|#foo:bar,baz:quux|T234567\n",
            ),
            (
                Key::from_parts("test_gauge", &[("foo", "bar"), ("baz", "quux")]),
                3.13232,
                Some(234_567),
                None,
                &[Label::new("gfoo", "bar"), Label::new("gbaz", "quux")][..],
                "test_gauge:3.13232|g|#foo:bar,baz:quux,gfoo:bar,gbaz:quux|T234567\n",
            ),
            (
                Key::from_parts("test_gauge", &[("foo", "bar"), ("baz", "quux")]),
                3.13232,
                Some(234_567),
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

            // Write another payload (as a sanity check for previous panic bug)
            let result = writer.write_distribution(&key, values.iter().copied(), None, None);
            assert_eq!(result.payloads_written(), 1);
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
