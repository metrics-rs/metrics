//! Native histogram support for Prometheus.
//!
//! This module implements Prometheus native histograms, which use exponential buckets
//! to efficiently represent histogram data without requiring predefined bucket boundaries.

use std::collections::btree_map::Entry;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};

/// IEEE 754 frexp implementation matching Go's math.Frexp behavior.
/// Returns (mantissa, exponent) such that f = mantissa × 2^exponent,
/// where mantissa is in the range [0.5, 1) for finite non-zero f.
fn frexp(f: f64) -> (f64, i32) {
    if f == 0.0 || !f.is_finite() {
        return (f, 0);
    }

    let bits = f.to_bits();
    let sign_bit = bits & (1u64 << 63);
    let mut exp_bits = (bits >> 52) & 0x7FF;
    let mut mantissa_bits = bits & 0x000F_FFFF_FFFF_FFFF;

    if exp_bits == 0 {
        // Subnormal number - normalize it
        let shift = mantissa_bits.leading_zeros() - 12; // 12 = 64 - 52
        mantissa_bits <<= shift + 1;
        mantissa_bits &= 0x000F_FFFF_FFFF_FFFF; // Clear the implicit 1 bit
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
        {
            exp_bits = (1022_i32 - shift as i32) as u64; // Adjust exponent
        }
        let mantissa_f64_bits = sign_bit | (1022u64 << 52) | mantissa_bits;
        let mantissa = f64::from_bits(mantissa_f64_bits);
        #[allow(clippy::cast_possible_truncation)]
        let exponent = exp_bits as i32 - 1022;
        (mantissa, exponent)
    } else {
        // Normal number
        #[allow(clippy::cast_possible_truncation)]
        let exponent = exp_bits as i32 - 1023; // Remove IEEE bias, then subtract 1 more for [0.5,1) range
        let mantissa_f64_bits = sign_bit | (1022u64 << 52) | mantissa_bits; // Set exponent to 1022 (bias-1) for [0.5,1) range
        let mantissa = f64::from_bits(mantissa_f64_bits);
        (mantissa, exponent + 1)
    }
}

/// Schema constants
const MIN_SCHEMA: i32 = -4;
const MAX_SCHEMA: i32 = 8;

