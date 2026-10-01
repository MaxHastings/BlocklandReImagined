//! One test body, run twice: on the test's synthetic fixture as a plain
//! test, and on the generated v20 content (which only the local push gate
//! has) as an ignored one.
#![allow(unused_macros)]

/// `synthetic_and_content!(Fixture: body, ...)` emits, for each `body`
/// (`fn body(&Fixture) -> anyhow::Result<()>`), a module `body` holding the
/// tests `synthetic` (`Fixture::synthetic()`) and `content`
/// (`Fixture::content()`, ignored).
macro_rules! synthetic_and_content {
    ($fixture:ident: $($body:ident),+ $(,)?) => {$(
        mod $body {
            #[test]
            fn synthetic() -> anyhow::Result<()> {
                super::$body(&super::$fixture::synthetic()?)
            }
            #[test]
            #[ignore = "requires generated v20 content"]
            fn content() -> anyhow::Result<()> {
                super::$body(&super::$fixture::content()?)
            }
        }
    )+};
}
