use std::slice;

/// A compressed set of integers.
///
/// For some workloads, working with a large set of integers can require an outsized amount of
/// memory for numbers that are very similar.  This data structure takes chunks of integers and
/// compresses then by using delta encoding and variable-byte encoding.
///
/// Delta encoding tracks the difference between successive integers: if you have 1000000 and
/// 1000001, the difference between the two is only 1.  Coupled with variable-byte encoding, we can
/// compress those two numbers within 4 bytes, where normally they would require a minimum of 8
/// bytes if they were 32-bit integers, or 16 bytes if they were 64-bit integers.  Over large runs
/// of integers where the delta is relatively small compared to the original value, the compression
/// savings add up quickly.
///
/// The original integers can be decompressed and collected, or can be decompressed on-the-fly
/// while passing them to a given function, allowing callers to observe the integers without
/// allocating the entire size of the decompressed set.
///
/// # Performance
/// As this is a scalar implementation, performance depends heavily on not only the input size, but
/// also the delta between values, as well as whether or not the decompressed values are being
/// collected or used on-the-fly.
///
/// Bigger deltas between values means longer variable-byte sizes which is hard for the CPU to
/// predict.  As the linear benchemarks show, things are much faster when the delta between values
/// is minimal.
///
/// These figures were generated on a 2015 Macbook Pro (Core i7, 2.2GHz base/3.7GHz turbo).
///
/// |                        | compress (1) | decompress (2) | decompress/sum (3) | decompress_with/sum (4) |
/// |------------------------|--------------|----------------|--------------------|-------------------------|
/// | normal, 100 values     |  94 Melem/s  |   76 Melem/s   |     71 Melem/s     |       126 Melem/s       |
/// | normal, 10000 values   |  92 Melem/s  |   85 Melem/s   |    109 Melem/s     |       109 Melem/s       |
/// | normal, 1000000 values |  86 Melem/s  |   79 Melem/s   |     68 Melem/s     |       110 Melem/s       |
/// | linear, 100 values     | 334 Melem/s  |  109 Melem/s   |    110 Melem/s     |       297 Melem/s       |
/// | linear, 10000 values   | 654 Melem/s  |  174 Melem/s   |    374 Melem/s     |       390 Melem/s       |
/// | linear, 1000000 values | 703 Melem/s  |  180 Melem/s   |    132 Melem/s     |       392 Melem/s       |
///
/// The normal values consistent of an approximation of real nanosecond-based timing measurements
/// of a web service.  The linear values are simply sequential integers ranging from 0 to the
/// configured size of the test run.
///
/// Operations:
///  1. simply compress the input set, no decompression
///  2. decompress the entire compressed set into a single vector
///  3. same as #2 but sum all of the original values at the end
///  4. use `decompress_with` to sum the numbers incrementally
#[derive(Debug, Default, Clone)]
pub struct StreamingIntegers {
    inner: Vec<u8>,
    len: usize,
    last: Option<i64>,
}

impl StreamingIntegers {
    /// Creates a new, empty streaming set.
    pub fn new() -> Self {
        Default::default()
    }

    /// Returns the number of elements in the set, also referred to as its 'length'.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if the set contains no elements.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Compresses a slice of integers, and adds them to the set.
    pub fn compress(&mut self, src: &[u64]) {
        let src_len = src.len();
        if src_len == 0 {
            return;
        }

        self.len += src_len;

        // Technically, 64-bit integers can take up to 10 bytes when encoded as variable integers
        // if they're at the maximum size, so we need to properly allocate here.  As we directly
        // operate on a mutable slice of the inner buffer below, we _can't_ afford to lazily
        // allocate or guess at the resulting compression, otherwise we'll get a panic at runtime
        // for bounds checks.
        //
        // TODO: we should try and add some heuristic here, because we're potentially
        // overallocating by a lot when we plan for the worst case scenario
        self.inner.reserve(src_len * 10);

        let mut buf_idx = self.inner.len();
        let buf_cap = self.inner.capacity();
        let mut buf = unsafe {
            let buf_ptr = self.inner.as_mut_ptr();
            slice::from_raw_parts_mut(buf_ptr, buf_cap)
        };

        // If we have no last value, then the very first integer we write is the full value and not
        // a delta value.
        let mut src_idx = 0;
        if self.last.is_none() {
            let first = src[src_idx] as i64;
            self.last = Some(first);

            let zigzag = zigzag_encode(first);
            buf_idx = vbyte_encode(zigzag, &mut buf, buf_idx);

            src_idx += 1;
        }

        // Set up for our actual compression run.
        let mut last = self.last.unwrap();

        while src_idx < src_len {
            let value = src[src_idx] as i64;
            let diff = value - last;
            let zigzag = zigzag_encode(diff);
            buf_idx = vbyte_encode(zigzag, &mut buf, buf_idx);
            last = value;
            src_idx += 1;
        }

        unsafe {
            self.inner.set_len(buf_idx);
        }

        self.last = Some(last);
    }

