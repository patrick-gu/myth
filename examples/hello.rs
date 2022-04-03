use std::{borrow::Cow, env};

use myth::{Filter, Responder};

async fn named_handler(str: Cow<'_, str>) -> myth::Result {
    Ok(format!("Hello {}!", str).into_response())
}

async fn default_handler() -> myth::Result {
    Ok("Hello world!".into_response())
}

fn filter() -> myth::impl_Filter!(myth::Response) {
    myth::path::param_str()
        .and(myth::path::end())
        .handle(named_handler)
        .or(myth::any().handle(default_handler))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .pretty()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("myth=trace".parse().unwrap()),
        )
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::NEW)
        .init();

    myth::serve(filter())
        .bind(([127, 0, 0, 1], 8080))
        .run()
        .await;
}

#[cfg(test)]
mod tests {
    use myth::test;

    use super::filter;

    #[tokio::test]
    async fn get_root() {
        let response = test::get().response(&filter()).await;
        assert_eq!(response.body(), "Hello world!");
    }

    #[tokio::test]
    async fn get_named() {
        let response = test::get().uri("/person").response(&filter()).await;
        assert_eq!(response.body(), "Hello person!");
    }
}
