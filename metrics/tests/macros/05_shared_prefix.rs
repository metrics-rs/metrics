#![no_implicit_prelude]

fn main() {
    let labels = ::metrics::LabelSet::new(&[("service", "example")]);
    let _ = ::metrics::counter!("requests", &labels, "status" => "200");
    let _ = ::metrics::gauge!("connections", &labels, "state" => "active",);
    let _ = ::metrics::histogram!("duration", &labels, "method" => "GET");

    let empty = ::metrics::labels!();
    let common = ::metrics::labels!("service" => "example",);
    let key = "method";
    let dynamic = ::metrics::labels!(key => "GET");
    let requests = ::metrics::labels!(&common, "method" => "GET",);
    let _ = ::metrics::labels!(&empty);
    let _ = ::metrics::labels!(&dynamic,);
    let _ = ::metrics::counter!("requests", &requests);
    let _ = ::metrics::gauge!("connections", &common);
    let _ = ::metrics::histogram!("duration", &requests);

    let combined = ::metrics::labels!(&common, &dynamic, &requests,);
    let _ = ::metrics::labels!(&common, &dynamic, &requests, "status" => "200",);
    let _ = ::metrics::counter!("requests", &common, &dynamic, &requests);
    let _ = ::metrics::gauge!("connections", &common, &dynamic, "state" => "active",);
    let _ = ::metrics::histogram!("duration", &combined, &dynamic,);
}
