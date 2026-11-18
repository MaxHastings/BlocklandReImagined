//! Diagnostic: run the DTS v24 reader over given files and print summary
//! stats or full error chains; used to compare stock vs community exports.

fn main() {
    let mut failed = false;
    for arg in std::env::args().skip(1) {
        match std::fs::read(&arg) {
            Err(e) => {
                println!("{arg}: READ ERROR {e}");
                failed = true;
            }
            Ok(data) => match bri_convert::shape::read_dts(&data, "probe".into()) {
                Ok((shape, prov)) => {
                    let json = serde_json::to_string(&shape).unwrap_or_default();
                    println!(
                        "{arg}: OK exporter={} nodes={} objects={} meshes={}/{} materials={} animations={} json_bytes={}",
                        prov.exporter,
                        shape.nodes.len(),
                        shape.objects.len(),
                        shape.meshes.iter().filter(|m| m.is_some()).count(),
                        shape.meshes.len(),
                        shape.materials.len(),
                        shape.animations.len(),
                        json.len()
                    );
                }
                Err(e) => {
                    println!("{arg}: ERR\n{e:#}");
                    failed = true;
                }
            },
        }
    }
    std::process::exit(i32::from(failed));
}
