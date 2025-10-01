fn main() -> anyhow::Result<()> {
    println!("cargo:rerun-if-changed=res/*");

    let out_dir = std::env::var("OUT_DIR")?;
    let mut copy_options = fs_extra::dir::CopyOptions::new();
    copy_options.overwrite = true;

    let mut paths = Vec::new();
    paths.push("res/");
    fs_extra::copy_items(&paths, out_dir, &copy_options)?;

    Ok(())
}
