#[cfg(all(test, feature = "http-listener"))]
mod http_listener_test {
    use http_body_util::{BodyExt, Collected, Empty};
    use hyper::{
        body::{Buf, Bytes},
        header::{ACCEPT, CONTENT_TYPE},
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

            let (status, body, _) = read_from(uri, None).await;

            assert_eq!(status, StatusCode::OK);
            assert!(String::from_utf8(body)
                .unwrap()
                .contains("basic_gauge{wutang=\"forever\"} -1.23"));
        });
    }

    #[cfg(feature = "protobuf")]
    #[test]
    fn test_http_listener_protobuf() {
        use prost::Message;

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

            let labels = vec![Label::new("test", "protobuf")];
            let key = Key::from_parts("test_gauge", labels);
            let gauge = recorder.register_gauge(&key, &METADATA);
            gauge.set(42.0);

            runtime.spawn(exporter);
            tokio::time::sleep(Duration::from_millis(200)).await;

            let uri = format!("http://{socket_address}")
                .parse::<Uri>()
                .unwrap_or_else(|e| panic!("Error parsing URI: {:?}", e));

            // Test protobuf content negotiation
            let (status, body, content_type) =
                read_from(uri, Some("application/vnd.google.protobuf")).await;

            assert_eq!(status, StatusCode::OK);
            assert!(content_type.contains("application/vnd.google.protobuf"));
            assert!(!body.is_empty(), "Protobuf response should not be empty");

            // Parse the protobuf response to verify it's correct
            let mut cursor = std::io::Cursor::new(&body);

            // Include the generated protobuf types for testing
            mod pb {
                include!(concat!(env!("OUT_DIR"), "/io.prometheus.client.rs"));
            }

            let metric_family = pb::MetricFamily::decode_length_delimited(&mut cursor)
                .expect("Failed to decode protobuf response");

            assert_eq!(metric_family.name.as_ref().unwrap(), "test_gauge");
            assert_eq!(metric_family.r#type.unwrap(), pb::MetricType::Gauge as i32);
            assert_eq!(metric_family.metric.len(), 1);

            let metric = &metric_family.metric[0];
            assert!(metric.gauge.is_some());
            assert_eq!(metric.gauge.as_ref().unwrap().value.unwrap(), 42.0);

            assert_eq!(metric.label.len(), 1);
            assert_eq!(metric.label[0].name.as_ref().unwrap(), "test");
            assert_eq!(metric.label[0].value.as_ref().unwrap(), "protobuf");
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

    async fn read_from(
        endpoint: Uri,
        accept_header: Option<&str>,
    ) -> (StatusCode, Vec<u8>, String) {
        let client =
            Client::builder(hyper_util::rt::TokioExecutor::new()).build(HttpConnector::new());

        let req = Request::builder().uri(endpoint.to_string());

        let req = if let Some(accept) = accept_header { req.header(ACCEPT, accept) } else { req };

        let req = req
            .body(Empty::<Bytes>::new())
            .unwrap_or_else(|e| panic!("Failed building request: {:?}", e));

        let response = client
            .request(req)
            .await
            .unwrap_or_else(|e| panic!("Failed requesting data from {endpoint}: {:?}", e));

        let status = response.status();
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let mut body = response
            .into_body()
            .collect()
            .await
            .map(Collected::aggregate)
            .unwrap_or_else(|e| panic!("Error reading response: {:?}", e));

        let body_bytes = body.copy_to_bytes(body.remaining()).to_vec();

        (status, body_bytes, content_type)
    }
}
