use anyhow::Result;
use std::io::{Read, Write};

fn main() -> Result<()> {
    const LIMIT: u64 = 256 << 20;
    let mut input = Vec::new();
    std::io::stdin().take(LIMIT + 1).read_to_end(&mut input)?;
    anyhow::ensure!(input.len() as u64 <= LIMIT, "Model exceeds 256 MiB");
    let shape = bri_gltf::read_model(&input)?;
    std::io::stdout().write_all(&bri_gltf::encode_model(&shape)?)?;
    Ok(())
}
