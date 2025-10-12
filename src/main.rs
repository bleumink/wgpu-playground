#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use wgpu_web::run;

fn main() -> anyhow::Result<()> {
    run()?;
    Ok(())
}
