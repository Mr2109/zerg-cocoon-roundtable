//! 圆桌派独立窗口（开发/测试——T8 集成后走虫族 UI 船体）
//! 用法: cargo run --bin zerg-roundtable-ui

use zerg_roundtable::ui::RoundtableApp;

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]).with_title("圆桌派"),
        ..Default::default()
    };
    eframe::run_native(
        "圆桌派",
        options,
        Box::new(|_cc| {
            zerg_roundtable::ui::setup_fonts(&_cc.egui_ctx);
            Ok(Box::new(RoundtableApp::new()))
        }),
    )
}
