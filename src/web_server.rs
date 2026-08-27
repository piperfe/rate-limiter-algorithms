use crate::TokenBucket;
use crate::window_unit::WindowUnit;
use axum::{
    Router,
    body::Body,
    extract::State,
    http::{HeaderMap, Response},
    routing::get,
};
use axum_macros::FromRef;
use dashmap::DashMap;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Instant;

#[derive(Deserialize, Clone, Debug)]
struct AppConfig {
    #[serde(default = "default_capacity")]
    capacity: u64,
    #[serde(default = "default_unit_time")]
    unit_time: WindowUnit,
    #[serde(default = "default_refill_rate_per_unit_time")]
    refill_rate_per_unit_time: u64,
}
fn default_capacity() -> u64 {
    60
}
fn default_unit_time() -> WindowUnit {
    WindowUnit::Seconds
}
fn default_refill_rate_per_unit_time() -> u64 {
    1
}

#[derive(Clone, FromRef)]
struct AppState {
    config: AppConfig,
    client_buckets: Arc<DashMap<String, TokenBucket>>,
}

pub fn create_routes() -> Router {
    let client_buckets = Arc::new(DashMap::new());
    let app_config = envy::from_env::<AppConfig>()
        .expect("Boot Error: Required environment variables are missing or misconfigured!");
    println!("Server configuration loaded: {:?}", app_config);

    Router::new()
        .route("/rate-limit", get(rate_limit_handler))
        .with_state(AppState {
            config: app_config,
            client_buckets,
        })
}

async fn rate_limit_handler(
    State(client_buckets): State<Arc<DashMap<String, TokenBucket>>>,
    State(config): State<AppConfig>,
    headers: HeaderMap,
) -> Response<Body> {
    let client_api_key = match headers.get("X-Api-Key").map(|value| value.to_str()) {
        Some(Ok(client_api_key)) => client_api_key,
        Some(Err(_)) => {
            return bad_request_response_builder("X-Api-Key header contains invalid characters");
        }
        None => return bad_request_response_builder("X-Api-Key header is missing"),
    };

    let now = Instant::now();
    let mut client_bucket = client_buckets
        .entry(client_api_key.to_string())
        .or_insert_with(|| {
            TokenBucket::new(
                config.capacity,
                config.unit_time,
                config.refill_rate_per_unit_time,
                now,
            )
        });
    let response = client_bucket.is_allowed(now);

    if !response.allowed {
        return too_many_requests_response_builder(client_api_key);
    }
    response_ok_builder(client_api_key, response.remaining_tokens)
}

fn too_many_requests_response_builder(client_api_key: &str) -> Response<Body> {
    return Response::builder()
        .status(429)
        .header("RateLimit-Policy", "\"api-v1\";q=1;w=1")
        .header(
            "RateLimit",
            format!("\"api-v1\";r=0;t=1;pk=:{}:", client_api_key),
        )
        .body(Body::from("Too Many Requests"))
        .unwrap();
}

fn bad_request_response_builder(reason: &str) -> Response<Body> {
    Response::builder()
        .status(400)
        .body(Body::from(format!("Bad Request: {}", reason)))
        .unwrap()
}

fn response_ok_builder(client_api_key: &str, remaining_tokens: u64) -> Response<Body> {
    Response::builder()
        .status(200)
        .header("RateLimit-Policy", "\"api-v1\";q=1;w=1")
        .header(
            "RateLimit",
            format!(
                "\"api-v1\";r={};t=1;pk=:{}:",
                remaining_tokens, client_api_key
            ),
        )
        .body(Body::from("Hello, World!"))
        .unwrap()
}

#[cfg(test)]
mod integration_tests {
    use crate::web_server::create_routes;
    use axum_test::TestServer;
    use serial_test::serial;
    use temp_env;

    mod bad_request {
        use super::*;

        #[tokio::test]
        async fn should_return_400_when_api_key_header_is_missing() {
            let routes = create_routes();
            let server = TestServer::new(routes);

            let response = server
                .get("/rate-limit")
                .add_header("X-API-identifier", "client_1")
                .await;

            assert_eq!(response.status_code(), 400);
            assert_eq!(response.text(), "Bad Request: X-Api-Key header is missing");
        }

