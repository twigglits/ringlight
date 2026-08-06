mod app;
mod camera;
mod cursor;
mod glow;
mod settings;

fn main() -> cosmic::iced::Result {
    env_logger::init();
    cosmic::applet::run::<app::RingLight>(())
}
