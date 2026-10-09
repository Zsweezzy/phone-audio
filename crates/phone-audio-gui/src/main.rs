//! phone-audio GUI — a small always-on-top-style window to route a Bluetooth
//! phone's audio to this PC. Polls status every second via a subscription.

use std::time::Duration;

use iced::widget::{column, combo_box, container, row, slider, text, toggler};
use iced::{Element, Length, Size, Task, Theme};
use phone_audio_core::{App, Phone, Status};

fn main() -> iced::Result {
    iced::application(Gui::new, Gui::update, Gui::view)
        .title("Phone Audio")
        .theme(|_gui: &Gui| Theme::TokyoNight)
        .subscription(|_gui: &Gui| iced::time::every(Duration::from_secs(1)).map(|_| Msg::Tick))
        .window_size(Size::new(340.0, 200.0))
        .run()
}

struct Gui {
    app: Option<App>,
    status: Status,
    phones: Vec<Phone>,
    combo: combo_box::State<String>,
    selection: Option<String>,
    volume: f32,
    dragging: bool,
    error: Option<String>,
}

#[derive(Debug, Clone)]
enum Msg {
    Tick,
    PhonePicked(String),
    Toggle,
    VolumeChanged(f32),
    VolumeReleased,
}

impl Gui {
    fn new() -> Self {
        Self {
            app: App::new().ok(),
            status: Status::default(),
            phones: Vec::new(),
            combo: combo_box::State::new(Vec::new()),
            selection: None,
            volume: 50.0,
            dragging: false,
            error: None,
        }
    }

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Tick => self.refresh(),
            Msg::PhonePicked(label) => {
                self.selection = Some(label.clone());
                if let Some(app) = self.app.as_mut() {
                    if let Some(p) = self.phones.iter().find(|p| p.to_string() == label) {
                        match app.set_phone(&p.mac) {
                            Ok(_) => self.error = None,
                            Err(e) => self.error = Some(e.to_string()),
                        }
                    }
                }
                self.refresh();
            }
            Msg::Toggle => {
                if let Some(app) = self.app.as_mut() {
                    let r = if self.status.on {
                        app.turn_off()
                    } else {
                        app.turn_on()
                    };
                    match r {
                        Ok(()) => self.error = None,
                        Err(e) => self.error = Some(e.to_string()),
                    }
                }
                self.refresh();
            }
            Msg::VolumeChanged(v) => {
                self.dragging = true;
                self.volume = v;
            }
            Msg::VolumeReleased => {
                self.dragging = false;
                if let Some(app) = self.app.as_mut() {
                    match app.set_volume(f64::from(self.volume)) {
                        Ok(()) => self.error = None,
                        Err(e) => self.error = Some(e.to_string()),
                    }
                }
            }
        }
        Task::none()
    }

    fn refresh(&mut self) {
        let Some(app) = self.app.as_mut() else { return };
        match app.status() {
            Ok(s) => {
                if !self.dragging {
                    if let Some(v) = s.volume {
                        self.volume = v as f32;
                    }
                }
                self.status = s;
                self.error = None;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
        match app.list_phones() {
            Ok(phones) => {
                let options: Vec<String> = phones.iter().map(|p| p.to_string()).collect();
                // Rebuild only on an actual change so an open dropdown survives ticks.
                if self.combo.options() != options.as_slice() {
                    self.combo = combo_box::State::with_selection(options, self.selection.as_ref());
                }
                self.phones = phones;
            }
            Err(e) if self.error.is_none() => self.error = Some(e.to_string()),
            Err(_) => {}
        }
    }

    fn view(&self) -> Element<'_, Msg> {
        let picker = combo_box(
            &self.combo,
            "Select phone…",
            self.selection.as_ref(),
            Msg::PhonePicked,
        )
        .width(Length::Fill);

        let mut tgl = toggler(self.status.on).label("On main speakers");
        if self.status.available {
            tgl = tgl.on_toggle(|_| Msg::Toggle);
        }
        let profile = self.status.profile.as_deref().unwrap_or("-");
        let toggle_row = row![tgl, text(profile).size(12)];

        let volume_row = row![
            slider(0.0..=100.0, self.volume, Msg::VolumeChanged)
                .on_release(Msg::VolumeReleased)
                .width(Length::Fill),
            text(format!("{:.0}%", self.volume)).width(Length::Shrink)
        ];

        let status_text = if self.status.on {
            "Routing phone audio to this PC".to_string()
        } else if !self.status.reason.is_empty() {
            self.status.reason.clone()
        } else {
            "Off".to_string()
        };

        let mut col =
            column![picker, toggle_row, volume_row, text(status_text).size(12),].spacing(8);

        if let Some(e) = &self.error {
            col = col.push(
                text(e)
                    .size(11)
                    .color(iced::Color::from_rgba(0.9, 0.35, 0.35, 1.0)),
            );
        }

        container(col).padding(10).into()
    }
}