/// Native histogram bounds for different schemas (from Go implementation)
#[allow(clippy::unreadable_literal)]
const NATIVE_HISTOGRAM_BOUNDS: &[&[f64]] = &[
    // Schema "0":
    &[0.5],
    // Schema 1:
    &[0.5, 0.7071067811865475],
    // Schema 2:
    &[0.5, 0.5946035575013605, 0.7071067811865475, 0.8408964152537144],
    // Schema 3:
    &[
        0.5,
        0.5452538663326288,
        0.5946035575013605,
        0.6484197773255048,
        0.7071067811865475,
        0.7711054127039704,
        0.8408964152537144,
        0.9170040432046711,
    ],
    // Schema 4:
    &[
        0.5,
        0.5221368912137069,
        0.5452538663326288,
        0.5693943173783458,
        0.5946035575013605,
        0.620928906036742,
        0.6484197773255048,
        0.6771277734684463,
        0.7071067811865475,
        0.7384130729697496,
        0.7711054127039704,
        0.805245165974627,
        0.8408964152537144,
        0.8781260801866495,
        0.9170040432046711,
        0.9576032806985735,
    ],
    // Schema 5:
    &[
        0.5,
        0.5109485743270583,
        0.5221368912137069,
        0.5335702003384117,
        0.5452538663326288,
        0.5571933712979462,
        0.5693943173783458,
        0.5818624293887887,
        0.5946035575013605,
        0.6076236799902344,
        0.620928906036742,
        0.6345254785958666,
        0.6484197773255048,
        0.6626183215798706,
        0.6771277734684463,
        0.6919549409819159,
        0.7071067811865475,
        0.7225904034885232,
        0.7384130729697496,
        0.7545822137967112,
        0.7711054127039704,
        0.7879904225539431,
        0.805245165974627,
        0.8228777390769823,
        0.8408964152537144,
        0.8593096490612387,
        0.8781260801866495,
        0.8973545375015533,
        0.9170040432046711,
        0.9370838170551498,
        0.9576032806985735,
        0.9785720620876999,
    ],
    // Schema 6:
    &[
        0.5,
        0.5054446430258502,
        0.5109485743270583,
        0.5165124395106142,
        0.5221368912137069,
        0.5278225891802786,
        0.5335702003384117,
        0.5393803988785598,
        0.5452538663326288,
        0.5511912916539204,
        0.5571933712979462,
        0.5632608093041209,
        0.5693943173783458,
        0.5755946149764913,
        0.5818624293887887,
        0.5881984958251406,
        0.5946035575013605,
        0.6010783657263515,
        0.6076236799902344,
        0.6142402680534349,
        0.620928906036742,
        0.6276903785123455,
        0.6345254785958666,
        0.6414350080393891,
        0.6484197773255048,
        0.6554806057623822,
        0.6626183215798706,
        0.6698337620266515,
        0.6771277734684463,
        0.6845012114872953,
        0.6919549409819159,
        0.6994898362691555,
        0.7071067811865475,
        0.7148066691959849,
        0.7225904034885232,
        0.7304588970903234,
        0.7384130729697496,
        0.7464538641456323,
        0.7545822137967112,
        0.762799075372269,
        0.7711054127039704,
        0.7795022001189185,
        0.7879904225539431,
        0.7965710756711334,
        0.805245165974627,
        0.8140137109286738,
        0.8228777390769823,
        0.8318382901633681,
        0.8408964152537144,
        0.8500531768592616,
        0.8593096490612387,
        0.8686669176368529,
        0.8781260801866495,
        0.8876882462632604,
        0.8973545375015533,
        0.9071260877501991,
        0.9170040432046711,
        0.9269895625416926,
        0.9370838170551498,
        0.9472879907934827,
        0.9576032806985735,
        0.9680308967461471,
        0.9785720620876999,
        0.9892280131939752,
    ],
    // Schema 7:
    &[
        0.5,
        0.5027149505564014,
        0.5054446430258502,
        0.5081891574554764,
        0.5109485743270583,
        0.5137229745593818,
        0.5165124395106142,
        0.5193170509806894,
        0.5221368912137069,
        0.5249720429003435,
        0.5278225891802786,
        0.5306886136446309,
        0.5335702003384117,
        0.5364674337629877,
        0.5393803988785598,
        0.5423091811066545,
        0.5452538663326288,
        0.5482145409081883,
        0.5511912916539204,
        0.5541842058618393,
        0.5571933712979462,
        0.5602188762048033,
        0.5632608093041209,
        0.5663192597993595,
        0.5693943173783458,
        0.572486072215902,
        0.5755946149764913,
        0.5787200368168754,
        0.5818624293887887,
        0.585021884841625,
        0.5881984958251406,
        0.5913923554921704,
        0.5946035575013605,
        0.5978321960199137,
        0.6010783657263515,
        0.6043421618132907,
        0.6076236799902344,
        0.6109230164863786,
        0.6142402680534349,
        0.6175755319684665,
        0.620928906036742,
        0.6243004885946023,
        0.6276903785123455,
        0.6310986751971253,
        0.6345254785958666,
        0.637970889198196,
        0.6414350080393891,
        0.6449179367033329,
        0.6484197773255048,
        0.6519406325959679,
        0.6554806057623822,
        0.659039800633032,
        0.6626183215798706,
        0.6662162735415805,
        0.6698337620266515,
        0.6734708931164728,
        0.6771277734684463,
        0.6808045103191123,
        0.6845012114872953,
        0.688217985377265,
        0.6919549409819159,
        0.6957121878859629,
        0.6994898362691555,
        0.7032879969095076,
        0.7071067811865475,
        0.7109463010845827,
        0.7148066691959849,
        0.718687998724491,
        0.7225904034885232,
        0.7265139979245261,
        0.7304588970903234,
        0.7344252166684908,
        0.7384130729697496,
        0.7424225829363761,
        0.7464538641456323,
        0.7505070348132126,
        0.7545822137967112,
        0.7586795205991071,
        0.762799075372269,
        0.7669409989204777,
        0.7711054127039704,
        0.7752924388424999,
        0.7795022001189185,
        0.7837348199827764,
        0.7879904225539431,
        0.7922691326262467,
        0.7965710756711334,
        0.8008963778413465,
        0.805245165974627,
        0.8096175675974316,
        0.8140137109286738,
        0.8184337248834821,
        0.8228777390769823,
        0.8273458838280969,
        0.8318382901633681,
        0.8363550898207981,
        0.8408964152537144,
        0.8454623996346523,
        0.8500531768592616,
        0.8546688815502312,
        0.8593096490612387,
        0.8639756154809185,
        0.8686669176368529,
        0.8733836930995842,
        0.8781260801866495,
        0.8828942179666361,
        0.8876882462632604,
        0.8925083056594671,
        0.8973545375015533,
        0.9022270839033115,
        0.9071260877501991,
        0.9120516927035263,
        0.9170040432046711,
        0.9219832844793128,
        0.9269895625416926,
        0.9320230241988943,
        0.9370838170551498,
        0.9421720895161669,
        0.9472879907934827,
        0.9524316709088368,
        0.9576032806985735,
        0.9628029718180622,
        0.9680308967461471,
        0.9732872087896164,
        0.9785720620876999,
        0.9838856116165875,
        0.9892280131939752,
        0.9945994234836328,
    ],
    // Schema 8:
    &[
        0.5,
        0.5013556375251013,
        0.5027149505564014,
        0.5040779490592088,
        0.5054446430258502,
        0.5068150424757447,
        0.5081891574554764,
        0.509566998038869,
        0.5109485743270583,
        0.5123338964485679,
        0.5137229745593818,
        0.5151158188430205,
        0.5165124395106142,
        0.5179128468009786,
        0.5193170509806894,
        0.520725062344158,
        0.5221368912137069,
        0.5235525479396449,
        0.5249720429003435,
        0.526395386502313,
        0.5278225891802786,
        0.5292536613972564,
        0.5306886136446309,
        0.5321274564422321,
        0.5335702003384117,
        0.5350168559101208,
        0.5364674337629877,
        0.5379219445313954,
        0.5393803988785598,
        0.5408428074966075,
        0.5423091811066545,
        0.5437795304588847,
        0.5452538663326288,
        0.5467321995364429,
        0.5482145409081883,
        0.549700901315111,
        0.5511912916539204,
        0.5526857228508706,
        0.5541842058618393,
        0.5556867516724088,
        0.5571933712979462,
        0.5587040757836845,
        0.5602188762048033,
        0.5617377836665098,
        0.5632608093041209,
        0.564787964283144,
        0.5663192597993595,
        0.5678547070789026,
        0.5693943173783458,
        0.5709381019847808,
        0.572486072215902,
        0.5740382394200894,
        0.5755946149764913,
        0.5771552102951081,
        0.5787200368168754,
        0.5802891060137493,
        0.5818624293887887,
        0.5834400184762408,
        0.585021884841625,
        0.5866080400818185,
        0.5881984958251406,
        0.5897932637314379,
        0.5913923554921704,
        0.5929957828304968,
        0.5946035575013605,
        0.5962156912915756,
        0.5978321960199137,
        0.5994530835371903,
        0.6010783657263515,
        0.6027080545025619,
        0.6043421618132907,
        0.6059806996384005,
        0.6076236799902344,
        0.6092711149137041,
        0.6109230164863786,
        0.6125793968185725,
        0.6142402680534349,
        0.6159056423670379,
        0.6175755319684665,
        0.6192499490999082,
        0.620928906036742,
        0.622612415087629,
        0.6243004885946023,
        0.6259931389331581,
        0.6276903785123455,
        0.6293922197748583,
        0.6310986751971253,
        0.6328097572894031,
        0.6345254785958666,
        0.6362458516947014,
        0.637970889198196,
        0.6397006037528346,
        0.6414350080393891,
        0.6431741147730128,
        0.6449179367033329,
        0.6466664866145447,
        0.6484197773255048,
        0.6501778216898253,
        0.6519406325959679,
        0.6537082229673385,
        0.6554806057623822,
        0.6572577939746774,
        0.659039800633032,
        0.6608266388015788,
        0.6626183215798706,
        0.6644148621029772,
        0.6662162735415805,
        0.6680225691020727,
        0.6698337620266515,
        0.6716498655934177,
        0.6734708931164728,
        0.6752968579460171,
        0.6771277734684463,
        0.6789636531064505,
        0.6808045103191123,
        0.6826503586020058,
        0.6845012114872953,
        0.6863570825438342,
        0.688217985377265,
        0.690083933630119,
        0.6919549409819159,
        0.6938310211492645,
        0.6957121878859629,
        0.6975984549830999,
        0.6994898362691555,
        0.7013863456101023,
        0.7032879969095076,
        0.7051948041086352,
        0.7071067811865475,
        0.7090239421602076,
        0.7109463010845827,
        0.7128738720527471,
        0.7148066691959849,
        0.7167447066838943,
        0.718687998724491,
        0.7206365595643126,
        0.7225904034885232,
        0.7245495448210174,
        0.7265139979245261,
        0.7284837772007218,
        0.7304588970903234,
        0.7324393720732029,
        0.7344252166684908,
        0.7364164454346837,
        0.7384130729697496,
        0.7404151139112358,
        0.7424225829363761,
        0.7444354947621984,
        0.7464538641456323,
        0.7484777058836176,
        0.7505070348132126,
        0.7525418658117031,
        0.7545822137967112,
        0.7566280937263048,
        0.7586795205991071,
        0.7607365094544071,
        0.762799075372269,
        0.7648672334736434,
        0.7669409989204777,
        0.7690203869158282,
        0.7711054127039704,
        0.7731960915705107,
        0.7752924388424999,
        0.7773944698885442,
        0.7795022001189185,
        0.7816156449856788,
        0.7837348199827764,
        0.7858597406461707,
        0.7879904225539431,
        0.7901268813264122,
        0.7922691326262467,
        0.7944171921585818,
        0.7965710756711334,
        0.7987307989543135,
        0.8008963778413465,
        0.8030678282083853,
        0.805245165974627,
        0.8074284071024302,
        0.8096175675974316,
        0.8118126635086642,
        0.8140137109286738,
        0.8162207259936375,
        0.8184337248834821,
        0.820652723822003,
        0.8228777390769823,
        0.8251087869603088,
        0.8273458838280969,
        0.8295890460808079,
        0.8318382901633681,
        0.8340936325652911,
        0.8363550898207981,
        0.8386226785089391,
        0.8408964152537144,
        0.8431763167241966,
        0.8454623996346523,
        0.8477546807446661,
        0.8500531768592616,
        0.8523579048290255,
        0.8546688815502312,
        0.8569861239649629,
        0.8593096490612387,
        0.8616394738731368,
        0.8639756154809185,
        0.8663180910111553,
        0.8686669176368529,
        0.871022112577578,
        0.8733836930995842,
        0.8757516765159389,
        0.8781260801866495,
        0.8805069215187917,
        0.8828942179666361,
        0.8852879870317771,
        0.8876882462632604,
        0.890095013257712,
        0.8925083056594671,
        0.8949281411607002,
        0.8973545375015533,
        0.8997875124702672,
        0.9022270839033115,
        0.9046732696855155,
        0.9071260877501991,
        0.909585556079304,
        0.9120516927035263,
        0.9145245157024483,
        0.9170040432046711,
        0.9194902933879467,
        0.9219832844793128,
        0.9244830347552253,
        0.9269895625416926,
        0.92950288621441,
        0.9320230241988943,
        0.9345499949706191,
        0.9370838170551498,
        0.93962450902828,
        0.9421720895161669,
        0.9447265771954693,
        0.9472879907934827,
        0.9498563490882775,
        0.9524316709088368,
        0.9550139751351947,
        0.9576032806985735,
        0.9601996065815236,
        0.9628029718180622,
        0.9654133954938133,
        0.9680308967461471,
        0.9706554947643201,
        0.9732872087896164,
        0.9759260581154889,
        0.9785720620876999,
        0.9812252401044634,
        0.9838856116165875,
        0.9865531961276168,
        0.9892280131939752,
        0.9919100824251095,
        0.9945994234836328,
        0.9972960560854698,
    ],
];

