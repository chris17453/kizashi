use egress_gateway::{
    admin_router, decide, parse_connect_target, parse_proxy_authorization, AdminState,
    PostgresAllowlistRepository, PostgresAuditLogRepository, ProxyDeps,
};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::sync::Arc;
use tokio::net::TcpListener;

/// Relays bytes both directions between the caller and the real destination once a CONNECT
/// tunnel is established — Egress Gateway never inspects the TLS handshake or anything after
/// it (ADR-0021): destination-level audit logging only, not deep request inspection.
async fn tunnel(upgraded: hyper::upgrade::Upgraded, target_addr: String) {
    let mut server_stream = match tokio::net::TcpStream::connect(&target_addr).await {
        Ok(stream) => stream,
        Err(e) => {
            tracing::error!(target = %target_addr, error = %e, "failed to connect to destination");
            return;
        }
    };
    let mut client_stream = TokioIo::new(upgraded);
    if let Err(e) = tokio::io::copy_bidirectional(&mut client_stream, &mut server_stream).await {
        tracing::debug!(target = %target_addr, error = %e, "tunnel closed");
    }
}

async fn handle_connect(
    deps: Arc<ProxyDeps>,
    req: Request<Incoming>,
) -> Result<Response<String>, hyper::Error> {
    let identity = req
        .headers()
        .get("proxy-authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(parse_proxy_authorization);

    let authority = req.uri().authority().map(|a| a.to_string()).unwrap_or_default();
    let Some(target) = parse_connect_target(&authority) else {
        return Ok(Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body("malformed CONNECT target".to_string())
            .unwrap());
    };

    let decision = decide(&deps, identity.as_ref(), &target).await;
    if !decision.allowed {
        tracing::warn!(host = %target.host, "denied by tenant allowlist");
        return Ok(Response::builder()
            .status(StatusCode::FORBIDDEN)
            .body("destination not in tenant allowlist".to_string())
            .unwrap());
    }

    let target_addr = format!("{}:{}", target.host, target.port);
    tokio::spawn(async move {
        match hyper::upgrade::on(req).await {
            Ok(upgraded) => tunnel(upgraded, target_addr).await,
            Err(e) => tracing::error!(error = %e, "failed to upgrade CONNECT request"),
        }
    });

    Ok(Response::builder().status(StatusCode::OK).body(String::new()).unwrap())
}

async fn proxy_service(
    deps: Arc<ProxyDeps>,
    req: Request<Incoming>,
) -> Result<Response<String>, hyper::Error> {
    if req.method() == Method::CONNECT {
        handle_connect(deps, req).await
    } else {
        Ok(Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body("egress-gateway only supports CONNECT tunneling".to_string())
            .unwrap())
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let proxy_addr =
        std::env::var("PROXY_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3128".to_string());
    let admin_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());

    let pool = common::connect_with_schema(&database_url, "egress_gateway")
        .await
        .expect("failed to connect to postgres");
    let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    sqlx::migrate::Migrator::new(migrations_dir)
        .await
        .expect("failed to load migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");

    let allowlist_repository = Arc::new(PostgresAllowlistRepository::new(pool.clone()));
    let deps = Arc::new(ProxyDeps {
        allowlist_repository: allowlist_repository.clone(),
        audit_log_repository: Arc::new(PostgresAuditLogRepository::new(pool)),
    });

    let admin_state = AdminState { allowlist_repository };
    let admin_listener = tokio::net::TcpListener::bind(&admin_addr).await.expect("bind failed");
    tracing::info!(addr = %admin_addr, "egress-gateway admin API listening");
    tokio::spawn(async move {
        axum::serve(admin_listener, admin_router(admin_state)).await.expect("admin server error");
    });

    let proxy_listener = TcpListener::bind(&proxy_addr).await.expect("bind failed");
    tracing::info!(addr = %proxy_addr, "egress-gateway CONNECT proxy listening");
    loop {
        let (stream, _) = match proxy_listener.accept().await {
            Ok(pair) => pair,
            Err(e) => {
                tracing::error!(error = %e, "failed to accept connection");
                continue;
            }
        };
        let io = TokioIo::new(stream);
        let deps = deps.clone();
        tokio::spawn(async move {
            let service = service_fn(move |req| proxy_service(deps.clone(), req));
            if let Err(e) =
                http1::Builder::new().serve_connection(io, service).with_upgrades().await
            {
                tracing::debug!(error = %e, "connection error");
            }
        });
    }
}
