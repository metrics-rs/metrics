//! Exports metrics over HTTP.
//!
//! This exporter can utilize observers that are able to be converted to a textual representation
//! via [`Drain<String>`].  It will respond to any requests, regardless of the method or path.
//!
//! # Run Modes
//! - `run` can be used to block the current thread, running the HTTP server on the configured
//! address
//! - `into_future` will return a [`Future`] that when driven will run the HTTP server on the
//! configured address
#![deny(missing_docs)]
#[macro_use]
extern crate log;

use hyper::rt::run as hyper_run;
use hyper::{
    rt::Future,
    service::service_fn_ok,
    {Body, Response, Server},
};
use metrics_core::{Builder, Drain, Observe, Observer};
use std::{net::SocketAddr, sync::Arc};

/// Exports metrics over HTTP.
pub struct HttpExporter<C, B> {
    controller: C,
    builder: B,
    address: SocketAddr,
}

impl<C, B> HttpExporter<C, B>
where
    C: Observe + Send + Sync + 'static,
    B: Builder + Send + Sync + 'static,
    B::Output: Drain<String> + Observer,
{
    /// Creates a new [`HttpExporter`] that listens on the given `address`.
    ///
    /// Observers expose their output by being converted into strings.
    pub fn new(controller: C, builder: B, address: SocketAddr) -> Self {
        HttpExporter {
            controller,
            builder,
            address,
        }
    }

    /// Run the exporter on the current thread.
    ///
    /// This starts an HTTP server on the `address` the exporter was originally configured with,
    /// responding to any request with the output of the configured observer.
    pub fn run(self) {
        let server = self.into_future();
        hyper_run(server);
    }

    /// Converts this exporter into a future which can be driven externally.
    ///
    /// This starts an HTTP server on the `address` the exporter was originally configured with,
    /// responding to any request with the output of the configured observer.
    pub fn into_future(self) -> impl Future<Item = (), Error = ()> {
        let controller = self.controller;
        let builder = self.builder;
        let address = self.address;

        build_hyper_server(controller, builder, address)
    }
}

fn build_hyper_server<C, B>(
    controller: C,
    builder: B,
    address: SocketAddr,
) -> impl Future<Item = (), Error = ()>
where
    C: Observe + Send + Sync + 'static,
    B: Builder + Send + Sync + 'static,
    B::Output: Drain<String> + Observer,
{
    let builder = Arc::new(builder);
    let controller = Arc::new(controller);

    let service = move || {
        let controller2 = controller.clone();
        let builder = builder.clone();

        service_fn_ok(move |_| {
            let mut observer = builder.build();

            controller2.observe(&mut observer);
            let output = observer.drain();
            Response::new(Body::from(output))
        })
    };

    Server::bind(&address)
        .serve(service)
        .map_err(|e| error!("http exporter server error: {}", e))
}