/// Calculate the schema value for a given bucket factor (like Go's pickSchema function).
///
/// The schema defines the bucket schema for native histograms.
/// For base-2 bucket schemas where `bucket_factor` = 2^(2^-n), the schema is n.
///
/// Examples:
/// - `bucket_factor` = 2.0 -> schema = 0 (1 bucket per power of 2)
/// - `bucket_factor` = sqrt(2) ≈ 1.414 -> schema = 1 (2 buckets per power of 2)
/// - `bucket_factor` = 2^(1/4) ≈ 1.189 -> schema = 2 (4 buckets per power of 2)
pub(crate) fn calculate_schema_from_bucket_factor(bucket_factor: f64) -> i32 {
    // For bucket_factor = 2^(2^-n), we want to solve for n
    // bucket_factor = 2^(2^-n)
    // log2(bucket_factor) = 2^-n
    // log2(log2(bucket_factor)) = -n
    // n = -log2(log2(bucket_factor))

    assert!(bucket_factor > 1.0, "bucket_factor must be greater than 1.0");

    let log_bucket_factor = bucket_factor.log2();
    assert!(log_bucket_factor > 0.0, "log of bucket_factor must be positive");

    #[allow(clippy::cast_possible_truncation)]
    let schema = -(log_bucket_factor.log2()).round() as i32;

    // Clamp to valid schema range
    schema.clamp(MIN_SCHEMA, MAX_SCHEMA)
}

