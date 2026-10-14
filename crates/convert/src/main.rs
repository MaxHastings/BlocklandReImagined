fn main() -> anyhow::Result<()> {
    bri_convert::convert_root::run(std::env::args_os().skip(1).collect())
}