    /// Decompresses all of the integers written to the set.
    ///
    /// Returns a vector with all of the original values.  For larger sets of integers, this can be
    /// slow due to the allocation required.  Consider [decompress_with] to incrementally iterate
    /// the decompresset set in smaller chunks.
    ///
    /// [decompress_with]: StreamingIntegers::decompress_with
    pub fn decompress(&self) -> Vec<u64> {
        let mut values = Vec::new();

        let mut buf_idx = 0;
        let buf_len = self.inner.len();
        let buf = self.inner.as_slice();

        let mut last = 0;
        while buf_idx < buf_len {
            let (value, new_idx) = vbyte_decode(&buf, buf_idx);
            buf_idx = new_idx;

            let delta = zigzag_decode(value);
            let original = last + delta;
            last = original;

            values.push(original as u64);
        }

        values
    }

    /// Decompresses all of the integers written to the set, invoking `f` for each batch.
    ///
    /// During decompression, values are batched internally until a limit is reached, and then `f`
    /// is called with a reference to the batch.  This leads to minimal allocation to decompress
    /// the entire set, for use cases where the values can be observed incrementally without issue.
    pub fn decompress_with<F>(&self, mut f: F)
    where
        F: FnMut(&[u64]),
    {
        let mut values = Vec::with_capacity(1024);

        let mut buf_idx = 0;
        let buf_len = self.inner.len();
        let buf = self.inner.as_slice();

        let mut last = 0;
        while buf_idx < buf_len {
            let (value, new_idx) = vbyte_decode(&buf, buf_idx);
            buf_idx = new_idx;

            let delta = zigzag_decode(value);
            let original = last + delta;
            last = original;

            values.push(original as u64);
            if values.len() == values.capacity() {
                f(&values);
                values.clear();
            }
        }

        if !values.is_empty() {
            f(&values);
        }
    }
}

#[inline]
fn zigzag_encode(input: i64) -> u64 {
    ((input << 1) ^ (input >> 63)) as u64
}

#[inline]
fn zigzag_decode(input: u64) -> i64 {
    ((input >> 1) as i64) ^ (-((input & 1) as i64))
}

#[inline]
fn vbyte_encode(mut input: u64, buf: &mut [u8], mut buf_idx: usize) -> usize {
    while input >= 128 {
        buf[buf_idx] = 0x80 as u8 | (input as u8 & 0x7F);
        buf_idx += 1;
        input >>= 7;
    }
    buf[buf_idx] = input as u8;
    buf_idx + 1
}

#[inline]
fn vbyte_decode(buf: &[u8], mut buf_idx: usize) -> (u64, usize) {
    let mut tmp = 0;
    let mut factor = 0;
    loop {
        tmp |= u64::from(buf[buf_idx] & 0x7F) << (7 * factor);
        if buf[buf_idx] & 0x80 != 0x80 {
            return (tmp, buf_idx + 1);
        }

        buf_idx += 1;
        factor += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::StreamingIntegers;

    #[test]
    fn test_streaming_integers_new() {
        let si = StreamingIntegers::new();
        let decompressed = si.decompress();
        assert_eq!(decompressed.len(), 0);
    }

    #[test]
    fn test_streaming_integers_single_block() {
        let mut si = StreamingIntegers::new();
        let decompressed = si.decompress();
        assert_eq!(decompressed.len(), 0);

        let values = vec![8, 6, 7, 5, 3, 0, 9];
        si.compress(&values);

        let decompressed = si.decompress();
        assert_eq!(decompressed, values);
    }

    #[test]
    fn test_streaming_integers_multiple_blocks() {
        let mut si = StreamingIntegers::new();
        let decompressed = si.decompress();
        assert_eq!(decompressed.len(), 0);

        let values = vec![8, 6, 7, 5, 3, 0, 9];
        si.compress(&values);

        let values2 = vec![6, 6, 6];
        si.compress(&values2);

        let values3 = vec![];
        si.compress(&values3);

        let values4 = vec![6, 6, 6, 7, 7, 7, 8, 8, 8];
        si.compress(&values4);

        let total = vec![values, values2, values3, values4]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        let decompressed = si.decompress();
        assert_eq!(decompressed, total);
    }

    #[test]
    fn test_streaming_integers_empty_block() {
        let mut si = StreamingIntegers::new();
        let decompressed = si.decompress();
        assert_eq!(decompressed.len(), 0);

        let values = vec![];
        si.compress(&values);

        let decompressed = si.decompress();
        assert_eq!(decompressed.len(), 0);
    }
}
