use myth::{Filter, StatusCode};

#[tokio::test]
async fn basic() -> reqwest::Result<()> {
    let filter = myth::any().handle(|| async { Ok("Hello world!") });
    let server = myth::serve(filter).bind(([127, 0, 0, 1], 0));
    let addr = server.local_addr();
    tokio::spawn(server.run());
    let response = reqwest::get(format!("http://{}/", addr)).await?;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("Content-Type").unwrap(),
        "text/plain; charset=utf-8"
    );
    assert_eq!(response.text().await?, "Hello world!");
    Ok(())
}
