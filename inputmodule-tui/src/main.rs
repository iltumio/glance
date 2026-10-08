mod api;
mod app;
mod composer;
mod event;
mod features;
mod games;
mod inputs;
mod render;
mod serial;
mod tui;
mod widgets;

use app::App;
use clap::Parser;
use event::{AppEvent, EventHandler};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use std::time::Instant;
use tui::Tui;

#[derive(Debug, Parser)]
#[command(
    version,
    about = "Games and visualizations for Framework LED Matrix input modules"
)]
struct Args {
    /// Serial port (e.g. /dev/ttyACM0, /dev/serial/by-id/... or COM3)
    #[arg(long, conflicts_with = "no_device")]
    serial_dev: Option<String>,
    /// Run without enumerating or connecting to hardware
    #[arg(long, conflicts_with = "list")]
    no_device: bool,
    /// List compatible LED Matrix devices and exit
    #[arg(short, long)]
    list: bool,
    /// Load a widget layout template (JSON)
    #[arg(long, conflicts_with_all = ["list", "examples"])]
    template: Option<PathBuf>,
    /// File written by the editor's s key (default: loaded template or layout.json)
    #[arg(long, conflicts_with_all = ["headless", "validate_template", "list", "examples"])]
    save_template: Option<PathBuf>,
    /// Play a template without terminal UI; Ctrl-C stops and restores the device
    #[arg(long, requires = "template", conflicts_with_all = ["list", "examples", "validate_template"])]
    headless: bool,
    /// Check a template and exit without opening a terminal or serial connection
    #[arg(long, requires = "template", conflicts_with_all = ["headless", "list", "examples"])]
    validate_template: bool,
    /// Stop headless playback after this many seconds
    #[arg(long, requires = "headless", value_parser = clap::value_parser!(u64).range(1..=86400))]
    duration: Option<u64>,
    /// Open the original standalone examples menu instead of the widget editor
    #[arg(long)]
    examples: bool,
    /// JSON-RPC listen address; starts automatically for API inputs (default 127.0.0.1:3030)
    #[arg(long, conflicts_with_all = ["list", "examples", "validate_template"])]
    api_listen: Option<SocketAddr>,
}

fn main() -> std::io::Result<()> {
    let args = Args::parse();
    if args.list {
        for device in serial::device::list_devices()? {
            println!(
                "{}\n  Serial: {}\n  Product: {}",
                device.port,
                device.serial_number.as_deref().unwrap_or("?"),
                device.product.as_deref().unwrap_or("LED Matrix")
            );
        }
        return Ok(());
    }
    let template = if let Some(path) = &args.template {
        widgets::Template::load(path)?
    } else {
        widgets::Template::default()
    };
    if args.validate_template {
        println!(
            "Valid template: {} widgets on {}x{} LEDs",
            template.widgets.len(),
            template.canvas.width,
            template.canvas.height
        );
        return Ok(());
    }
    let mut app = App::new(args.no_device, args.serial_dev, Instant::now());
    if !args.examples {
        let path = args
            .save_template
            .or(args.template)
            .unwrap_or_else(|| "layout.json".into());
        app.enable_composer(template, path)?;
    }
    let mut api = app.composer.as_ref().map(|composer| {
        api::ApiManager::new(
            composer.runtime.input_hub(),
            args.api_listen
                .unwrap_or_else(|| "127.0.0.1:3030".parse().unwrap()),
            args.api_listen.is_some(),
        )
    });
    if let Some(api) = &mut api {
        api.ensure()?;
    }
    app.discover_device();
    if args.headless {
        if let Some(api) = &api {
            eprintln!("{}", api.status());
        }
        return run_headless(&mut app, args.duration);
    }
    let mut tui = Tui::init()?;
    let result = run(&mut app, &mut tui, &mut api);
    drop(api);
    drop(app);
    result
}

fn run(app: &mut App, tui: &mut Tui, api: &mut Option<api::ApiManager>) -> std::io::Result<()> {
    let events = EventHandler;
    while app.running {
        if let (Some(api), Some(composer)) = (api.as_mut(), app.composer.as_mut()) {
            if let Err(error) = api.ensure() {
                composer.notice = format!("API startup failed: {error}");
            }
            composer.api_status = api.status();
        }
        app.update(Instant::now());
        tui.draw(app)?;
        match events.next(app.poll_timeout(Instant::now()))? {
            AppEvent::Key(key) => app.handle_key(key, Instant::now()),
            AppEvent::Mouse(mouse) => {
                let (width, height) = crossterm::terminal::size()?;
                if let Some(composer) = &mut app.composer {
                    render::composer::handle_mouse(
                        composer,
                        mouse,
                        ratatui::layout::Rect::new(0, 0, width, height),
                    );
                }
            }
            AppEvent::Wake => {}
        }
    }
    Ok(())
}

fn run_headless(app: &mut App, duration: Option<u64>) -> std::io::Result<()> {
    let running = Arc::new(AtomicBool::new(true));
    let signal_running = running.clone();
    ctrlc::set_handler(move || signal_running.store(false, Ordering::SeqCst))
        .map_err(std::io::Error::other)?;
    let started = Instant::now();
    eprintln!("Playing template without TUI; Ctrl-C to stop");
    let mut previous_status = String::new();
    while running.load(Ordering::SeqCst) {
        let now = Instant::now();
        if duration
            .is_some_and(|seconds| now.duration_since(started) >= Duration::from_secs(seconds))
        {
            break;
        }
        app.update(now);
        let mut status = app.device_status();
        if let Some(composer) = &app.composer {
            for error in composer.runtime.input_errors() {
                status.push_str(&format!("\n{error}"));
            }
        }
        if status != previous_status {
            eprintln!("{status}");
            previous_status = status;
        }
        std::thread::sleep(app.poll_timeout(Instant::now()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cli_conflicts_are_validated_before_terminal_init() {
        assert!(Args::try_parse_from(["tui", "--no-device", "--serial-dev", "COM3"]).is_err());
        assert!(Args::try_parse_from(["tui", "--no-device", "--list"]).is_err());
        assert!(Args::try_parse_from(["tui", "--headless"]).is_err());
        assert!(Args::try_parse_from(["tui", "--duration", "1"]).is_err());
        assert!(Args::try_parse_from([
            "tui",
            "--template",
            "layout.json",
            "--headless",
            "--examples"
        ])
        .is_err());
        assert!(Args::try_parse_from([
            "tui",
            "--template",
            "layout.json",
            "--headless",
            "--duration",
            "0"
        ])
        .is_err());
        assert_eq!(
            Args::try_parse_from(["tui", "--serial-dev", "COM3"])
                .unwrap()
                .serial_dev
                .as_deref(),
            Some("COM3")
        );
    }
}
