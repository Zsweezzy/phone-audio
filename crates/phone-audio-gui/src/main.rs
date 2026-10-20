use iced::border::Border;
use iced::font::Weight;
use iced::widget::container::Style as ContainerStyle;
use iced::widget::{column, combo_box, container, row, slider, text, toggler, Space};
use iced::{Alignment, Color, Element, Font, Length, Size, Subscription, Task, Theme};
use phone_audio_core::{App, Phone, Status};
use std::time::Duration;
fn main() -> iced::Result {
    iced::application(Gui::new, Gui::update, Gui::view)
        .title("Phone Audio")
        .theme(|_gui: &Gui| Theme::TokyoNight)
        .subscription(|gui: &Gui| {
            let status = iced::time::every(Duration::from_secs(1)).map(|_| Msg::Tick);
            if gui.busy {
                Subscription::batch([
                    status,
                    iced::time::every(Duration::from_millis(100)).map(|_| Msg::SpinnerTick),
                ])
            } else {
                status
            }
        })
        .window_size(Size::new(360.0, 260.0))
        .run()
}
struct Gui {
    app: Option<App>,
    startup_error: Option<String>,
    status: Status,
    phones: Vec<Phone>,
    combo: combo_box::State<String>,
    selection: Option<String>,
    volume: f32,
    dragging: bool,
    error: Option<String>,
    busy: bool,
    pending: Option<bool>,
    spinner_phase: usize,
}
#[derive(Debug, Clone)]
enum Msg {
    Tick,
    PhonePicked(String),
    Toggle,
    ToggleDone(std::result::Result<(), String>),
    SpinnerTick,
    VolumeChanged(f32),
    VolumeReleased,
}
const SPINNER_GLYPHS: [char; 4] = ['◜', '◝', '◞', '◟'];
impl Gui {
    fn new() -> Self {
        let (app, startup_error) = match App::new() {
            Ok(app) => (Some(app), None),
            Err(e) => (None, Some(e.to_string())),
        };
        Self {
            app,
            startup_error,
            status: Status::default(),
            phones: Vec::new(),
            combo: combo_box::State::new(Vec::new()),
            selection: None,
            volume: 50.0,
            dragging: false,
            error: None,
            busy: false,
            pending: None,
            spinner_phase: 0,
        }
    }
    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Tick => {
                self.refresh();
                Task::none()
            }
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
                Task::none()
            }
            Msg::Toggle => {
                if self.busy {
                    return Task::none();
                }
                let Some(app) = self.app.clone() else {
                    return Task::none();
                };
                let on = self.status.on;
                self.pending = Some(!self.pending.unwrap_or(self.status.on));
                self.busy = true;
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let mut app = app;
                            if on {
                                app.turn_off().map_err(|e| e.to_string())
                            } else {
                                app.turn_on().map_err(|e| e.to_string())
                            }
                        })
                        .await
                        .map_err(|e| format!("toggle worker failed: {e}"))?
                    },
                    Msg::ToggleDone,
                )
            }
            Msg::ToggleDone(result) => {
                self.pending = None;
                self.busy = false;
                match result {
                    Ok(()) => self.error = None,
                    Err(e) => self.error = Some(e),
                }
                self.refresh();
                Task::none()
            }
            Msg::SpinnerTick => {
                self.spinner_phase = (self.spinner_phase + 1) % SPINNER_GLYPHS.len();
                Task::none()
            }
            Msg::VolumeChanged(v) => {
                self.dragging = true;
                self.volume = v;
                Task::none()
            }
            Msg::VolumeReleased => {
                self.dragging = false;
                if let Some(app) = self.app.as_mut() {
                    match app.set_volume(f64::from(self.volume)) {
                        Ok(()) => self.error = None,
                        Err(e) => self.error = Some(e.to_string()),
                    }
                }
                Task::none()
            }
        }
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
                self.phones = s.phones.clone();
                self.status = s;
                self.error = None;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
        let options: Vec<String> = self.phones.iter().map(|p| p.to_string()).collect();
        if !options.is_empty()
            && self
                .selection
                .as_ref()
                .is_none_or(|sel| !options.contains(sel))
        {
            self.selection = Some(options[0].clone());
        }
        if self.combo.options() != options.as_slice() {
            self.combo = combo_box::State::with_selection(options, self.selection.as_ref());
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
        let mut tgl = toggler(self.pending.unwrap_or(self.status.on))
            .label("On main speakers")
            .size(15.0)
            .spacing(8);
        if self.status.available {
            tgl = tgl.on_toggle(|_| Msg::Toggle);
        }
        let status_text = if self.busy {
            "working…".to_string()
        } else if self.status.on {
            "Routing phone audio to this PC".to_string()
        } else if !self.status.reason.is_empty() {
            self.status.reason.clone()
        } else {
            "Off".to_string()
        };
        let has_error = self.error.is_some() || self.startup_error.is_some();
        let dot_color = if has_error {
            RED
        } else if self.busy {
            CYAN
        } else if self.status.on {
            GREEN
        } else {
            MUTED
        };
        let status_color = if has_error {
            RED
        } else if self.status.on || self.busy {
            CYAN
        } else {
            MUTED
        };
        let (pill_text, pill_color, pill_bg) = if self.busy {
            ("working…", CYAN, CYAN_TINT)
        } else if self.status.on {
            ("routing to PC", GREEN, GREEN_TINT)
        } else {
            ("off", MUTED, CARD_BG)
        };
        let header = row![
            text("●").size(9).color(dot_color),
            text("Phone Audio").size(14).color(FG).font(BOLD),
            Space::new().width(Length::Fill),
            container(text(pill_text).size(11).color(pill_color))
                .padding([6, 12])
                .style(move |_| pill(pill_bg)),
        ]
        .align_y(Alignment::Center)
        .spacing(8);
        let profile = self.status.profile.as_deref().unwrap_or("-");
        let profile_color = if self.status.on { CYAN } else { MUTED };
        let phone_card = container(
            column![
                text("Phone").size(11).color(MUTED),
                row![
                    picker,
                    container(text(profile).size(10).color(profile_color))
                        .padding([4, 10])
                        .style(|_| pill(CHIP_BG)),
                ]
                .spacing(8),
            ]
            .spacing(6)
            .width(Length::Fill),
        )
        .padding(10)
        .style(|_| card());
        let mut toggle_row = row![tgl];
        if self.busy {
            toggle_row = toggle_row.push(
                text(SPINNER_GLYPHS[self.spinner_phase])
                    .size(14)
                    .color(CYAN),
            );
        }
        let mut col = column![header, phone_card, toggle_row].spacing(10);
        if self.status.on {
            col = col.push(row![
                slider(0.0..=100.0, self.volume, Msg::VolumeChanged)
                    .on_release(Msg::VolumeReleased)
                    .width(Length::Fill),
                text(format!("{:.0}%", self.volume))
                    .width(Length::Shrink)
                    .color(FG),
            ]);
        }
        col = col.push(text(status_text).size(11).color(status_color));
        if let Some(e) = self.error.as_ref().or(self.startup_error.as_ref()) {
            col = col.push(text(e).size(11).color(RED));
        }
        container(col).padding(14).into()
    }
}
const FG: Color = Color::from_rgba8(0xc0, 0xca, 0xf5, 1.0);
const MUTED: Color = Color::from_rgba8(0x73, 0x7a, 0xa2, 1.0);
const CARD_BG: Color = Color::from_rgba8(0x29, 0x2e, 0x42, 1.0);
const BORDER: Color = Color::from_rgba8(0x3b, 0x42, 0x61, 1.0);
const CHIP_BG: Color = Color::from_rgba8(0x1f, 0x23, 0x35, 1.0);
const CYAN: Color = Color::from_rgba8(0x7d, 0xcf, 0xff, 1.0);
const GREEN: Color = Color::from_rgba8(0x9e, 0xce, 0x6a, 1.0);
const RED: Color = Color::from_rgba8(0xf7, 0x76, 0x8e, 1.0);
const GREEN_TINT: Color = Color::from_rgba8(0x9e, 0xce, 0x6a, 0.15);
const CYAN_TINT: Color = Color::from_rgba8(0x7d, 0xcf, 0xff, 0.15);
const BOLD: Font = Font {
    weight: Weight::Bold,
    ..Font::DEFAULT
};
fn card() -> ContainerStyle {
    ContainerStyle::default()
        .background(CARD_BG)
        .border(Border::default().rounded(10.0).width(1.0).color(BORDER))
}
fn pill(bg: Color) -> ContainerStyle {
    ContainerStyle::default()
        .background(bg)
        .border(Border::default().rounded(999.0))
}
