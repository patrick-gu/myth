use myth::{Filter, Responder};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct Person {
    name: String,
    age: i32,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .pretty()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("myth=trace".parse().unwrap())
                .add_directive("json=trace".parse().unwrap()),
        )
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::NEW)
        .init();

    let filter = myth::body::content_length_limit(2 * 2usize.pow(10))
        .and(myth::json::request())
        .handle(|person: Person| async move {
            tracing::debug!("incoming person {:#?}", person);
            let message = format!("Person {} is age {}", person.name, person.age);
            Ok(message.into_response())
        })
        .recover(|error: myth::json::Error| async move {
            Ok(format!("JSON error: {}", error).into_response())
        });

    myth::serve(filter).bind(([127, 0, 0, 1], 8080)).run().await;
}