/// Configuration for native histograms.
#[derive(Debug, Clone)]
pub struct NativeHistogramConfig {
    /// The base for the exponential buckets. Must be greater than 1.
    /// Common values are 2.0 for power-of-2 buckets, or smaller values
    /// like 1.1 for finer granularity.
    bucket_factor: f64,
    /// Maximum number of buckets. This limits memory usage.
    max_buckets: u32,
    /// The zero threshold. Values within [`-zero_threshold`, `zero_threshold`] are
    /// considered zero and tracked in a special zero bucket.
    zero_threshold: f64,
}

impl NativeHistogramConfig {
    /// Creates a new native histogram configuration.
    ///
    /// # Arguments
    /// * `bucket_factor` - The base for exponential buckets (must be > 1.0)
    /// * `max_buckets` - Maximum number of buckets to limit memory usage
    /// * `zero_threshold` - Threshold for considering values as zero (must be >= 0.0)
    ///
    /// # Returns
    /// A new configuration, or an error if parameters are invalid.
    ///
    /// # Errors
    /// Returns an error if `bucket_factor` is not greater than 1.0, `max_buckets` is 0,
    /// or `zero_threshold` is negative.
    pub fn new(
        bucket_factor: f64,
        max_buckets: u32,
        zero_threshold: f64,
    ) -> Result<Self, &'static str> {
        if bucket_factor <= 1.0 {
            return Err("bucket_factor must be greater than 1.0");
        }
        if max_buckets == 0 {
            return Err("max_buckets must be greater than 0");
        }
        if zero_threshold < 0.0 {
            return Err("zero_threshold must be non-negative");
        }