        #[tokio::test]
        async fn should_return_400_when_api_key_header_has_invalid_characters() {
            let routes = create_routes();
            let server = TestServer::new(routes);

            let response = server
                .get("/rate-limit")
                .add_header("X-API-Key", "clïent")
                .await;

            assert_eq!(response.status_code(), 400);
            assert_eq!(
                response.text(),
                "Bad Request: X-Api-Key header contains invalid characters"
            );
        }
    }

    mod configuration {
        use super::*;

        #[tokio::test]
        #[serial]
        async fn should_return_200_with_custom_capacity_from_env_vars() {
            let bucket_capacity = ("CAPACITY", Some("10"));
            let unit_time = ("UNIT_TIME", Some("Seconds"));
            let refill_rate_per_unit_time = ("REFILL_RATE_PER_UNIT_TIME", Some("1"));
            temp_env::async_with_vars(
                [bucket_capacity, unit_time, refill_rate_per_unit_time],
                (|| async {
                    let routes = create_routes();
                    let server = TestServer::new(routes);

                    let response = server
                        .get("/rate-limit")
                        .add_header("X-API-Key", "client_1")
                        .await;

                    assert_eq!(response.status_code(), 200);
                    assert_eq!(response.text(), "Hello, World!");
                    assert_eq!(
                        response.header("RateLimit-Policy").to_str().unwrap(),
                        "\"api-v1\";q=1;w=1"
                    );
                    assert_eq!(
                        response.header("RateLimit").to_str().unwrap(),
                        "\"api-v1\";r=9;t=1;pk=:client_1:"
                    );
                })(),
            )
            .await;
        }

        #[tokio::test]
        #[serial]
        async fn should_apply_the_configured_unit_time_to_the_refill_period() {
            let bucket_capacity = ("CAPACITY", Some("1"));
            let unit_time = ("UNIT_TIME", Some("Minutes"));
            let refill_rate_per_unit_time = ("REFILL_RATE_PER_UNIT_TIME", Some("1"));
            temp_env::async_with_vars(
                [bucket_capacity, unit_time, refill_rate_per_unit_time],
                (|| async {
                    let routes = create_routes();
                    let server = TestServer::new(routes);

                    let first = server
                        .get("/rate-limit")
                        .add_header("X-API-Key", "client_1")
                        .await;
                    assert_eq!(first.status_code(), 200);

                    // A second passes: enough to refill under the default `Seconds`,
                    // nowhere near enough under the configured `Minutes`.
                    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

                    let second = server
                        .get("/rate-limit")
                        .add_header("X-API-Key", "client_1")
                        .await;
                    assert_eq!(second.status_code(), 429);
                    assert_eq!(second.text(), "Too Many Requests");
                })(),
            )
            .await;
        }

        #[tokio::test]
        #[serial]
        async fn should_return_200_with_default_capacity_for_new_client() {
            let routes = create_routes();
            let server = TestServer::new(routes);

            let response = server
                .get("/rate-limit")
                .add_header("X-API-Key", "client_1")
                .await;

            assert_eq!(response.status_code(), 200);
            assert_eq!(response.text(), "Hello, World!");
            assert_eq!(
                response.header("RateLimit-Policy").to_str().unwrap(),
                "\"api-v1\";q=1;w=1"
            );
            assert_eq!(
                response.header("RateLimit").to_str().unwrap(),
                "\"api-v1\";r=59;t=1;pk=:client_1:"
            );
        }
    }

    mod rate_limiting {
        use super::*;

        #[tokio::test]
        #[serial]
        async fn should_return_200_with_decremented_tokens_on_repeat_request() {
            let routes = create_routes();
            let server = TestServer::new(routes);

            server
                .get("/rate-limit")
                .add_header("X-API-Key", "client_1")
                .await;
            server
                .get("/rate-limit")
                .add_header("X-API-Key", "client_1")
                .await;
            let response = server
                .get("/rate-limit")
                .add_header("X-API-Key", "client_1")
                .await;

            assert_eq!(response.status_code(), 200);
            assert_eq!(response.text(), "Hello, World!");
            assert_eq!(
                response.header("RateLimit-Policy").to_str().unwrap(),
                "\"api-v1\";q=1;w=1"
            );
            assert_eq!(
                response.header("RateLimit").to_str().unwrap(),
                "\"api-v1\";r=57;t=1;pk=:client_1:"
            );
        }

