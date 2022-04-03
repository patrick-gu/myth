use myth::{test, Filter};

#[tokio::test]
async fn simple_routes() {
    let filter = myth::path::literal("foo")
        .handle(|| async { Ok("foo") })
        .or(myth::path::end().handle(|| async { Ok("end") }));

    test::get()
        .success(&filter, |success| assert_eq!(success, "end"))
        .await;

    test::post()
        .uri("/foo")
        .success(&filter, |success| assert_eq!(success, "foo"))
        .await;
}

#[tokio::test]
async fn multiple_literals() {
    let filter = myth::path::literal("foo")
        .and(myth::path::literal("bar"))
        .handle(|| async { Ok("foo") })
        .or(myth::path::end().handle(|| async { Ok("end") }));

    test::put()
        .uri("/foo/bar")
        .success(&filter, |success| assert_eq!(success, "foo"))
        .await;
}
