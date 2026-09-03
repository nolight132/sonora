use tokio::sync::mpsc::UnboundedSender;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use super::{Event, Shown};

const TOOLTIP: &str = "Sonora";
const PNG: &[u8] = match cfg!(target_os = "macos") {
    true => include_bytes!("../../../../assets/tray/template-64.png"),
    false => include_bytes!("../../../../assets/tray/sonora.png"),
};
const MENU_ON_CLICK: bool = cfg!(target_os = "macos");

pub struct Icon {
    _icon: TrayIcon,
    caption: MenuItem,
    toggle: MenuItem,
    previous: MenuItem,
    next: MenuItem,
    show: MenuItem,
    quit: MenuItem,
}

impl Icon {
    pub fn new(sender: UnboundedSender<Event>) -> Option<Self> {
        let image = match image::load_from_memory(PNG) {
            Ok(image) => image.into_rgba8(),
            Err(error) => {
                log::warn!("tray: cannot decode the tray icon: {error:#}");
                return None;
            }
        };
        let (width, height) = image.dimensions();
        let icon = match tray_icon::Icon::from_rgba(image.into_raw(), width, height) {
            Ok(icon) => icon,
            Err(error) => {
                log::warn!("tray: cannot build the tray icon: {error:#}");
                return None;
            }
        };

        let caption = MenuItem::with_id("caption", "", false, None);
        let toggle = MenuItem::with_id("toggle", "", true, None);
        let previous = MenuItem::with_id("previous", "", true, None);
        let next = MenuItem::with_id("next", "", true, None);
        let show = MenuItem::with_id("show", "", true, None);
        let quit = MenuItem::with_id("quit", "", true, None);
        let menu = Menu::new();
        if let Err(error) = menu.append_items(&[
            &caption,
            &PredefinedMenuItem::separator(),
            &toggle,
            &previous,
            &next,
            &PredefinedMenuItem::separator(),
            &show,
            &quit,
        ]) {
            log::warn!("tray: cannot build the tray menu: {error:#}");
            return None;
        }

        let built = TrayIconBuilder::new()
            .with_icon(icon)
            .with_icon_as_template(true)
            .with_tooltip(TOOLTIP)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(MENU_ON_CLICK)
            .build();
        let icon = match built {
            Ok(icon) => icon,
            Err(error) => {
                log::warn!("tray: cannot place the tray icon: {error:#}");
                return None;
            }
        };

        let menus = sender.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if let Some(event) = translate(&event.id) {
                menus.send(event).ok();
            }
        }));
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            else {
                return;
            };
            if !MENU_ON_CLICK {
                sender.send(Event::Show).ok();
            }
        }));

        Some(Self {
            _icon: icon,
            caption,
            toggle,
            previous,
            next,
            show,
            quit,
        })
    }

    pub fn show(&mut self, shown: &Shown) {
        self.caption.set_text(&shown.caption);
        self.toggle.set_text(&shown.toggle);
        self.previous.set_text(&shown.previous);
        self.next.set_text(&shown.next);
        self.show.set_text(&shown.show);
        self.quit.set_text(&shown.quit);
    }
}

fn translate(id: &MenuId) -> Option<Event> {
    Some(match id.0.as_str() {
        "toggle" => Event::Toggle,
        "previous" => Event::Previous,
        "next" => Event::Next,
        "show" => Event::Show,
        "quit" => Event::Quit,
        _ => return None,
    })
}
