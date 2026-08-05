use crate::TokenBucket;
use axum::{
    Router,
    body::Body,
    extract::State,
    http::{HeaderMap, Response},
    routing::get,
};
use axum_macros::FromRef;
use serde::Deserialize;
use std::sync::{Arc, Mutex};

#[derive(Deserialize, Clone, Debug)]
struct AppConfig {
    #[serde(default = "default_bucket_capacity")]
    bucket_capacity: u64,
    #[serde(default = "default_bucket_rate")]
    bucket_refill_rate_per_second: u64,
}
fn default_bucket_capacity() -> u64 {
    60
}
fn default_bucket_rate() -> u64 {
    1
}

#[derive(Clone, FromRef)]
struct AppState {
    config: AppConfig,
    client_buckets: Arc<Mutex<Vec<TokenBucket>>>,
}

pub fn create_routes() -> Router {
    let client_buckets = Arc::new(Mutex::<Vec<TokenBucket>>::new(vec![]));
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
    State(client_buckets): State<Arc<Mutex<Vec<TokenBucket>>>>,
    State(config): State<AppConfig>,
    headers: HeaderMap,
) -> Response<Body> {
    let client_api_key = headers.get("X-Api-Key").unwrap().to_str().unwrap();
    let mut client_buckets = client_buckets.lock().unwrap();

    let known_client_optional = client_buckets
        .iter_mut()
        .find(|bucket| bucket.matches_client_id(client_api_key));

    if known_client_optional.is_some() {
        let known_client = known_client_optional.unwrap();
        let response = known_client.is_allowed();

        if !response.allowed {
            return too_many_requests_response_builder(client_api_key);
        }
        return response_ok_builder(client_api_key, response.remaining_tokens);
    }

    let mut new_bucket = TokenBucket::new(
        String::from(client_api_key),
        config.bucket_capacity,
        config.bucket_refill_rate_per_second,
    );
    let response = new_bucket.is_allowed();
    client_buckets.push(new_bucket);

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
    use temp_env;

    #[tokio::test]
    async fn should_setting_the_bucket_for_new_users_using_env_vars() {
        let bucket_capacity = ("BUCKET_CAPACITY", Some("10"));
        let bucket_refill_rate_per_second = ("BUCKET_REFILL_RATE_PER_SECOND", Some("1"));
        temp_env::async_with_vars([bucket_capacity, bucket_refill_rate_per_second], (|| async {
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
        })()).await;
    }

    #[tokio::test]
    async fn should_setting_the_bucket_for_new_users_using_default_values() {
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

    #[tokio::test]
    async fn should_decreasing_requests_in_the_bucket_for_old_users() {
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
    async fn should_deny_a_request() {
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

    #[tokio::test]
    async fn should_accepting_and_denying_concurrent_requests_for_one_user() {
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
    async fn should_accepting_and_denying_concurrent_requests_for_multiple_users() {
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
