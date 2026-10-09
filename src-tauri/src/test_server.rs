//! A local HTTP server for tests that send. It records every request it
//! receives and answers each with the same response, so a test can assert on
//! what Conductor actually put on the wire.

use std::{
    convert::Infallible,
    sync::{Arc, Mutex},
};

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{body::Incoming, header::HeaderMap, service::service_fn, Request, Response};
use hyper_util::rt::TokioIo;

pub struct Received {
    pub method: String,
    /// The path and query string, as sent.
    pub target: String,
    pub headers: HeaderMap,
    pub body: Bytes,
}

impl Received {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }
}

pub struct TestServer {
    pub url: String,
    received: Arc<Mutex<Vec<Received>>>,
}

impl TestServer {
    pub fn replying(status: u16, content_type: &'static str, reply: &'static str) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind test server");
        listener
            .set_nonblocking(true)
            .expect("nonblocking listener");
        let url = format!("http://{}", listener.local_addr().expect("local address"));
        let received = Arc::new(Mutex::new(Vec::new()));
        let log = received.clone();

        tauri::async_runtime::spawn(async move {
            let listener = tokio::net::TcpListener::from_std(listener).expect("tokio listener");
            while let Ok((stream, _)) = listener.accept().await {
                let log = log.clone();
                let service = service_fn(move |request: Request<Incoming>| {
                    let log = log.clone();
                    async move {
                        let (parts, body) = request.into_parts();
                        let body = body.collect().await.map(|body| body.to_bytes());
                        log.lock().unwrap().push(Received {
                            method: parts.method.to_string(),
                            target: parts.uri.to_string(),
                            headers: parts.headers,
                            body: body.unwrap_or_default(),
                        });
                        let response = Response::builder()
                            .status(status)
                            .header("content-type", content_type)
                            .body(Full::new(Bytes::from_static(reply.as_bytes())))
                            .expect("valid response");
                        Ok::<_, Infallible>(response)
                    }
                });
                tauri::async_runtime::spawn(async move {
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                });
            }
        });

        Self { url, received }
    }

    /// The one request the server got; fails the test if it got more or none.
    pub fn only_request(&self) -> Received {
        let mut received = self.received.lock().unwrap();
        assert_eq!(received.len(), 1, "expected exactly one request");
        received.pop().unwrap()
    }
}
