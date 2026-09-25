fn main() {
    if std::env::var("CARGO_CFG_WINDOWS").is_ok() {
        embed_resource::compile("notty.rc", embed_resource::NONE).manifest_required().unwrap();
    }
}
