use smart_omj_lib::closed_loop;

#[tokio::main]
async fn main() {
    let args:Vec<String>=std::env::args().skip(1).collect();
    let root=std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let resources=root.join("resources");
    let result=if args.first().is_some_and(|s|s=="file") {
        match args.get(1) {
            Some(path)=>closed_loop::inspect_file(&resources,std::path::Path::new(path)).map(|v|println!("{v}")),
            None=>Err("file requires a PNG path".into()),
        }
    } else {
        closed_loop::cli(&resources,&args).await
    };
    if let Err(e)=result {eprintln!("{e}");std::process::exit(1);}
}
