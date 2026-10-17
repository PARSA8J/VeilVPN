//! VeilApp — the liquid-glass native UI.

use crate::engine::{Cmd, Engine, Phase, Shared};
use crate::i18n::I18nState;
use crate::license::{self, LicenseState, Plan};
use crate::store::Config;
use crate::theme::*;
use crate::uri::Server;
use egui::{
    Align, Align2, Color32, Context, FontData, FontDefinitions, FontFamily, Frame, Layout,
    Pos2, Rect, RichText, ScrollArea, Sense, Stroke, Vec2,
};
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub struct VeilApp {
    pub i18n: I18nState,
    pub config: Config,
    pub engine: Engine,
    pub shared: Arc<Shared>,
    pub license: Option<license::License>,
    pub lic_state: LicenseState,

    pub selected_key: String,
    pub show_settings: bool,
    pub gate: bool,
    pub key_input: String,
    pub sub_input: String,
    pub toast: Option<(String, f64, bool)>, // text, shown_at_sec, is_error
    pub acrylic_tried: u32,
    pub acrylic_done: bool,
}

impl VeilApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_fonts(&cc.egui_ctx);
        let config = Config::load();
        let mut i18n = I18nState::new(config.lang.clone());
        i18n.lang = config.lang.clone();

        let shared = Shared::new();
        // seed servers (always available)
        let seed = crate::i18n::parse_seed();
        let n = seed.len();
        *shared.servers.lock().unwrap() = seed;
        *shared.pings.lock().unwrap() = vec![u32::MAX; n];
        shared.ctx.lock().unwrap().replace(cc.egui_ctx.clone());

        let engine = Engine::start(shared.clone());

        // license: load or issue trial
        let mut lic = license::load();
        if lic.is_none() {
            let l = license::issue_trial();
            let _ = license::save(&l);
            lic = Some(l);
        }
        let lic_state = license::evaluate(&lic);

        // background: ping + live fetch shortly after boot
        {
            let shared2 = shared.clone();
            let custom = if config.custom_source.trim().is_empty() { None } else { Some(config.custom_source.clone()) };
            let tx = engine.tx.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(600));
                let _ = tx.send(Cmd::PingAll);
                std::thread::sleep(std::time::Duration::from_millis(1600));
                let _ = tx.send(Cmd::FetchSources { urls: vec![], custom });
            });
        }

        let app = Self {
            i18n,
            selected_key: config.selected_key.clone(),
            show_settings: false,
            gate: !lic_state.valid,
            key_input: String::new(),
            sub_input: config.custom_source.clone(),
            toast: None,
            acrylic_tried: 0,
            acrylic_done: false,
            license: lic,
            lic_state,
            config,
            engine,
            shared,
        };
        app
    }

    fn now_s(&self) -> f64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0)
    }

    fn toast(&mut self, text: String, err: bool) {
        self.toast = Some((text, self.now_s(), err));
    }

    fn selected_server(&self) -> Option<Server> {
        let servers = self.shared.servers.lock().unwrap();
        if let Some(i) = servers.iter().position(|s| s.key() == self.selected_key) {
            return Some(servers[i].clone());
        }
        None
    }

    fn try_acrylic(&mut self, ctx: &Context) {
        if self.acrylic_done || self.acrylic_tried > 90 {
            return;
        }
        self.acrylic_tried += 1;
        if crate::sysproxy::apply_acrylic("VeilVPN") {
            self.acrylic_done = true;
            ctx.request_repaint();
        }
    }
}

fn setup_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("vazir".into(), Arc::new(FontData::from_static(crate::i18n::FONT_VAZIR)));
    fonts.font_data.insert("notocjk".into(), Arc::new(FontData::from_static(crate::i18n::FONT_CJK)));
    for fam in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts
            .families
            .entry(fam.clone())
            .or_default()
            .push("vazir".into());
        fonts.families.entry(fam).or_default().push("notocjk".into());
    }
    ctx.set_fonts(fonts);
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::light();
    style.visuals.panel_fill = Color32::TRANSPARENT;
    style.visuals.window_fill = Color32::TRANSPARENT;
    style.visuals.extreme_bg_color = Color32::from_white_alpha(90);
    style.visuals.widgets.inactive.bg_fill = Color32::from_white_alpha(120);
    style.visuals.widgets.hovered.bg_fill = Color32::from_white_alpha(220);
    style.visuals.selection.stroke = Stroke::new(1.5, c_accent());
    style.spacing.item_spacing = Vec2::new(8.0, 8.0);
    ctx.set_style(style);
}