        Ok(Self { bucket_factor, max_buckets, zero_threshold })
    }

    /// Returns the bucket factor.
    pub fn bucket_factor(&self) -> f64 {
        self.bucket_factor
    }

    /// Returns the maximum number of buckets.
    pub fn max_buckets(&self) -> u32 {
        self.max_buckets
    }

    /// Returns the zero threshold.
    pub fn zero_threshold(&self) -> f64 {
        self.zero_threshold
    }
}

/// A native histogram implementation using exponential buckets.
///
/// This implementation follows the Prometheus native histogram specification,
/// using sparse representation with exponential buckets and schema-based indexing.
#[derive(Debug)]
pub struct NativeHistogram {
    config: NativeHistogramConfig,
    /// Count of observations
    count: AtomicU64,
    /// Sum of all observations (stored as atomic u64 bits)
    sum: AtomicU64,
    /// Count of zero observations (values within `zero_threshold`)
    zero_count: AtomicU64,
    /// Positive buckets: maps bucket index to count
    positive_buckets: std::sync::RwLock<std::collections::BTreeMap<i32, u64>>,
    /// Negative buckets: maps bucket index to count
    negative_buckets: std::sync::RwLock<std::collections::BTreeMap<i32, u64>>,
    /// Schema for bucket calculations (atomic for thread safety) - calculated from `bucket_factor`
    schema: AtomicI32,
    /// Number of buckets currently used (for limiting)
    bucket_count: AtomicU64,
}

impl NativeHistogram {
    /// Creates a new native histogram with the given configuration.
    pub(crate) fn new(config: NativeHistogramConfig) -> Self {
        let schema = calculate_schema_from_bucket_factor(config.bucket_factor());

        Self {
            schema: AtomicI32::new(schema),
            config,
            count: AtomicU64::new(0),
            sum: AtomicU64::new((0.0f64).to_bits()),
            zero_count: AtomicU64::new(0),
            positive_buckets: std::sync::RwLock::new(std::collections::BTreeMap::new()),
            negative_buckets: std::sync::RwLock::new(std::collections::BTreeMap::new()),
            bucket_count: AtomicU64::new(0),
        }
    }

