use std::net::SocketAddr;
use tokio::net::TcpListener;

/// Check if a local port is free by attempting to bind on 127.0.0.1.
/// Returns true if the port is available, false if already in use.
pub async fn is_local_port_free(port: u16) -> bool {
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    TcpListener::bind(addr).await.is_ok()
}

/// Find a free port on 127.0.0.1 by letting the OS assign one.
/// Returns the assigned port number.
pub async fn find_free_port() -> std::io::Result<u16> {
    let addr: SocketAddr = ([127, 0, 0, 1], 0).into();
    let listener = TcpListener::bind(addr).await?;
    Ok(listener.local_addr()?.port())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn is_local_port_free_returns_false_when_bound() {
        let addr: SocketAddr = ([127, 0, 0, 1], 0).into();
        let listener = TcpListener::bind(addr).await.unwrap();
        let port = listener.local_addr().unwrap().port();

        assert!(!is_local_port_free(port).await);
    }

    #[tokio::test]
    async fn is_local_port_free_returns_true_for_free_port() {
        let addr: SocketAddr = ([127, 0, 0, 1], 0).into();
        let listener = TcpListener::bind(addr).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        assert!(is_local_port_free(port).await);
    }

    #[tokio::test]
    async fn find_free_port_returns_valid_port() {
        let port = find_free_port().await.unwrap();
        assert!(port > 0);
    }

    #[tokio::test]
    async fn find_free_port_returns_different_ports() {
        let port1 = find_free_port().await.unwrap();
        let port2 = find_free_port().await.unwrap();
        assert_ne!(port1, port2);
    }
}
