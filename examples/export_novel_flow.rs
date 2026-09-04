//! 一次性导出工具：内置 novel 模板 → templates/novel.flow.json（A2 双轨）
//! 运行：cargo run --example export_novel_flow -- templates/
fn main() {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "templates".into());
    let json = zerg_roundtable::templates::novel::export_novel_flow_json();
    std::fs::create_dir_all(&dir).unwrap();
    let path = format!("{dir}/novel.flow.json");
    std::fs::write(&path, &json).unwrap();
    println!("导出: {path} ({} 字节)", json.len());
}