        #[tokio::test]
        #[serial]
        async fn should_return_429_when_tokens_exhausted() {
            let routes = create_routes();
            let server = TestServer::new(routes);
            let mut responses = vec![];
            let client_id = "client_1";

            for _ in 0..=60 {
                let response = server
                    .get("/rate-limit")
                    .add_header("X-API-Key", client_id)
                    .await;
                responses.push(response);
            }

            let too_many_requests = &responses[60];

            assert_eq!(too_many_requests.status_code(), 429);
            assert_eq!(too_many_requests.text(), "Too Many Requests");
            assert_eq!(
                too_many_requests
                    .header("RateLimit-Policy")
                    .to_str()
                    .unwrap(),
                "\"api-v1\";q=1;w=1"
            );
            assert_eq!(
                too_many_requests.header("RateLimit").to_str().unwrap(),
                "\"api-v1\";r=0;t=1;pk=:client_1:"
            );
        }
    }

    mod concurrency {
        use super::*;

        #[tokio::test]
        #[serial]
        async fn should_enforce_limit_correctly_under_concurrent_load_single_client() {
            let routes = create_routes();
            let server = std::sync::Arc::new(TestServer::new(routes));
            let handles: Vec<_> = (1..120)
                .map(|_| {
                    let server = server.clone();
                    tokio::spawn(async move {
                        server
                            .get("/rate-limit")
                            .add_header("X-API-Key", "client_1")
                            .await
                    })
                })
                .collect();

            let results = futures::future::join_all(handles).await;
            let status_200_count = results
                .iter()
                .filter(|response| response.as_ref().is_ok_and(|r| r.status_code() == 200))
                .count();

            let status_429_count = results
                .iter()
                .filter(|response| response.as_ref().is_ok_and(|r| r.status_code() == 429))
                .count();
            assert_eq!(status_200_count, 60);
            assert_eq!(status_429_count, 59);
        }

        #[tokio::test]
        #[serial]
        async fn should_isolate_limits_between_concurrent_clients() {
            let routes = create_routes();
            let server = std::sync::Arc::new(TestServer::new(routes));
            let handles: Vec<_> = (1..120)
                .flat_map(|_| {
                    [
                        tokio::spawn({
                            let server = server.clone();
                            async move {
                                (
                                    "client_1",
                                    server
                                        .get("/rate-limit")
                                        .add_header("X-API-Key", "client_1")
                                        .await,
                                )
                            }
                        }),
                        tokio::spawn({
                            let server = server.clone();
                            async move {
                                (
                                    "client_2",
                                    server
                                        .get("/rate-limit")
                                        .add_header("X-API-Key", "client_2")
                                        .await,
                                )
                            }
                        }),
                    ]
                    .into_iter()
                })
                .collect();

            let results = futures::future::join_all(handles).await;
            let client_1_responses: Vec<_> = results
                .iter()
                .filter_map(|r| r.as_ref().ok())
                .filter(|(client, _)| client == &"client_1")
                .map(|(_, resp)| resp)
                .collect();
            let client_2_responses: Vec<_> = results
                .iter()
                .filter_map(|r| r.as_ref().ok())
                .filter(|(client, _)| client == &"client_2")
                .map(|(_, resp)| resp)
                .collect();
            let client_1_200 = client_1_responses
                .iter()
                .filter(|r| r.status_code() == 200)
                .count();
            let client_1_429 = client_1_responses
                .iter()
                .filter(|r| r.status_code() == 429)
                .count();
            let client_2_200 = client_2_responses
                .iter()
                .filter(|r| r.status_code() == 200)
                .count();
            let client_2_429 = client_2_responses
                .iter()
                .filter(|r| r.status_code() == 429)
                .count();

            assert_eq!(client_1_200, 60);
            assert_eq!(client_1_429, 59);
            assert_eq!(client_2_200, 60);
            assert_eq!(client_2_429, 59);
        }
    }
}
