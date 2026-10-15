fn main() -> anyhow::Result<()> {
    bri_convert::map_bundle::run(std::env::args().skip(1).collect())
}
