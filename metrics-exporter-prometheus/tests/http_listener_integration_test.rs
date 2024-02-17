use http_body_util::{BodyExt, Collected, Empty};
use hyper::{
    body::{Buf, Bytes},
    Request, StatusCode, Uri,
};
use hyper_util::client::legacy::{connect::HttpConnector, Client};
use metrics::{Key, Label, Recorder};
use metrics_exporter_prometheus::PrometheusBuilder;
use std::net::SocketAddr;
use std::time::Duration;

static METADATA: metrics::Metadata =
    metrics::Metadata::new(module_path!(), metrics::Level::INFO, Some(module_path!()));

#[cfg(all(test, feature = "http-listener"))]
#[test]
fn test_http_listener() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|e| panic!("Failed to create test runtime: {:?}", e));

    runtime.block_on(async {        
        let port = find_available_port().unwrap();
        let socket_address = SocketAddr::from(([127, 0, 0, 1], port));

        let (recorder, exporter) = {
            PrometheusBuilder::new()
                .with_http_listener(socket_address)
                .build()
                .unwrap_or_else(|e| {
                    panic!("failed to create Prometheus recorder and http listener: {:?}", e)
                })
        };

        let labels = vec![Label::new("wutang", "forever")];
        let key = Key::from_parts("basic_gauge", labels);
        let gauge = recorder.register_gauge(&key, &METADATA);
        gauge.set(-3.14);

        runtime.spawn(exporter); //async { exporter.await});
        tokio::time::sleep(Duration::from_millis(200)).await;

        let uri = format!("http://{socket_address}")
            .parse::<Uri>()
            .unwrap_or_else(|e| panic!("Error parsing URI: {:?}", e));

        let (status, body) = read_from(uri).await;

        println!("Status: {status}");
        println!("Body:");
        println!("{body}");

        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("basic_gauge{wutang=\"forever\"} -3.14"));
    });
}

async fn read_from(endpoint: Uri) -> (StatusCode, String) {
    let client = Client::builder(hyper_util::rt::TokioExecutor::new()).build(HttpConnector::new());

    let req = Request::builder()
        .uri(endpoint.to_string())
        .body(Empty::<Bytes>::new())
        .unwrap_or_else(|e| panic!("Failed building request: {:?}", e));

    let response = client
        .request(req)
        .await
        .unwrap_or_else(|e| panic!("Failed requesting data from {endpoint}: {:?}", e));

    let status = response.status();
    let mut body = response
        .into_body()
        .collect()
        .await
        .map(Collected::aggregate)
        .unwrap_or_else(|e| panic!("Error reading response: {:?}", e));

    let body_string = String::from_utf8(body.copy_to_bytes(body.remaining()).to_vec())
        .unwrap_or_else(|e| panic!("Error decoding response body: {:?}", e));

    (status, body_string)
}

fn find_available_port() -> Option<u16> {
    (25000_u16..26000).find(|&port| match std::net::TcpListener::bind(("localhost", port)) {
        Ok(_) => true,
        Err(_) => false,
    })
}
