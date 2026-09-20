//! Export the frozen diagnostic corpus. No browser, network or tool execution.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = cxweb_codex_adapter::quality_corpus::manifest()?;
    serde_json::to_writer_pretty(std::io::stdout().lock(), &manifest)?;
    Ok(())
}
