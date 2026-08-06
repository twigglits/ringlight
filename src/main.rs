mod app;
mod camera;
mod config;
mod cursor;
mod glow;
mod settings;

fn main() -> cosmic::iced::Result {
    env_logger::init();
    cosmic::applet::run::<app::RingLight>(())
}
