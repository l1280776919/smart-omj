#[tokio::main]
async fn main() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let serial = std::env::args().nth(1);
    match smart_omj_lib::diagnostics::run(
        &root.join("resources"),
        &root.join("target/diagnostics"),
        serial.as_deref(),
    )
    .await
    {
        Ok(report) => println!(
            "{}",
            serde_json::to_string_pretty(&report).expect("serialize diagnostic report")
        ),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
