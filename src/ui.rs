use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

use gpui::{
    div, prelude::*, px, rgb, size, App, Application, Bounds, Context, TitlebarOptions, Window,
    WindowBounds, WindowKind, WindowOptions,
};
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{MouseButton, TrayIconBuilder, TrayIconEvent};

use crate::portmap;
use crate::update;

enum TrayCmd {
    Show,
    Quit,
}

struct StatusView;

impl Render for StatusView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .gap_2()
            .justify_center()
            .items_center()
            .bg(rgb(0x0a0b10))
            .text_color(rgb(0xf4f5ff))
            .child(div().text_xl().child("ss-bridge"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_color(rgb(0x56d364))
                    .child(div().size_2().rounded_full().bg(rgb(0x2ecc71)))
                    .child("rodando"),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x6b7080))
                    .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
            )
            .children(portmap::state().is_closed().then(port_banner))
            .children(update::available().map(update_banner))
    }
}

fn port_banner() -> impl IntoElement {
    let detail = match portmap::state() {
        portmap::State::NoRouter => "o roteador não respondeu ao UPnP",
        _ => "o roteador não abriu a porta",
    };
    div()
        .mt_2()
        .px_3()
        .py_1()
        .rounded_md()
        .bg(rgb(0x2e2413))
        .text_sm()
        .text_color(rgb(0xf0c674))
        .child(format!("torrents lentos: {detail} (porta {})", portmap::PORT))
}

fn update_banner(version: &'static str) -> impl IntoElement {
    div()
        .id("update")
        .mt_2()
        .px_3()
        .py_1()
        .rounded_md()
        .bg(rgb(0x1b1d2e))
        .text_sm()
        .text_color(rgb(0xb8bcff))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x262947)))
        .on_click(|_, _, cx| cx.open_url(update::RELEASES))
        .child(format!("v{version} disponível · baixar"))
}

fn tray_icon() -> tray_icon::Icon {
    let img = image::load_from_memory(include_bytes!("../packaging/tray.png"))
        .expect("tray png")
        .into_rgba8();
    let (w, h) = img.dimensions();
    tray_icon::Icon::from_rgba(img.into_raw(), w, h).expect("icon")
}

fn setup_tray(tx: Sender<TrayCmd>) -> anyhow::Result<()> {
    let menu = Menu::new();
    let show = MenuItem::new("Abrir ss-bridge", true, None);
    let quit = MenuItem::new("Sair", true, None);
    menu.append(&show)?;
    menu.append(&quit)?;
    let show_id = show.id().clone();
    let quit_id = quit.id().clone();

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("ss-bridge")
        .with_icon(tray_icon())
        .build()?;
    std::mem::forget(tray);

    let menu_tx = tx.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id == quit_id {
            let _ = menu_tx.send(TrayCmd::Quit);
        } else if event.id == show_id {
            let _ = menu_tx.send(TrayCmd::Show);
        }
    }));
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if matches!(event, TrayIconEvent::DoubleClick { button: MouseButton::Left, .. }) {
            let _ = tx.send(TrayCmd::Show);
        }
    }));
    Ok(())
}

// Show or hide the Dock icon (macOS only). Closing the window drops it to the
// menu bar like Tailscale; showing it brings the Dock icon back.
fn set_dock_visible(visible: bool) {
    #[cfg(target_os = "macos")]
    unsafe {
        use objc::{class, msg_send, sel, sel_impl};
        let app: *mut objc::runtime::Object = msg_send![class!(NSApplication), sharedApplication];
        // NSApplicationActivationPolicyRegular = 0, Accessory = 1.
        let policy: isize = if visible { 0 } else { 1 };
        let _: () = msg_send![app, setActivationPolicy: policy];
    }
    let _ = visible;
}

fn open_window(cx: &mut App) {
    set_dock_visible(true);
    let bounds = Bounds::centered(None, size(px(380.), px(200.)), cx);
    let _ = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions { title: Some("ss-bridge".into()), ..Default::default() }),
            kind: WindowKind::Normal,
            is_resizable: false,
            ..Default::default()
        },
        |window, cx| {
            // Closing the window drops the app to the tray and off the Dock,
            // rather than quitting it.
            window.on_window_should_close(cx, |_window, _cx| {
                set_dock_visible(false);
                true
            });
            cx.new(|_| StatusView)
        },
    );
    cx.activate(true);
}

pub fn run() {
    Application::new().run(|cx: &mut App| {
        open_window(cx);

        let (tx, rx): (Sender<TrayCmd>, Receiver<TrayCmd>) = channel();
        if let Err(err) = setup_tray(tx) {
            eprintln!("tray: {err:#}");
        }

        let bg = cx.background_executor().clone();
        cx.spawn(async move |cx| {
            let mut announced = false;
            loop {
                if !announced && update::available().is_some() {
                    announced = true;
                    let _ = cx.update(|cx| cx.refresh_windows());
                }
                while let Ok(cmd) = rx.try_recv() {
                    match cmd {
                        TrayCmd::Show => {
                            let _ = cx.update(|cx| {
                                if cx.windows().is_empty() {
                                    open_window(cx);
                                } else {
                                    set_dock_visible(true);
                                    cx.activate(true);
                                }
                            });
                        }
                        TrayCmd::Quit => { let _ = cx.update(|cx| cx.quit()); }
                    }
                }
                bg.timer(Duration::from_millis(120)).await;
            }
        })
        .detach();
    });
}
