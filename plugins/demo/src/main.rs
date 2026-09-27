use arinn_protocol::{DaemonProxy, PROVIDER_PATH};
use std::time::Duration;
struct Demo {
    delay: Duration,
}
#[zbus::interface(name = "org.arinn.SpikeProvider1")]
impl Demo {
    async fn activate(&self, id: &str) -> zbus::fdo::Result<()> {
        if id != "demo.activity" {
            return Err(zbus::fdo::Error::InvalidArgs("unknown ID".into()));
        }
        tokio::time::sleep(self.delay).await;
        Ok(())
    }
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let token = std::fs::read_to_string(std::env::var("ARINN_SPIKE_TOKEN_FILE")?)?;
    let delay = std::env::var("ARINN_DEMO_DELAY_MS")
        .unwrap_or("100".into())
        .parse()?;
    let connection = zbus::connection::Builder::session()?
        .serve_at(
            PROVIDER_PATH,
            Demo {
                delay: Duration::from_millis(delay),
            },
        )?
        .build()
        .await?;
    let proxy = DaemonProxy::new(&connection).await?;
    loop {
        if let Ok(generation) = proxy.register(token.trim(), 1).await {
            eprintln!("{{\"component\":\"demo\",\"operation\":\"registered\"}}");
            loop {
                let items = vec![("demo.activity".into(), "Демонстрационная активность".into())];
                if proxy.publish(&generation, 1, items).await.is_err() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
