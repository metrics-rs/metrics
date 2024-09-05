#[cfg(all(test, feature = "http-listener"))]
mod http_listener_test {
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
    use tokio::net::TcpListener;

    static METADATA: metrics::Metadata =
        metrics::Metadata::new(module_path!(), metrics::Level::INFO, Some(module_path!()));

    #[test]
    fn test_http_listener() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap_or_else(|e| panic!("Failed to create test runtime: {:?}", e));

        runtime.block_on(async {
            let local = [127, 0, 0, 1];
            let port = get_available_port(local).await;
            let socket_address = SocketAddr::from((local, port));

            let (recorder, exporter) = {
                PrometheusBuilder::new().with_http_listener(socket_address).build().unwrap_or_else(
                    |e| panic!("failed to create Prometheus recorder and http listener: {:?}", e),
                )
            };

            let labels = vec![Label::new("wutang", "forever")];
            let key = Key::from_parts("basic_gauge", labels);
            let gauge = recorder.register_gauge(&key, &METADATA);
            gauge.set(-1.23);

            runtime.spawn(exporter); //async { exporter.await});
            tokio::time::sleep(Duration::from_millis(200)).await;

            let uri = format!("http://{socket_address}")
                .parse::<Uri>()
                .unwrap_or_else(|e| panic!("Error parsing URI: {:?}", e));

            let (status, body) = read_from(uri).await;

            assert_eq!(status, StatusCode::OK);
            assert!(body.contains("basic_gauge{wutang=\"forever\"} -1.23"));
        });
    }

    async fn get_available_port(listen_address: [u8; 4]) -> u16 {
        let socket_address = SocketAddr::from((listen_address, 0));
        TcpListener::bind(socket_address)
            .await
            .unwrap_or_else(|e| {
                panic!("Unable to bind to an available port on address {socket_address}: {:?}", e);
            })
            .local_addr()
            .expect("Unable to obtain local address from TcpListener")
            .port()
    }

    async fn read_from(endpoint: Uri) -> (StatusCode, String) {
        let client =
            Client::builder(hyper_util::rt::TokioExecutor::new()).build(HttpConnector::new());

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
}
