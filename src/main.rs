#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dodo_invoice_service::run().await
}