    /// Records a single observation.
    pub(crate) fn observe(&self, value: f64) {
        self.count.fetch_add(1, Ordering::Relaxed);

        // Skip sparse bucket logic and sum updates for NaN values
        if value.is_nan() {
            return;
        }

        // Atomically update the sum using compare-and-swap loop
        loop {
            let current_sum_bits = self.sum.load(Ordering::Relaxed);
            let current_sum = f64::from_bits(current_sum_bits);
            let new_sum = current_sum + value;

            if self
                .sum
                .compare_exchange_weak(
                    current_sum_bits,
                    new_sum.to_bits(),
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                break;
            }
        }

        let mut v = value;
        let mut key: i32;
        let schema = self.schema.load(Ordering::Relaxed);
        let zero_threshold = self.config.zero_threshold();
        let mut is_inf = false;

        if v.is_infinite() {
            if v.is_sign_positive() {
                v = f64::MAX;
            } else {
                v = f64::MIN;
            }
            is_inf = true;
        }

        // Calculate bucket key using Go's algorithm with frexp
        let (frac, exp) = frexp(v.abs());

        if schema > 0 {
            // Use predefined bounds for positive schemas
            #[allow(clippy::cast_sign_loss)]
            let bounds = &NATIVE_HISTOGRAM_BOUNDS[schema as usize];
            // Binary search for the bucket
            let idx = bounds
                .binary_search_by(|&bound| {
                    bound.partial_cmp(&frac).unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap_or_else(|x| x);
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            let key_base = idx as i32;
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            let bounds_len = bounds.len() as i32;
            key = key_base + (exp - 1) * bounds_len;
        } else {
            // For schema <= 0, use simpler calculation
            key = exp;
            if (frac - 0.5).abs() < f64::EPSILON {
                key -= 1;
            }
            if schema < 0 {
                let offset = (1 << (-schema)) - 1;
                key = (key + offset) >> (-schema);
            }
        }

        // Increment key for infinity values
        if is_inf {
            key += 1;
        }

        // Track if we added a new bucket
        let mut added_new_bucket = false;

        // Determine which bucket to update based on value and zero threshold
        if value > zero_threshold {
            let mut buckets = self.positive_buckets.write().unwrap();
            // Use single entry API call to avoid race condition
            match buckets.entry(key) {
                Entry::Vacant(entry) => {
                    entry.insert(1);
                    self.bucket_count.fetch_add(1, Ordering::Relaxed);
                    added_new_bucket = true;
                }
                Entry::Occupied(mut entry) => {
                    *entry.get_mut() += 1;
                }
            }
        } else if value < -zero_threshold {
            let mut buckets = self.negative_buckets.write().unwrap();
            // Use single entry API call to avoid race condition
            match buckets.entry(key) {
                Entry::Vacant(entry) => {
                    entry.insert(1);
                    self.bucket_count.fetch_add(1, Ordering::Relaxed);
                    added_new_bucket = true;
                }
                Entry::Occupied(mut entry) => {
                    *entry.get_mut() += 1;
                }
            }
        } else {
            // Value is within zero threshold
            self.zero_count.fetch_add(1, Ordering::Relaxed);
        }

        // Check bucket limit after releasing locks
        if added_new_bucket {
            self.limit_buckets();
        }
    }

    /// Limits the number of buckets.
    fn limit_buckets(&self) {
        if self.config.max_buckets() == 0 {
            return; // No limit configured
        }

        let current_bucket_count = self.bucket_count();
        if current_bucket_count <= u64::from(self.config.max_buckets()) {
            return; // Under the limit
        }

        self.reduce_bucket_resolution();
    }

    /// Reduces bucket resolution.
    /// This reduces the schema by 1 (doubles bucket width) and re-buckets
    /// existing data.
    fn reduce_bucket_resolution(&self) {
        let current_schema = self.schema.load(Ordering::Relaxed);

        // If we're already at minimum schema, we can't reduce further
        if current_schema <= MIN_SCHEMA {
            return;
        }

        // Reduce schema by 1 (double bucket width)
        let new_schema = current_schema - 1;
        self.schema.store(new_schema, Ordering::Relaxed);

        // Re-bucket positive buckets
        {
            let mut pos_buckets = self.positive_buckets.write().unwrap();
            let old_buckets = std::mem::take(&mut *pos_buckets);
            let old_count = old_buckets.len() as u64;

            for (mut k, v) in old_buckets {
                if k > 0 {
                    k += 1;
                }
                k /= 2;

                *pos_buckets.entry(k).or_insert(0) += v;
            }

            // Update bucket count
            let new_count = pos_buckets.len() as u64;
            if new_count < old_count {
                self.bucket_count.fetch_sub(old_count - new_count, Ordering::Relaxed);
            }
        }

        // Re-bucket negative buckets
        {
            let mut neg_buckets = self.negative_buckets.write().unwrap();
            let old_buckets = std::mem::take(&mut *neg_buckets);
            let old_count = old_buckets.len() as u64;

            for (mut k, v) in old_buckets {
                if k > 0 {
                    k += 1;
                }
                k /= 2;

                *neg_buckets.entry(k).or_insert(0) += v;
            }

            // Update bucket count
            let new_count = neg_buckets.len() as u64;
            if new_count < old_count {
                self.bucket_count.fetch_sub(old_count - new_count, Ordering::Relaxed);
            }
        }

        // Note: This operation preserves all bucket counts by merging them into wider buckets,
        // maintaining count, sum, and zero_count while reducing resolution.
    }

    /// Returns the total count of observations.
    #[cfg(any(feature = "protobuf", test))]
    pub(crate) fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    /// Returns the sum of all observations.
    #[cfg(any(feature = "protobuf", test))]
    pub(crate) fn sum(&self) -> f64 {
        f64::from_bits(self.sum.load(Ordering::Relaxed))
    }

    /// Returns the count of zero observations.
    #[cfg(any(feature = "protobuf", test))]
    pub(crate) fn zero_count(&self) -> u64 {
        self.zero_count.load(Ordering::Relaxed)
    }

    /// Returns a snapshot of the positive buckets.
    #[cfg(any(feature = "protobuf", test))]
    pub(crate) fn positive_buckets(&self) -> std::collections::BTreeMap<i32, u64> {
        self.positive_buckets.read().unwrap().clone()
    }

    /// Returns a snapshot of the negative buckets.
    #[cfg(any(feature = "protobuf", test))]
    pub(crate) fn negative_buckets(&self) -> std::collections::BTreeMap<i32, u64> {
        self.negative_buckets.read().unwrap().clone()
    }

    /// Returns the configuration used by this histogram.
    #[cfg(feature = "protobuf")]
    pub(crate) fn config(&self) -> &NativeHistogramConfig {
        &self.config
    }

    /// Returns the current schema being used.
    #[cfg(any(feature = "protobuf", test))]
    pub(crate) fn schema(&self) -> i32 {
        self.schema.load(Ordering::Relaxed)
    }

    /// Returns the total number of buckets currently in use.
    fn bucket_count(&self) -> u64 {
        self.bucket_count.load(Ordering::Relaxed)
    }
}

impl Clone for NativeHistogram {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            count: AtomicU64::new(self.count.load(Ordering::Relaxed)),
            sum: AtomicU64::new(self.sum.load(Ordering::Relaxed)),
            zero_count: AtomicU64::new(self.zero_count.load(Ordering::Relaxed)),
            positive_buckets: std::sync::RwLock::new(self.positive_buckets.read().unwrap().clone()),
            negative_buckets: std::sync::RwLock::new(self.negative_buckets.read().unwrap().clone()),
            schema: AtomicI32::new(self.schema.load(Ordering::Relaxed)),
            bucket_count: AtomicU64::new(self.bucket_count()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frexp_function() {
        let (m, e) = frexp(1.0);
        assert!((m - 0.5).abs() < f64::EPSILON);
        assert_eq!(e, 1);

        let (m, e) = frexp(2.0);
        assert!((m - 0.5).abs() < f64::EPSILON);
        assert_eq!(e, 2);

        let (m, e) = frexp(0.5);
        assert!((m - 0.5).abs() < f64::EPSILON);
        assert_eq!(e, 0);

        let (m, e) = frexp(4.0);
        assert!((m - 0.5).abs() < f64::EPSILON);
        assert_eq!(e, 3);

        // Test zero
        let (m, e) = frexp(0.0);
        assert_eq!(m, 0.0);
        assert_eq!(e, 0);

        // Test negative numbers
        let (m, e) = frexp(-2.0);
        assert!((m - (-0.5)).abs() < f64::EPSILON);
        assert_eq!(e, 2);
    }

    #[test]
    fn test_observe_nan_values() {
        let config = NativeHistogramConfig::new(2.0, 160, 0.1).unwrap();
        let histogram = NativeHistogram::new(config);

        // Observe some normal values first
        histogram.observe(1.0);
        histogram.observe(2.0);

        // Observe NaN - should increment count but not affect sum or buckets
        histogram.observe(f64::NAN);

        assert_eq!(histogram.count(), 3);
        assert!((histogram.sum() - 3.0).abs() < f64::EPSILON); // Sum should still be 1.0 + 2.0
        assert_eq!(histogram.zero_count(), 0);
        assert!(!histogram.positive_buckets().is_empty()); // Should have buckets from normal values
    }

    #[test]
    fn test_bucket_reduction() {
        // Create histogram with very low bucket limit to trigger reduction
        let config = NativeHistogramConfig::new(2.0, 2, 0.1).unwrap(); // Only 2 buckets max
        let histogram = NativeHistogram::new(config);

        let initial_schema = histogram.schema();

        // Add observations that will create multiple buckets
        histogram.observe(1.0);
        histogram.observe(2.0);
        histogram.observe(4.0);
        histogram.observe(8.0);

        // Should have triggered bucket reduction and schema change
        let final_schema = histogram.schema();
        assert!(final_schema < initial_schema, "Schema should have been reduced");

        // Count and sum should be preserved
        assert_eq!(histogram.count(), 4);
        assert!((histogram.sum() - 15.0).abs() < f64::EPSILON);

        // Buckets should be preserved (merged, not cleared)
        let pos_buckets = histogram.positive_buckets();
        assert!(!pos_buckets.is_empty(), "Buckets should be preserved after reduction");

        // The total count across all buckets should still be 4
        let total_bucket_count: u64 = pos_buckets.values().sum();
        assert_eq!(total_bucket_count, 4, "All observations should be preserved in buckets");
    }

    #[test]
    fn test_new_native_histogram() {
        let config = NativeHistogramConfig::new(2.0, 160, 0.1).unwrap();
        let histogram = NativeHistogram::new(config);
        assert_eq!(histogram.count(), 0);
        assert!((histogram.sum() - 0.0).abs() < f64::EPSILON);
        assert_eq!(histogram.zero_count(), 0);
        assert_eq!(histogram.schema(), 0); // 2.0 -> schema 0
    }

    #[test]
    fn test_observe_positive_values() {
        let config = NativeHistogramConfig::new(2.0, 160, 0.1).unwrap();
        let histogram = NativeHistogram::new(config);
        histogram.observe(1.0);
        histogram.observe(2.0);
        histogram.observe(4.0);

        assert_eq!(histogram.count(), 3);
        assert!((histogram.sum() - 7.0).abs() < f64::EPSILON);
        assert_eq!(histogram.zero_count(), 0);

        let pos_buckets = histogram.positive_buckets();
        assert!(!pos_buckets.is_empty());
    }

    #[test]
    fn test_observe_negative_values() {
        let config = NativeHistogramConfig::new(2.0, 160, 0.1).unwrap();
        let histogram = NativeHistogram::new(config);
        histogram.observe(-1.0);
        histogram.observe(-2.0);

        assert_eq!(histogram.count(), 2);
        assert!((histogram.sum() - (-3.0)).abs() < f64::EPSILON);
        assert_eq!(histogram.zero_count(), 0);

        let neg_buckets = histogram.negative_buckets();
        assert!(!neg_buckets.is_empty());
        assert!(histogram.positive_buckets().is_empty());
    }

    #[test]
    fn test_observe_zero_values() {
        let config = NativeHistogramConfig::new(2.0, 160, 0.1).unwrap();
        let histogram = NativeHistogram::new(config);
        histogram.observe(0.0);
        histogram.observe(0.05);
        histogram.observe(-0.05);

        assert_eq!(histogram.count(), 3);
        assert_eq!(histogram.zero_count(), 3);
        assert!(histogram.positive_buckets().is_empty());
        assert!(histogram.negative_buckets().is_empty());
    }

    #[test]
    fn test_schema_based_bucketing() {
        let config = NativeHistogramConfig::new(2.0, 160, 0.1).unwrap();
        let histogram = NativeHistogram::new(config);

        // Test that different values go to different buckets
        histogram.observe(1.0);
        histogram.observe(2.0);
        histogram.observe(4.0);

        assert_eq!(histogram.count(), 3);
        assert_eq!(histogram.zero_count(), 0);

        let pos_buckets = histogram.positive_buckets();
        // Should have buckets for different values
        assert!(!pos_buckets.is_empty());
    }

    #[test]
    fn test_invalid_config() {
        // Invalid bucket_factor
        assert!(NativeHistogramConfig::new(0.5, 160, 1e-128).is_err());

        // Invalid max_buckets
        assert!(NativeHistogramConfig::new(2.0, 0, 1e-128).is_err());

        // Invalid zero_threshold
        assert!(NativeHistogramConfig::new(2.0, 160, -1.0).is_err());
    }
}
