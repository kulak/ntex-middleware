use core::time::Duration;
use ntex::service::{Middleware, Service, ServiceCtx};
use ntex::util::BoxFuture;
use ntex::web::{Error, WebRequest, WebResponse};
use std::sync::Arc;
use time::OffsetDateTime;
use tokio::sync::Mutex;
use tokio::time::interval;

/// # Example
///
/// ```
/// let last_time = Arc::new(Mutex::new(OffsetDateTime::now_utc()));
/// let server_last_time = last_time.clone();
///
/// let server = web::server(move || {
///     App::new()
///         .wrap(inactivity::Inactivity {
///             last_accessed: server_last_time.clone(),
///         })
///         .service((
///             web::resource("/index.html").to(|| async { "Hello world!" }),
///         ))
/// })
/// .bind("127.0.0.1:8080")?
/// .run();
///
/// let shutdown_server = server.clone();
/// tokio::spawn(async move {
///     inactivity::until_idle(last_time).await;
///     info!("reached limit of inactivity, shutting down the server");
///     shutdown_server.stop(true).await;
/// });
///
/// server.await?;
/// ```
pub struct Inactivity {
    pub last_accessed: Arc<Mutex<OffsetDateTime>>,
}

// Middleware factory is `Middleware` trait from ntex-service crate
// `S` - type of the next service
// `B` - type of response's body
impl<S> Middleware<S> for Inactivity {
    type Service = InactivityMiddleware<S>;

    fn create(&self, service: S) -> Self::Service {
        InactivityMiddleware {
            service: service,
            last_accessed: self.last_accessed.clone(),
        }
    }
}

pub struct InactivityMiddleware<S> {
    service: S,
    last_accessed: Arc<Mutex<OffsetDateTime>>,
}

impl<S, Err> Service<WebRequest<Err>> for InactivityMiddleware<S>
where
    Err: 'static,
    S: Service<WebRequest<Err>, Response = WebResponse, Error = Error>,
{
    type Response = WebResponse;
    type Error = Error;
    type Future<'f> = BoxFuture<'f, Result<Self::Response, Self::Error>> where S: 'f;

    ntex::forward_poll_ready!(service);
    ntex::forward_poll_shutdown!(service);

    fn call<'a>(&'a self, req: WebRequest<Err>, ctx: ServiceCtx<'a, Self>) -> Self::Future<'a> {
        Box::pin(async move {
            let mut lock = self.last_accessed.lock().await;
            *lock = OffsetDateTime::now_utc();
            let res = ctx.call(&self.service, req).await?;
            Ok(res)
        })
    }
}

/// Helps to build background monitor to shutdown server on inactivity.
/// See example for Inactivity struct.
pub async fn until_idle(
    last_time: Arc<Mutex<OffsetDateTime>>,
    limit_secs: i64,
    check_interval_secs: u64,
) {
    let mut interval = interval(Duration::from_secs(check_interval_secs));
    loop {
        let now = OffsetDateTime::now_utc();
        let last = last_time.lock().await;
        let idle = now - *last;
        if idle.whole_seconds() > limit_secs {
            break;
        };
        interval.tick().await;
    }
}
