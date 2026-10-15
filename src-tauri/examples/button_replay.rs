//! Replay PNG cases through the production detector without device actions.
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("Usage: button_replay <staged-resources> <cases.json> <output.json>".into());
    }
    let resources = PathBuf::from(&args[0]);
    let cases: Vec<serde_json::Value> = serde_json::from_slice(&std::fs::read(&args[1])?)?;
    let mut results = Vec::new();
    for case in cases {
        let path = case["path"].as_str().ok_or("case path missing")?;
        let result = smart_omj_lib::closed_loop::inspect_file(&resources, std::path::Path::new(path))?;
        results.push(serde_json::json!({"key":case["key"],"path":path,"observation":result["observation"]}));
    }
    std::fs::write(&args[2], serde_json::to_vec_pretty(&results)?)?;
    println!("Replayed {} images without sending device input", results.len());
    Ok(())
}
