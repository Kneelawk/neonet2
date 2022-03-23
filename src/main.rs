fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        dotenv::dotenv().ok();
        env_logger::init();

        neonet2::flow::DesktopFlow::new()
            .title("NeoNet 2")
            .width(1920)
            .height(1080)
            .fullscreen(true)
            .start::<neonet2::neonet::NeonetApp>()
            .unwrap();
    }
}