impl eframe::App for VeilApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.try_acrylic(ctx);

        // periodic repaint for timer / animations
        let st = self.shared.status();
        if matches!(st.phase, Phase::Connected | Phase::Connecting | Phase::Disconnecting) {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }

        // toast expiry
        if let Some((_, t0, _)) = self.toast {
            if self.now_s() - t0 > 3.0 {
                self.toast = None;
            }
        }

        egui::CentralPanel::default()
            .frame(Frame::none().fill(Color32::TRANSPARENT))
            .show(ctx, |ui| {
                let avail = ui.available_size();
                let (bg_rect, _) = ui.allocate_exact_size(avail, Sense::hover());
                if !self.acrylic_done {
                    paint_bg(ui.painter().clone(), bg_rect);
                } else {
                    // light frost layer over the acrylic blur
                    ui.painter().rect_filled(bg_rect, 0.0, Color32::from_white_alpha(70));
                }
                self.ui_body(ui, ctx);
                self.ui_toast(ui);
            });
    }
}

impl VeilApp {
    fn ui_body(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        let rtl = self.i18n.is_rtl();
        let pad = 16.0;
        let mut area = ui.available_rect_before_wrap();
        area.min += Vec2::splat(pad);
        area.max -= Vec2::new(pad, pad * 0.6);
        let ui_w = area.width();

        // ---- titlebar (custom, undecorated window) ----
        {
            let tb_h = 34.0;
            let tb = Rect::from_min_size(area.min, Vec2::new(ui_w, tb_h));
            let resp = ui.allocate_rect(tb, Sense::click_and_drag());
            if resp.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if resp.double_clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!ctx.input(|i| i.viewport().maximized.unwrap_or(false))));
            }
            let p = ui.painter().clone();
            // brand dot
            let dot_c = Pos2::new(if rtl { tb.right() - 12.0 } else { tb.left() + 12.0 }, tb.center().y);
            p.clone().circle_filled(dot_c, 7.0, c_accent());
            p.clone().circle_filled(dot_c + Vec2::new(-2.0, -2.5), 2.5, Color32::from_white_alpha(160));
            p.clone().text(
                Pos2::new(dot_c.x + if rtl { -18.0 } else { 18.0 }, tb.center().y),
                if rtl { Align2::RIGHT_CENTER } else { Align2::LEFT_CENTER },
                "VeilVPN",
                egui::FontId::proportional(15.0),
                c_ink(),
            );
            // window buttons (right side; left in rtl)
            let close_c = Pos2::new(if rtl { tb.left() + 16.0 } else { tb.right() - 16.0 }, tb.center().y);
            let min_c = Pos2::new(if rtl { tb.left() + 46.0 } else { tb.right() - 46.0 }, tb.center().y);
            let hover_close = close_c.distance(ctx.input(|i| i.pointer.hover_pos().unwrap_or(Pos2::ZERO))) < 12.0;
            glass_circle(p.clone(), close_c, 11.0, hover_close, if hover_close { Some(Color32::from_rgb(0xEF, 0x44, 0x44)) } else { None });
            p.clone().text(close_c, Align2::CENTER_CENTER, "✕", egui::FontId::proportional(9.0), Color32::from_white_alpha(240));
            let hover_min = min_c.distance(ctx.input(|i| i.pointer.hover_pos().unwrap_or(Pos2::ZERO))) < 12.0;
            glass_circle(p.clone(), min_c, 11.0, hover_min, None);
            p.clone().line_segment([min_c - Vec2::new(4.0, 0.0), min_c + Vec2::new(4.0, 0.0)], Stroke::new(1.6, c_ink()));

            if resp.clicked() {
                let pos = ctx.input(|i| i.pointer.interact_pos().unwrap_or(Pos2::ZERO));
                if close_c.distance(pos) < 12.0 {
                    // restore system proxy if we own it, then close
                    if matches!(self.shared.status().phase, Phase::Connected) {
                        let _ = self.engine.tx.send(Cmd::Disconnect);
                        std::thread::sleep(std::time::Duration::from_millis(350));
                    }
                    self.engine.shutdown();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                } else if min_c.distance(pos) < 12.0 {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }
            }
        }

        let mut y = area.min.y + 34.0 + 6.0;

        // ---- header: status pill + settings ----
        {
            let h = 30.0;
            let st = self.shared.status();
            let (label, col) = match st.phase {
                Phase::Connected => (self.i18n.t("connected"), c_success()),
                Phase::Connecting => (self.i18n.t("connecting"), c_warn()),
                Phase::Disconnecting => (self.i18n.t("disconnecting"), c_warn()),
                Phase::Error(_) => (self.i18n.t("conn_fail"), c_danger()),
                Phase::Idle => (self.i18n.t("disconnected"), c_sub()),
            };
            let pill_w = 118.0;
            let pill = Rect::from_min_size(
                Pos2::new(if rtl { area.right() - pill_w } else { area.left() }, y),
                Vec2::new(pill_w, h),
            );
            let p = ui.painter().clone();
            p.clone().rect_filled(pill, rad(h / 2.0), Color32::from_white_alpha(110));
            p.clone().rect_stroke(pill, rad(h / 2.0), Stroke::new(1.0, Color32::from_white_alpha(170)), egui::StrokeKind::Inside);
            let dot = Pos2::new(if rtl { pill.right() - 16.0 } else { pill.left() + 16.0 }, pill.center().y);
            let pulse = ((self.now_s() * 2.0).sin() * 0.5 + 0.5) as f32;
            p.clone().circle_filled(dot, 4.0 + if matches!(st.phase, Phase::Connected) { pulse * 1.5 } else { 0.0 }, col);
            p.clone().text(
                Pos2::new(if rtl { pill.right() - 26.0 } else { pill.left() + 26.0 }, pill.center().y),
                if rtl { Align2::RIGHT_CENTER } else { Align2::LEFT_CENTER },
                label,
                egui::FontId::proportional(12.0),
                c_ink(),
            );
            // settings glass circle
            let gear_c = Pos2::new(if rtl { area.left() + 16.0 } else { area.right() - 16.0 }, y + h / 2.0);
            let gear_resp = ui.allocate_rect(Rect::from_center_size(gear_c, Vec2::splat(30.0)), Sense::click());
            glass_circle(p.clone(), gear_c, 15.0, gear_resp.hovered(), None);
            p.clone().text(gear_c, Align2::CENTER_CENTER, "⚙", egui::FontId::proportional(13.0), c_ink());
            if gear_resp.clicked() {
                self.show_settings = !self.show_settings;
            }
            y += h + 10.0;
        }

        // ---- connect zone ----
        {
            let st = self.shared.status();
            let zone_h = 150.0;
            let zone = Rect::from_min_size(Pos2::new(area.left(), y), Vec2::new(ui_w, zone_h));
            let p = ui.painter().clone();

            // title + sub
            let (title, tcol) = match st.phase {
                Phase::Connected => (self.i18n.t("connected"), c_success()),
                Phase::Connecting => (self.i18n.t("connecting"), c_warn()),
                Phase::Disconnecting => (self.i18n.t("disconnecting"), c_warn()),
                Phase::Error(ref e) => (format!("{} — {}", self.i18n.t("conn_fail"), e.split(':').next().unwrap_or("")), c_danger()),
                Phase::Idle => (self.i18n.t("tap_connect"), c_ink()),
            };
            p.clone().text(
                Pos2::new(area.center().x, zone.top() + 14.0),
                Align2::CENTER_CENTER,
                title,
                egui::FontId::proportional(19.0),
                tcol,
            );
            let sub = match (&st.phase, &st.active) {
                (Phase::Connected, Some(s)) => format!("{} · {}", s.remark, self.i18n.t("protected")),
                (Phase::Connecting, Some(s)) => format!("{}…", s.remark),
                _ => self.i18n.t("slogan"),
            };
            p.clone().text(
                Pos2::new(area.center().x, zone.top() + 36.0),
                Align2::CENTER_CENTER,
                sub,
                egui::FontId::proportional(11.5),
                c_sub(),
            );

            // big pill button
            let btn_w = 240.0_f32.min(ui_w - 24.0);
            let btn_h = 62.0;
            let btn = Rect::from_center_size(Pos2::new(area.center().x, zone.top() + 86.0), Vec2::new(btn_w, btn_h));
            let btn_resp = ui.allocate_rect(btn, Sense::click());
            let hovered = btn_resp.hovered();
            let now = self.now_s();

            let (base, glow, label, icon_col) = match st.phase {
                Phase::Connected => (c_success().gamma_multiply(0.92), Some(Color32::from_rgba_unmultiplied(16, 185, 129, 90)), self.i18n.t("disconnecting").replace("…", ""), Color32::WHITE),
                Phase::Connecting | Phase::Disconnecting => {
                    let breathe = ((now * 3.0).sin() * 0.5 + 0.5) as f32;
                    (
                        Color32::from_rgb(0xF5, 0x9E, 0x0B).gamma_multiply(0.75 + breathe * 0.25),
                        Some(Color32::from_rgba_unmultiplied(245, 158, 11, 80)),
                        self.i18n.t("connecting").replace("…", ""),
                        Color32::WHITE,
                    )
                }
                Phase::Idle | Phase::Error(_) => (c_charcoal(), None, self.i18n.t("tap_connect").to_uppercase(), Color32::WHITE),
            };
            let base2 = if hovered { base.gamma_multiply(1.08) } else { base };
            gloss_pill(p.clone(), btn, base2, glow);

            // icon circle inside pill
            let ic_c = Pos2::new(if rtl { btn.right() - 36.0 } else { btn.left() + 36.0 }, btn.center().y);
            p.clone().circle_filled(ic_c, 19.0, Color32::from_white_alpha(50));
            if matches!(st.phase, Phase::Connecting | Phase::Disconnecting) {
                power_icon(p.clone(), ic_c, 11.0, icon_col, 2.6, ((now * 1.5) % 0.85 + 0.12) as f32);
            } else {
                power_icon(p.clone(), ic_c, 11.0, icon_col, 2.6, 0.8);
            }
            // label
            let tx = if rtl { btn.left() + 22.0 } else { btn.right() - 22.0 };
            p.clone().text(
                Pos2::new(tx, btn.center().y),
                if rtl { Align2::LEFT_CENTER } else { Align2::RIGHT_CENTER },
                label,
                egui::FontId::proportional(17.5),
                icon_col,
            );

            if btn_resp.clicked() {
                self.on_connect_click();
            }

            // ip line
            let mut ip_txt = String::new();
            if let Some(ip) = &st.ip {
                ip_txt.push_str(&format!("IP {}", ip));
            }
            if let Some(s) = &st.active {
                if !ip_txt.is_empty() {
                    ip_txt.push_str("  ·  ");
                }
                ip_txt.push_str(&format!("{} {}", self.i18n.t("via"), s.remark));
            }
            if let (Phase::Connected, Some(since)) = (&st.phase, st.since) {
                if let Ok(d) = std::time::SystemTime::now().duration_since(since) {
                    let secs = d.as_secs();
                    if !ip_txt.is_empty() {
                        ip_txt.push_str("  ·  ");
                    }
                    ip_txt.push_str(&format!("{:02}:{:02}:{:02}", secs / 3600, (secs % 3600) / 60, secs % 60));
                }
            }
            if !ip_txt.is_empty() {
                p.clone().text(
                    Pos2::new(area.center().x, zone.bottom() - 10.0),
                    Align2::CENTER_CENTER,
                    ip_txt,
                    egui::FontId::proportional(11.0),
                    c_sub(),
                );
            }
            y += zone_h + 10.0;
        }

        // ---- servers panel ----
        {
            let panel_h = (area.bottom() - y - 30.0).max(120.0);
            let panel = Rect::from_min_size(Pos2::new(area.left(), y), Vec2::new(ui_w, panel_h));
            let p = ui.painter().clone();
            glass_panel(p.clone(), panel, 18.0, false);

            let inner = panel.shrink(12.0);
            // header
            let servers = self.shared.servers.lock().unwrap().clone();
            let countries: std::collections::BTreeSet<String> = servers.iter().filter(|s| !s.cc.is_empty()).map(|s| s.cc.clone()).collect();
            let head_txt = format!(
                "{} · {} {}",
                self.i18n.t("servers"),
                countries.len(),
                self.i18n.t("locations")
            );
            p.clone().text(
                Pos2::new(if rtl { inner.right() } else { inner.left() }, inner.top() + 8.0),
                if rtl { Align2::RIGHT_CENTER } else { Align2::LEFT_CENTER },
                head_txt,
                egui::FontId::proportional(11.5),
                c_sub(),
            );
            // refresh button
            let r_c = Pos2::new(if rtl { inner.left() + 12.0 } else { inner.right() - 12.0 }, inner.top() + 8.0);
            let r_resp = ui.allocate_rect(Rect::from_center_size(r_c, Vec2::splat(24.0)), Sense::click());
            p.clone().text(r_c, Align2::CENTER_CENTER, "⟳", egui::FontId::proportional(14.0), if r_resp.hovered() { c_accent() } else { c_ink() });
            if r_resp.clicked() {
                let custom = if self.config.custom_source.trim().is_empty() { None } else { Some(self.config.custom_source.clone()) };
                let _ = self.engine.tx.send(Cmd::FetchSources { urls: vec![], custom });
                self.toast(self.i18n.t("refreshing"), false);
            }
            // spinner if pinging
            if self.shared.pinging.load(Ordering::SeqCst) {
                let sp = Pos2::new(if rtl { inner.left() + 34.0 } else { inner.right() - 34.0 }, inner.top() + 8.0);
                let now = self.now_s();
                power_icon(p.clone(), sp, 6.0, c_accent(), 1.6, ((now * 2.0) % 0.85 + 0.12) as f32);
            }

            // list
            let list_top = inner.top() + 24.0;
            let list = Rect::from_min_size(Pos2::new(inner.left(), list_top), Vec2::new(inner.width(), inner.bottom() - list_top));
            let mut scroll_ui = ui.child_ui(list, Layout::top_down(Align::Min), None);
            ScrollArea::vertical().auto_shrink(false).show(&mut scroll_ui, |sui| {
                let pings = self.shared.pings.lock().unwrap().clone();
                // sort by ping
                let mut order: Vec<usize> = (0..servers.len()).collect();
                order.sort_by_key(|&i| pings.get(i).copied().unwrap_or(u32::MAX));
                for i in order {
                    let s = &servers[i];
                    let ms = pings.get(i).copied().unwrap_or(u32::MAX);
                    let selected = s.key() == self.selected_key;
                    let row_h = 46.0;
                    let (row_rect, row_resp) = sui.allocate_exact_size(Vec2::new(sui.available_width(), row_h), Sense::click());
                    let row = row_rect.shrink2(Vec2::new(4.0, 2.0));
                    if selected {
                        let sp = sui.painter().clone(); sp.rect_filled(row, rad(12.0), c_accent().gamma_multiply(0.14));
                        sui.painter().rect_stroke(row, rad(12.0), Stroke::new(1.2, c_accent().gamma_multiply(0.7)), egui::StrokeKind::Inside);
                    } else if row_resp.hovered() {
                        let sp = sui.painter().clone(); sp.rect_filled(row, rad(12.0), Color32::from_white_alpha(90));
                    } else {
                        let sp = sui.painter().clone(); sp.rect_filled(row, rad(12.0), Color32::from_white_alpha(55));
                    }
                    // cc badge
                    let bw = 34.0;
                    let brect = Rect::from_center_size(
                        Pos2::new(if rtl { row.right() - 8.0 - bw / 2.0 } else { row.left() + 8.0 + bw / 2.0 }, row.center().y),
                        Vec2::new(bw, 24.0),
                    );
                    cc_badge(sui.painter().clone(), brect, &s.cc);
                    // name + city
                    let nx = if rtl { row.right() - 8.0 - bw - 10.0 } else { row.left() + 8.0 + bw + 10.0 };
                    sui.painter().text(
                        Pos2::new(nx, row.center().y - 7.0),
                        if rtl { Align2::RIGHT_CENTER } else { Align2::LEFT_CENTER },
                        truncate(&s.remark, 34),
                        egui::FontId::proportional(12.0),
                        c_ink(),
                    );
                    sui.painter().text(
                        Pos2::new(nx, row.center().y + 8.0),
                        if rtl { Align2::RIGHT_CENTER } else { Align2::LEFT_CENTER },
                        format!("{} · {}", s.proto.to_uppercase(), s.host),
                        egui::FontId::proportional(9.5),
                        c_faint(),
                    );
                    // ping pill
                    let pw = 52.0;
                    let prect = Rect::from_center_size(
                        Pos2::new(if rtl { row.left() + 8.0 + pw / 2.0 } else { row.right() - 8.0 - pw / 2.0 }, row.center().y),
                        Vec2::new(pw, 22.0),
                    );
                    let (col, txt) = if ms == u32::MAX {
                        (c_faint(), "—".into())
                    } else {
                        let c = if ms < 300 { c_success() } else if ms < 900 { c_warn() } else { c_danger() };
                        (c, format!("{} ms", ms))
                    };
                    let sp = sui.painter().clone(); sp.rect_filled(prect, rad(11.0), Color32::from_white_alpha(90));
                    sui.painter().circle_filled(Pos2::new(prect.center().x - (if rtl { -12.0 } else { 12.0 }), prect.center().y), 2.5, col);
                    sui.painter().text(
                        Pos2::new(prect.center().x + (if rtl { 4.0 } else { -4.0 }), prect.center().y),
                        Align2::CENTER_CENTER,
                        txt,
                        egui::FontId::proportional(10.0),
                        c_ink(),
                    );
                    if row_resp.clicked() {
                        self.selected_key = s.key();
                        self.config.selected_key = s.key();
                        self.config.save();
                        // auto reconnect if connected
                        if matches!(self.shared.status().phase, Phase::Connected | Phase::Connecting) {
                            let _ = self.engine.tx.send(Cmd::Connect { server: s.clone() });
                        }
                    }
                }
            });
        }

        // ---- footer ----
        {
            let p = ui.painter().clone();
            let foot_y = area.bottom() - 2.0;
            let lic_txt = match self.lic_state.plan {
                Plan::Premium => self.i18n.t("premium"),
                Plan::Lifetime => self.i18n.t("lifetime"),
                Plan::Trial => {
                    if self.lic_state.days_left.is_infinite() {
                        self.i18n.t("trial_note").to_string()
                    } else {
                        format!("{}: {:.0} {}", self.i18n.t("trial_note"), self.lic_state.days_left, self.i18n.t("days_left"))
                    }
                }
                Plan::Free => self.i18n.t("free"),
            };
            p.clone().text(
                Pos2::new(if rtl { area.right() } else { area.left() }, foot_y),
                if rtl { Align2::RIGHT_CENTER } else { Align2::LEFT_CENTER },
                lic_txt,
                egui::FontId::proportional(10.0),
                c_sub(),
            );
            p.clone().text(
                Pos2::new(if rtl { area.left() } else { area.right() }, foot_y),
                if rtl { Align2::LEFT_CENTER } else { Align2::RIGHT_CENTER },
                "v1.6.0 · MIT",
                egui::FontId::proportional(10.0),
                c_sub(),
            );
        }

        // overlays
        if self.show_settings {
            self.ui_settings(ctx);
        }
        if self.gate {
            self.ui_gate(ctx);
        }
    }

    fn on_connect_click(&mut self) {
        let st = self.shared.status();
        match st.phase {
            Phase::Connected | Phase::Connecting | Phase::Disconnecting => {
                let _ = self.engine.tx.send(Cmd::Disconnect);
            }
            Phase::Idle | Phase::Error(_) => {
                if !self.lic_state.valid {
                    self.gate = true;
                    return;
                }
                if let Some(s) = self.selected_server() {
                    let _ = self.engine.tx.send(Cmd::Connect { server: s });
                } else {
                    self.toast(self.i18n.t("no_servers"), true);
                }
            }
        }
    }

    fn ui_settings(&mut self, ctx: &Context) {
        let screen = ctx.screen_rect();
        let w = (screen.width() - 40.0).min(360.0);
        let h = 310.0;
        let sheet = Rect::from_center_size(screen.center(), Vec2::new(w, h));
        let rtl = self.i18n.is_rtl();
        let inner = sheet.shrink(18.0);

        egui::Area::new(egui::Id::new("veil_settings"))
            .order(egui::Order::Foreground)
            .fixed_pos(sheet.min)
            .show(ctx, |ui| {
                ui.set_min_size(sheet.size());
                let p = ui.painter().clone();
                glass_panel(p.clone(), sheet, 22.0, true);

                p.clone().text(
                    Pos2::new(if rtl { inner.right() } else { inner.left() }, inner.top() + 8.0),
                    if rtl { Align2::RIGHT_CENTER } else { Align2::LEFT_CENTER },
                    self.i18n.t("settings"),
                    egui::FontId::proportional(15.0),
                    c_ink(),
                );
                let cx = Pos2::new(if rtl { inner.left() + 10.0 } else { inner.right() - 10.0 }, inner.top() + 8.0);
                let cresp = ui.allocate_rect(Rect::from_center_size(cx, Vec2::splat(26.0)), Sense::click());
                p.clone().text(cx, Align2::CENTER_CENTER, "✕", egui::FontId::proportional(11.0), c_ink());
                if cresp.clicked() {
                    self.show_settings = false;
                }

                // language label + combo
                let lab_y = inner.top() + 34.0;
                p.clone().text(
                    Pos2::new(if rtl { inner.right() } else { inner.left() }, lab_y),
                    if rtl { Align2::RIGHT_CENTER } else { Align2::LEFT_CENTER },
                    self.i18n.t("language"),
                    egui::FontId::proportional(12.0),
                    c_ink(),
                );
                let combo_rect = Rect::from_min_size(Pos2::new(inner.left(), lab_y + 16.0), Vec2::new(inner.width(), 30.0));
                p.clone().rect_filled(combo_rect, rad(10.0), Color32::from_white_alpha(150));
                p.clone().rect_stroke(combo_rect, rad(10.0), Stroke::new(1.0, Color32::from_black_alpha(50)), egui::StrokeKind::Inside);
                let cur = self
                    .i18n
                    .data
                    .languages
                    .iter()
                    .find(|l| l.code == self.i18n.lang)
                    .map(|l| l.name.clone())
                    .unwrap_or_else(|| "English".into());
                p.clone().text(
                    Pos2::new(if rtl { combo_rect.right() - 10.0 } else { combo_rect.left() + 10.0 }, combo_rect.center().y),
                    if rtl { Align2::RIGHT_CENTER } else { Align2::LEFT_CENTER },
                    cur,
                    egui::FontId::proportional(12.5),
                    c_ink(),
                );
                let caret = Pos2::new(if rtl { combo_rect.left() + 14.0 } else { combo_rect.right() - 14.0 }, combo_rect.center().y);
                p.clone().text(caret, Align2::CENTER_CENTER, "▾", egui::FontId::proportional(12.0), c_sub());
                if ui.allocate_rect(combo_rect, Sense::click()).clicked() {
                    let langs = self.i18n.data.languages.clone();
                    if let Some(i) = langs.iter().position(|l| l.code == self.i18n.lang) {
                        let next = langs[(i + 1) % langs.len()].code.clone();
                        self.set_language(next);
                    }
                }

                // custom source
                let lab2_y = combo_rect.bottom() + 14.0;
                p.clone().text(
                    Pos2::new(if rtl { inner.right() } else { inner.left() }, lab2_y),
                    if rtl { Align2::RIGHT_CENTER } else { Align2::LEFT_CENTER },
                    self.i18n.t("custom_source"),
                    egui::FontId::proportional(12.0),
                    c_ink(),
                );
                let row_y = lab2_y + 16.0;
                let edit_w = inner.width() - 104.0;
                let edit_rect = Rect::from_min_size(
                    Pos2::new(if rtl { inner.right() - edit_w } else { inner.left() }, row_y),
                    Vec2::new(edit_w, 30.0),
                );
                let edit = egui::TextEdit::singleline(&mut self.sub_input)
                    .hint_text(RichText::new("https://...").size(10.0))
                    .desired_width(edit_w)
                    .font(egui::TextStyle::Small);
                ui.put(edit_rect, edit);
                let btn_rect = Rect::from_min_size(
                    Pos2::new(if rtl { inner.left() } else { edit_rect.right() + 8.0 }, row_y),
                    Vec2::new(96.0, 30.0),
                );
                gloss_pill(p.clone(), btn_rect, c_charcoal(), None);
                p.clone().text(
                    btn_rect.center(),
                    Align2::CENTER_CENTER,
                    self.i18n.t("save_refresh"),
                    egui::FontId::proportional(9.5),
                    Color32::WHITE,
                );
                if ui.allocate_rect(btn_rect, Sense::click()).clicked() {
                    self.config.custom_source = self.sub_input.clone();
                    self.config.save();
                    let custom = if self.config.custom_source.trim().is_empty() { None } else { Some(self.config.custom_source.clone()) };
                    let _ = self.engine.tx.send(Cmd::FetchSources { urls: vec![], custom });
                    self.show_settings = false;
                    self.toast(self.i18n.t("refreshing"), false);
                }

                // license box
                let lic_rect = Rect::from_min_size(Pos2::new(inner.left(), row_y + 42.0), Vec2::new(inner.width(), 66.0));
                glass_panel(p.clone(), lic_rect, 12.0, false);
                let plan_txt = match self.lic_state.plan {
                    Plan::Premium => self.i18n.t("premium"),
                    Plan::Lifetime => self.i18n.t("lifetime"),
                    Plan::Trial => self.i18n.t("trial_note"),
                    Plan::Free => self.i18n.t("free"),
                };
                let days = if self.lic_state.days_left.is_infinite() {
                    "∞".to_string()
                } else {
                    format!("{:.0} {}", self.lic_state.days_left, self.i18n.t("days_left"))
                };
                p.clone().text(
                    Pos2::new(if rtl { lic_rect.right() - 12.0 } else { lic_rect.left() + 12.0 }, lic_rect.top() + 16.0),
                    if rtl { Align2::RIGHT_CENTER } else { Align2::LEFT_CENTER },
                    plan_txt,
                    egui::FontId::proportional(13.0),
                    c_ink(),
                );
                p.clone().text(
                    Pos2::new(if rtl { lic_rect.right() - 12.0 } else { lic_rect.left() + 12.0 }, lic_rect.top() + 38.0),
                    if rtl { Align2::RIGHT_CENTER } else { Align2::LEFT_CENTER },
                    if matches!(self.lic_state.plan, Plan::Premium | Plan::Lifetime) { "∞".to_string() } else { days },
                    egui::FontId::proportional(10.5),
                    c_sub(),
                );
                let tg_rect = Rect::from_min_size(
                    Pos2::new(if rtl { lic_rect.left() + 12.0 } else { lic_rect.right() - 132.0 }, lic_rect.top() + 14.0),
                    Vec2::new(120.0, 26.0),
                );
                gloss_pill(p.clone(), tg_rect, c_accent(), None);
                p.clone().text(tg_rect.center(), Align2::CENTER_CENTER, self.i18n.t("get_key"), egui::FontId::proportional(10.0), Color32::WHITE);
                if ui.allocate_rect(tg_rect, Sense::click()).clicked() {
                    open_url(ctx, "https://t.me/veilvpnverifybot");
                }
            });
    }

    fn ui_gate(&mut self, ctx: &Context) {
        let screen = ctx.screen_rect();
        let w = (screen.width() - 40.0).min(330.0);
        let h = 250.0;
        let sheet = Rect::from_center_size(screen.center(), Vec2::new(w, h));
        let inner = sheet.shrink(18.0);
        let rtl = self.i18n.is_rtl();

        egui::Area::new(egui::Id::new("veil_gate"))
            .order(egui::Order::Foreground)
            .fixed_pos(sheet.min)
            .show(ctx, |ui| {
                ui.set_min_size(sheet.size());
                let p = ui.painter().clone();
                p.clone().rect_filled(screen, 0.0, Color32::from_black_alpha(70));
                glass_panel(p.clone(), sheet, 22.0, true);
                p.clone().text(
                    Pos2::new(screen.center().x, inner.top() + 20.0),
                    Align2::CENTER_CENTER,
                    "🔒",
                    egui::FontId::proportional(24.0),
                    c_ink(),
                );
                p.clone().text(
                    Pos2::new(screen.center().x, inner.top() + 52.0),
                    Align2::CENTER_CENTER,
                    self.i18n.t("gate_title"),
                    egui::FontId::proportional(16.0),
                    c_ink(),
                );
                p.clone().text(
                    Pos2::new(screen.center().x, inner.top() + 74.0),
                    Align2::CENTER_CENTER,
                    self.i18n.t("gate_sub"),
                    egui::FontId::proportional(11.0),
                    c_sub(),
                );
                // key input
                let key_rect = Rect::from_min_size(Pos2::new(inner.left() + 6.0, inner.top() + 98.0), Vec2::new(inner.width() - 12.0, 30.0));
                let edit = egui::TextEdit::singleline(&mut self.key_input)
                    .hint_text(RichText::new("VEIL-...").size(11.0))
                    .desired_width(key_rect.width())
                    .font(egui::TextStyle::Small);
                ui.put(key_rect, edit);
                // activate button
                let act_rect = Rect::from_min_size(Pos2::new(inner.left() + 6.0, key_rect.bottom() + 8.0), Vec2::new(inner.width() - 12.0, 36.0));
                gloss_pill(p.clone(), act_rect, c_accent(), None);
                p.clone().text(act_rect.center(), Align2::CENTER_CENTER, self.i18n.t("activate"), egui::FontId::proportional(14.0), Color32::WHITE);
                if ui.allocate_rect(act_rect, Sense::click()).clicked() {
                    match license::activate(&self.key_input) {
                        Ok(l) => {
                            self.license = Some(l);
                            self.lic_state = license::evaluate(&self.license);
                            self.gate = false;
                            self.toast(self.i18n.t("key_ok"), false);
                        }
                        Err(_) => self.toast(self.i18n.t("key_bad"), true),
                    }
                }
                // telegram + later row
                let row_y = act_rect.bottom() + 8.0;
                let half = (inner.width() - 12.0) / 2.0;
                let tg_rect = Rect::from_min_size(Pos2::new(if rtl { inner.right() - half } else { inner.left() + 6.0 }, row_y), Vec2::new(half, 28.0));
                let later_rect = Rect::from_min_size(Pos2::new(if rtl { inner.left() + 6.0 } else { tg_rect.right() + 12.0 }, row_y), Vec2::new(half, 28.0));
                gloss_pill(p.clone(), tg_rect, Color32::from_rgb(0x2A, 0xAB, 0xEE), None);
                p.clone().text(tg_rect.center(), Align2::CENTER_CENTER, self.i18n.t("get_key"), egui::FontId::proportional(11.0), Color32::WHITE);
                if ui.allocate_rect(tg_rect, Sense::click()).clicked() {
                    open_url(ctx, "https://t.me/veilvpnverifybot");
                }
                p.clone().text(later_rect.center(), Align2::CENTER_CENTER, self.i18n.t("later"), egui::FontId::proportional(11.0), c_ink());
                if ui.allocate_rect(later_rect, Sense::click()).clicked() {
                    self.gate = false;
                }
            });
    }

    fn ui_toast(&mut self, ui: &mut egui::Ui) {
        if let Some((txt, t0, err)) = self.toast.clone() {
            let fade = (3.0 - (self.now_s() - t0)).clamp(0.0, 1.0);
            let screen = ui.ctx().screen_rect();
            let w = (screen.width() * 0.72).min(320.0);
            let rect = Rect::from_center_size(Pos2::new(screen.center().x, screen.bottom() - 34.0), Vec2::new(w, 34.0));
            let p = ui.painter().clone();
            let base = if err { c_danger() } else { c_charcoal() };
            p.clone().rect_filled(rect, rad(12.0), base.gamma_multiply((0.5 + fade * 0.45) as f32));
            p.clone().rect_stroke(rect, rad(12.0), Stroke::new(1.0, Color32::from_white_alpha((140.0 * fade) as u8)), egui::StrokeKind::Inside);
            p.clone().text(
                rect.center(),
                Align2::CENTER_CENTER,
                txt,
                egui::FontId::proportional(11.5),
                Color32::from_white_alpha(240),
            );
        }
    }

    fn set_language(&mut self, code: String) {
        self.i18n.lang = code.clone();
        self.config.lang = code;
        self.config.save();
    }
}

fn open_url(ctx: &Context, url: &str) {
    ctx.open_url(egui::OpenUrl::same_tab(url));
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let t: String = s.chars().take(max_chars - 1).collect();
        format!("{}…", t)
    }
}
