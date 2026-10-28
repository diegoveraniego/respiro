use adw::prelude::*;
use gtk::{gdk, gio, glib};
use std::{
    cell::{Cell, RefCell},
    ffi::{CStr, c_char, c_int, c_void},
    fs,
    path::PathBuf,
    ptr,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    thread,
    time::Duration,
};

const APP_ID: &str = "org.diego.Respiro";
const IDLE_HIDE_US: i64 = 2_500_000; // 2.5 seconds before auto-hiding controls & cursor

#[derive(Clone, Copy, Debug)]
struct Pattern {
    name: &'static str,
    durations: [f64; 4], // [inhale, hold1, exhale, hold2] in seconds
}

const PRESETS: &[Pattern] = &[
    Pattern {
        name: "5-5-5-5",
        durations: [5.0, 5.0, 5.0, 5.0],
    },
    Pattern {
        name: "4-4-4-4",
        durations: [4.0, 4.0, 4.0, 4.0],
    },
    Pattern {
        name: "4-7-8",
        durations: [4.0, 7.0, 8.0, 0.0],
    },
    Pattern {
        name: "5-5",
        durations: [5.0, 0.0, 5.0, 0.0],
    },
    Pattern {
        name: "Personalizado",
        durations: [5.0, 5.0, 5.0, 5.0],
    },
];

struct TimerOption {
    label: &'static str,
    seconds: f64,
}

const TIMER_OPTIONS: &[TimerOption] = &[
    TimerOption {
        label: "Sin límite",
        seconds: 0.0,
    },
    TimerOption {
        label: "2 min",
        seconds: 120.0,
    },
    TimerOption {
        label: "5 min",
        seconds: 300.0,
    },
    TimerOption {
        label: "10 min",
        seconds: 600.0,
    },
    TimerOption {
        label: "15 min",
        seconds: 900.0,
    },
];

const PHASE_NAMES: [&str; 4] = ["Inhala", "Mantiene", "Exhala", "Mantiene"];

fn config_path() -> PathBuf {
    glib::user_config_dir().join("respiro").join("state.conf")
}

fn load_config() -> (u32, [f64; 4], bool) {
    let Ok(text) = fs::read_to_string(config_path()) else {
        return (0, PRESETS[0].durations, false);
    };
    let mut preset = 0u32;
    let mut d = PRESETS[0].durations;
    let mut sound = false;
    for line in text.lines() {
        if let Some((k, v)) = line.split_once('=') {
            match k.trim() {
                "preset" => preset = v.trim().parse().unwrap_or(0),
                "d0" => d[0] = v.trim().parse::<f64>().unwrap_or(5.0).clamp(1.0, 30.0),
                "d1" => d[1] = v.trim().parse::<f64>().unwrap_or(5.0).clamp(0.0, 30.0),
                "d2" => d[2] = v.trim().parse::<f64>().unwrap_or(5.0).clamp(1.0, 30.0),
                "d3" => d[3] = v.trim().parse::<f64>().unwrap_or(5.0).clamp(0.0, 30.0),
                "sound" => sound = matches!(v.trim(), "true" | "1"),
                _ => {}
            }
        }
    }
    let idx = (preset as usize).min(PRESETS.len() - 1);
    if idx < PRESETS.len() - 1 {
        d = PRESETS[idx].durations;
    }
    (idx as u32, d, sound)
}

fn save_config(preset: u32, d: [f64; 4], sound: bool) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let content = format!(
        "preset={preset}\nd0={}\nd1={}\nd2={}\nd3={}\nsound={sound}\n",
        d[0], d[1], d[2], d[3]
    );
    let _ = fs::write(path, content);
}

#[repr(C)]
struct PaSampleSpec {
    format: c_int,
    rate: u32,
    channels: u8,
}

#[repr(C)]
struct PaBufferAttr {
    maxlength: u32,
    tlength: u32,
    prebuf: u32,
    minreq: u32,
    fragsize: u32,
}

const PA_STREAM_PLAYBACK: c_int = 1;
const PA_SAMPLE_FLOAT32LE: c_int = 5;

#[link(name = "pulse-simple")]
#[link(name = "pulse")]
unsafe extern "C" {
    fn pa_simple_new(
        server: *const c_char,
        name: *const c_char,
        dir: c_int,
        dev: *const c_char,
        stream_name: *const c_char,
        ss: *const PaSampleSpec,
        map: *const c_void,
        attr: *const PaBufferAttr,
        error: *mut c_int,
    ) -> *mut c_void;
    fn pa_simple_write(
        s: *mut c_void,
        data: *const c_void,
        bytes: usize,
        error: *mut c_int,
    ) -> c_int;
    fn pa_simple_free(s: *mut c_void);
}

struct AudioSynth {
    active: AtomicBool,
    level_bits: AtomicU32, // f32 bits in 0.0..=1.0
    phase: AtomicU32,      // 0: Inhala, 1: Mantiene1, 2: Exhala, 3: Mantiene2
}

impl AudioSynth {
    fn spawn() -> Arc<Self> {
        let synth = Arc::new(Self {
            active: AtomicBool::new(false),
            level_bits: AtomicU32::new(0.0_f32.to_bits()),
            phase: AtomicU32::new(0),
        });
        let s = synth.clone();
        thread::spawn(move || run_dsp_loop(s));
        synth
    }

    fn update(&self, active: bool, level: f64, phase: usize) {
        self.level_bits
            .store((level.clamp(0.0, 1.0) as f32).to_bits(), Ordering::Relaxed);
        self.phase.store(phase as u32, Ordering::Relaxed);
        self.active.store(active, Ordering::Relaxed);
    }
}

#[derive(Default)]
struct SvfBandPass {
    ic1eq: f32,
    ic2eq: f32,
    lp_post: f32,
    brown: f32,
}

impl SvfBandPass {
    fn step(&mut self, white: f32, g: f32, k: f32) -> f32 {
        // Leaky integration of white noise -> warm Brown Noise
        self.brown = (self.brown + 0.028 * white).clamp(-1.0, 1.0) * 0.996;

        // Cytomic SVF (Band-Pass output v1)
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = self.brown - self.ic2eq;
        let v1 = a1 * self.ic1eq + a2 * v3;
        let v2 = self.ic2eq + a2 * self.ic1eq + a3 * v3;
        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;

        // Gentle post low-pass (~1.6 kHz) to keep texture silky
        self.lp_post += 0.18 * (v1 - self.lp_post);
        self.lp_post
    }
}

fn run_dsp_loop(synth: Arc<AudioSynth>) {
    let sample_rate = 48_000u32;
    let frames_per_chunk = 512usize; // ~10.6 ms latency
    let mut buf = vec![0.0_f32; frames_per_chunk * 2];

    let mut rng_state = 0x853c_49e6_748f_ea9b_u64;
    let mut next_white = || -> f32 {
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;
        let u = (rng_state >> 40) as u32;
        (u as f32 / ((1u32 << 24) as f32)) * 2.0 - 1.0
    };

    let mut left = SvfBandPass::default();
    let mut right = SvfBandPass::default();
    let mut smooth_lvl = 0.0_f32;
    let mut smooth_q = 2.2_f32;
    let mut smooth_hold_gain = 1.0_f32;
    let mut gate_env = 0.0_f32;
    let mut pa_handle: *mut c_void = ptr::null_mut();

    let ss = PaSampleSpec {
        format: PA_SAMPLE_FLOAT32LE,
        rate: sample_rate,
        channels: 2,
    };
    let byte_len = (frames_per_chunk * 2 * std::mem::size_of::<f32>()) as u32;
    let attr = PaBufferAttr {
        maxlength: byte_len * 4,
        tlength: byte_len * 2,
        prebuf: byte_len,
        minreq: byte_len,
        fragsize: u32::MAX,
    };

    loop {
        let is_active = synth.active.load(Ordering::Relaxed);
        if !is_active && gate_env < 0.0002 {
            if !pa_handle.is_null() {
                unsafe { pa_simple_free(pa_handle) };
                pa_handle = ptr::null_mut();
            }
            thread::sleep(Duration::from_millis(25));
            continue;
        }

        if pa_handle.is_null() {
            let app_name = CStr::from_bytes_with_nul(b"Respiro\0").unwrap();
            let stream_name = CStr::from_bytes_with_nul(b"Breathing Guide\0").unwrap();
            let mut err: c_int = 0;
            pa_handle = unsafe {
                pa_simple_new(
                    ptr::null(),
                    app_name.as_ptr(),
                    PA_STREAM_PLAYBACK,
                    ptr::null(),
                    stream_name.as_ptr(),
                    &ss,
                    ptr::null(),
                    &attr,
                    &mut err,
                )
            };
            if pa_handle.is_null() {
                thread::sleep(Duration::from_millis(100));
                continue;
            }
        }

        let target_lvl = f32::from_bits(synth.level_bits.load(Ordering::Relaxed));
        let phase = synth.phase.load(Ordering::Relaxed);
        let is_hold = phase == 1 || phase == 3;
        let target_q = if is_hold { 3.2_f32 } else { 2.1_f32 };
        let target_hold_gain = if is_hold { 0.68_f32 } else { 1.0_f32 };
        let target_gate = if is_active { 1.0_f32 } else { 0.0_f32 };

        for frame in buf.chunks_exact_mut(2) {
            gate_env += 0.0006 * (target_gate - gate_env);
            smooth_lvl += 0.0008 * (target_lvl - smooth_lvl);
            smooth_q += 0.0005 * (target_q - smooth_q);
            smooth_hold_gain += 0.0005 * (target_hold_gain - smooth_hold_gain);

            // Exponential cutoff sweep: 135 Hz (empty lungs) -> 620 Hz (full inhale)
            let fc = 135.0_f32 * (620.0_f32 / 135.0_f32).powf(smooth_lvl);
            let g = (std::f32::consts::PI * fc / sample_rate as f32).tan();
            let k = 1.0 / smooth_q;

            // Normalize band-pass energy as Q and cutoff vary, and swell gently with inhalation
            let breath_swell = 0.42 + 0.58 * smooth_lvl;
            let amp = gate_env * breath_swell * smooth_hold_gain * 0.22;

            let wl = next_white();
            let wr = next_white();
            let sig_l = left.step(wl, g, k);
            let sig_r = right.step(wr, g, k);

            // Slight stereo cross-mix (80% L + 20% R) for natural headphones comfort
            frame[0] = (sig_l * 0.82 + sig_r * 0.18) * amp;
            frame[1] = (sig_r * 0.82 + sig_l * 0.18) * amp;
        }

        let mut err: c_int = 0;
        let res = unsafe {
            pa_simple_write(
                pa_handle,
                buf.as_ptr() as *const c_void,
                buf.len() * std::mem::size_of::<f32>(),
                &mut err,
            )
        };
        if res < 0 {
            unsafe { pa_simple_free(pa_handle) };
            pa_handle = ptr::null_mut();
        }
    }
}

struct State {
    running: bool,
    durations: [f64; 4],
    phase: usize,
    phase_elapsed: f64,
    session_elapsed: f64,
    session_limit: f64, // 0.0 = unlimited
    sound_enabled: bool,
    last_frame_us: Option<i64>,
    last_motion_us: i64,
    inhibit_cookie: u32,
}

impl State {
    fn new(durations: [f64; 4], sound_enabled: bool) -> Self {
        let mut s = Self {
            running: false,
            durations,
            phase: 0,
            phase_elapsed: 0.0,
            session_elapsed: 0.0,
            session_limit: 0.0,
            sound_enabled,
            last_frame_us: None,
            last_motion_us: 0,
            inhibit_cookie: 0,
        };
        s.advance_zero_phases();
        s
    }

    fn reset(&mut self) {
        self.running = false;
        self.phase = 0;
        self.phase_elapsed = 0.0;
        self.session_elapsed = 0.0;
        self.last_frame_us = None;
        self.advance_zero_phases();
    }

    fn advance_zero_phases(&mut self) {
        if self.durations.iter().all(|&d| d <= 0.0) {
            return;
        }
        let mut guard = 0;
        while self.durations[self.phase] <= 0.0 && guard < 4 {
            self.phase = (self.phase + 1) % 4;
            guard += 1;
        }
    }

    fn step(&mut self, dt: f64) -> bool {
        if !self.running {
            return false;
        }
        let total: f64 = self.durations.iter().sum();
        if total <= 0.0 {
            return false;
        }
        self.session_elapsed += dt;
        if self.session_limit > 0.0 && self.session_elapsed >= self.session_limit {
            self.reset();
            return true;
        }

        self.advance_zero_phases();
        self.phase_elapsed += dt;
        while self.phase_elapsed >= self.durations[self.phase] && self.durations[self.phase] > 0.0 {
            self.phase_elapsed -= self.durations[self.phase];
            self.phase = (self.phase + 1) % 4;
            let mut guard = 0;
            while self.durations[self.phase] <= 0.0 && guard < 4 {
                self.phase = (self.phase + 1) % 4;
                guard += 1;
            }
        }
        false
    }

    fn phase_fraction(&self) -> f64 {
        let dur = self.durations[self.phase];
        if dur <= 0.0 {
            0.0
        } else {
            (self.phase_elapsed / dur).clamp(0.0, 1.0)
        }
    }

    /// Smooth normalized lung fullness in `0.0..=1.0` used by both visual halo and band-pass audio synth.
    fn breath_level(&self) -> f64 {
        let t = smoothstep(self.phase_fraction());
        match self.phase % 4 {
            0 => t,
            1 => 1.0,
            2 => 1.0 - t,
            _ => 0.0,
        }
    }

    fn remaining_seconds(&self) -> u32 {
        let dur = self.durations[self.phase];
        if dur <= 0.0 {
            0
        } else {
            (dur - self.phase_elapsed).max(0.0).ceil() as u32
        }
    }
}

fn update_inhibit(app: &adw::Application, window: &adw::ApplicationWindow, state: &mut State) {
    if state.running && state.inhibit_cookie == 0 {
        state.inhibit_cookie = app.inhibit(
            Some(window),
            gtk::ApplicationInhibitFlags::IDLE,
            Some("Sesión de respiración activa"),
        );
    } else if !state.running && state.inhibit_cookie != 0 {
        app.uninhibit(state.inhibit_cookie);
        state.inhibit_cookie = 0;
    }
}

fn accent_rgba() -> gdk::RGBA {
    let style = adw::StyleManager::default();
    if style.is_dark() {
        gdk::RGBA::new(0.47, 0.68, 0.93, 1.0)
    } else {
        gdk::RGBA::new(0.21, 0.52, 0.89, 1.0)
    }
}

fn smoothstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Cubic Bézier controls `(c1, c2)` for a clockwise edge `a -> b` with outward bow and rounded corners.
fn edge_controls(
    prev: (f64, f64),
    a: (f64, f64),
    b: (f64, f64),
    next: (f64, f64),
    bow: f64,
    roundness: f64,
) -> ((f64, f64), (f64, f64)) {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let nx = dy * bow;
    let ny = -dx * bow;

    let t_ax = (b.0 - prev.0) * roundness;
    let t_ay = (b.1 - prev.1) * roundness;
    let t_bx = (next.0 - a.0) * roundness;
    let t_by = (next.1 - a.1) * roundness;

    let c1 = (a.0 + dx * 0.25 + nx + t_ax, a.1 + dy * 0.25 + ny + t_ay);
    let c2 = (a.0 + dx * 0.75 + nx - t_bx, a.1 + dy * 0.75 + ny - t_by);
    (c1, c2)
}

/// Evaluate cubic Bézier and split `[0, t]` via De Casteljau: returns `(sub_c1, sub_c2, pos)`.
fn cubic_split(
    a: (f64, f64),
    c1: (f64, f64),
    c2: (f64, f64),
    b: (f64, f64),
    t: f64,
) -> ((f64, f64), (f64, f64), (f64, f64)) {
    let lerp = |p: (f64, f64), q: (f64, f64), s: f64| {
        (p.0 + (q.0 - p.0) * s, p.1 + (q.1 - p.1) * s)
    };
    let q0 = lerp(a, c1, t);
    let q1 = lerp(c1, c2, t);
    let q2 = lerp(c2, b, t);
    let r0 = lerp(q0, q1, t);
    let r1 = lerp(q1, q2, t);
    let pos = lerp(r0, r1, t);
    (q0, r0, pos)
}

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();

    app.connect_startup(|app| {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(include_str!("style.css"));
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
        app.set_accels_for_action("win.toggle", &["space"]);
        app.set_accels_for_action("win.reset", &["r", "<Control>r"]);
        app.set_accels_for_action("app.quit", &["<Control>q", "Escape"]);
    });

    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &adw::Application) {
    if let Some(win) = app.active_window() {
        win.present();
        return;
    }

    let (initial_preset, initial_durations, initial_sound) = load_config();
    let state = Rc::new(RefCell::new(State::new(initial_durations, initial_sound)));
    let synth = AudioSynth::spawn();

    // Hidden probe to read GNOME `@accent_bg_color`
    let accent_probe = gtk::Box::builder()
        .css_classes(["accent-probe"])
        .visible(false)
        .build();

    // Settings popover
    let pop_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(10)
        .margin_top(10)
        .margin_bottom(10)
        .margin_start(10)
        .margin_end(10)
        .build();

    let names: Vec<&str> = PRESETS.iter().map(|p| p.name).collect();
    let mode_dropdown = gtk::DropDown::from_strings(&names);
    mode_dropdown.set_selected(initial_preset);
    mode_dropdown.set_tooltip_text(Some("Patrón de respiración"));
    pop_box.append(&mode_dropdown);

    let spins_grid = gtk::Grid::builder()
        .column_spacing(10)
        .row_spacing(6)
        .halign(gtk::Align::Center)
        .build();

    let spins: Rc<Vec<gtk::SpinButton>> = Rc::new(
        [
            ("Inhala", initial_durations[0], 1.0),
            ("Mantiene 1", initial_durations[1], 0.0),
            ("Exhala", initial_durations[2], 1.0),
            ("Mantiene 2", initial_durations[3], 0.0),
        ]
        .into_iter()
        .enumerate()
        .map(|(idx, (label_txt, initial, min_val))| {
            let lbl = gtk::Label::builder()
                .label(label_txt)
                .xalign(0.0)
                .css_classes(["caption", "dim-label"])
                .build();
            let spin = gtk::SpinButton::with_range(min_val, 30.0, 1.0);
            spin.set_value(initial);
            spins_grid.attach(&lbl, 0, idx as i32, 1, 1);
            spins_grid.attach(&spin, 1, idx as i32, 1, 1);
            spin
        })
        .collect(),
    );
    pop_box.append(&spins_grid);

    pop_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    let extra_grid = gtk::Grid::builder()
        .column_spacing(12)
        .row_spacing(8)
        .build();

    let timer_lbl = gtk::Label::builder()
        .label("Temporizador")
        .xalign(0.0)
        .hexpand(true)
        .css_classes(["caption", "dim-label"])
        .build();
    let timer_labels: Vec<&str> = TIMER_OPTIONS.iter().map(|t| t.label).collect();
    let timer_dropdown = gtk::DropDown::from_strings(&timer_labels);
    timer_dropdown.set_selected(0); // Always defaults to "Sin límite" so regulation has no goal interruption
    timer_dropdown.set_tooltip_text(Some("Duración de la sesión"));
    extra_grid.attach(&timer_lbl, 0, 0, 1, 1);
    extra_grid.attach(&timer_dropdown, 1, 0, 1, 1);

    let sound_lbl = gtk::Label::builder()
        .label("Sonido")
        .xalign(0.0)
        .hexpand(true)
        .css_classes(["caption", "dim-label"])
        .build();
    let sound_switch = gtk::Switch::builder()
        .active(initial_sound)
        .halign(gtk::Align::End)
        .valign(gtk::Align::Center)
        .tooltip_text("Sintetizador de ruido marrón filtrado con tu respiración")
        .build();
    extra_grid.attach(&sound_lbl, 0, 1, 1, 1);
    extra_grid.attach(&sound_switch, 1, 1, 1, 1);

    pop_box.append(&extra_grid);

    let reset_btn = gtk::Button::builder()
        .label("Reiniciar")
        .css_classes(["flat"])
        .build();
    pop_box.append(&reset_btn);

    let popover = gtk::Popover::builder().child(&pop_box).build();

    let gear_btn = gtk::MenuButton::builder()
        .icon_name("emblem-system-symbolic")
        .popover(&popover)
        .css_classes(["flat", "circular", "corner-btn"])
        .tooltip_text("Configuración")
        .build();

    let close_btn = gtk::Button::builder()
        .icon_name("window-close-symbolic")
        .css_classes(["flat", "circular", "corner-btn"])
        .tooltip_text("Cerrar (Esc)")
        .action_name("app.quit")
        .build();

    let top_right_controls = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(4)
        .halign(gtk::Align::End)
        .valign(gtk::Align::Start)
        .margin_top(12)
        .margin_end(12)
        .css_classes(["top-controls"])
        .build();
    top_right_controls.append(&gear_btn);
    top_right_controls.append(&close_btn);

    // Subtle bottom-center pause indicator icon that fades out when playing
    let pause_icon = gtk::Image::builder()
        .icon_name("media-playback-pause-symbolic")
        .pixel_size(16)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::End)
        .margin_bottom(24)
        .can_target(false)
        .css_classes(["pause-indicator"])
        .build();

    // Main canvas
    let area = gtk::DrawingArea::builder()
        .content_width(360)
        .content_height(360)
        .hexpand(true)
        .vexpand(true)
        .build();

    let phase_label = gtk::Label::builder()
        .label("Inhala")
        .css_classes(["phase-title"])
        .halign(gtk::Align::Center)
        .build();

    let timer_label = gtk::Label::builder()
        .label("5")
        .css_classes(["phase-timer", "numeric"])
        .halign(gtk::Align::Center)
        .build();

    let center_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .can_target(false)
        .build();
    center_box.append(&phase_label);
    center_box.append(&timer_label);

    let overlay = gtk::Overlay::builder()
        .child(&area)
        .hexpand(true)
        .vexpand(true)
        .build();
    overlay.add_overlay(&center_box);
    overlay.add_overlay(&pause_icon);
    overlay.add_overlay(&top_right_controls);
    overlay.add_overlay(&accent_probe);

    // WindowHandle allows dragging borderless window from anywhere and clicking to toggle
    let win_handle = gtk::WindowHandle::builder().child(&overlay).build();

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Respiro")
        .icon_name(APP_ID)
        .default_width(420)
        .default_height(420)
        .content(&win_handle)
        .build();

    let sync_ui_chrome = {
        let state = state.clone();
        let pause_icon = pause_icon.clone();
        let top_right_controls = top_right_controls.clone();
        let window = window.clone();
        move |show_controls: bool| {
            let st = state.borrow();
            if st.running {
                pause_icon.add_css_class("hidden-indicator");
            } else {
                pause_icon.remove_css_class("hidden-indicator");
            }

            if show_controls || !st.running {
                top_right_controls.remove_css_class("controls-hidden");
                window.set_cursor_from_name(None);
            } else {
                top_right_controls.add_css_class("controls-hidden");
                window.set_cursor_from_name(Some("none"));
            }
        }
    };

    let update_labels = {
        let state = state.clone();
        let phase_label = phase_label.clone();
        let timer_label = timer_label.clone();
        let center_box = center_box.clone();
        move || {
            let st = state.borrow();
            let active_count = st.durations.iter().filter(|&&d| d > 0.0).count();
            center_box.set_margin_top(if active_count == 3 { 36 } else { 0 });
            phase_label.set_label(PHASE_NAMES[st.phase]);
            if !st.running && st.phase_elapsed == 0.0 {
                timer_label.set_label(&format!("{}", st.durations[st.phase].round() as u32));
            } else {
                timer_label.set_label(&format!("{}", st.remaining_seconds()));
            }
        }
    };
    update_labels();
    sync_ui_chrome(true);

    // Restore controls & cursor whenever the pointer moves
    let motion = gtk::EventControllerMotion::new();
    let sync_ui_motion = sync_ui_chrome.clone();
    motion.connect_motion(glib::clone!(
        #[strong]
        state,
        move |_, _, _| {
            state.borrow_mut().last_motion_us = glib::monotonic_time();
            sync_ui_motion(true);
        }
    ));
    overlay.add_controller(motion);

    // Draw function: dynamic polygon (2, 3, or 4 vertices) + outward-curved arcs + GNOME accent dot
    area.set_draw_func(glib::clone!(
        #[strong]
        state,
        #[weak]
        accent_probe,
        move |widget, cr, w, h| {
            let st = state.borrow();
            let fg = widget.color();
            let accent = {
                let c = accent_probe.color();
                if c.alpha() > 0.01 {
                    c
                } else {
                    accent_rgba()
                }
            };

            let w = w as f64;
            let h = h as f64;
            let size = w.min(h) * 0.56;
            let half = size / 2.0;
            let cx = w / 2.0;
            let cy = h / 2.0;

            let p = st.phase % 4;
            let t = smoothstep(st.phase_fraction());
            let breath_scale = match p {
                0 => 1.0 + 0.30 * t,
                1 => 1.30,
                2 => 1.30 - 0.30 * t,
                _ => 1.0,
            };

            // Organic outward bow + smooth rounded corners at the active vertices
            let active_phases: Vec<usize> =
                (0..4).filter(|&i| st.durations[i] > 0.0).collect();
            let n = active_phases.len().max(2);
            let active_idx = active_phases.iter().position(|&ph| ph == p).unwrap_or(0);

            let (vertices, bow, roundness): (Vec<(f64, f64)>, f64, f64) = match n {
                2 => (
                    // 2-phase lens / leaf (bottom -> top -> bottom)
                    vec![(cx, cy + half * 1.05), (cx, cy - half * 1.05)],
                    0.34,
                    0.12,
                ),
                3 => {
                    // Equilateral triangle visually centered on its bounding box in the window
                    let r = half * 1.24;
                    let dx = r * (3.0_f64.sqrt() / 2.0);
                    let y_shift = r * 0.16;
                    (
                        vec![
                            (cx - dx, cy + r * 0.5 + y_shift),
                            (cx, cy - r + y_shift),
                            (cx + dx, cy + r * 0.5 + y_shift),
                        ],
                        0.13,
                        0.08,
                    )
                }
                _ => (
                    // 4-phase square: Bottom-Left -> Top-Left -> Top-Right -> Bottom-Right
                    vec![
                        (cx - half, cy + half),
                        (cx - half, cy - half),
                        (cx + half, cy - half),
                        (cx + half, cy + half),
                    ],
                    0.12,
                    0.08,
                ),
            };

            cr.set_line_cap(gtk::cairo::LineCap::Round);
            cr.set_line_join(gtk::cairo::LineJoin::Round);

            // Base outward-curved edges with rounded corners
            cr.set_source_rgba(
                fg.red() as f64,
                fg.green() as f64,
                fg.blue() as f64,
                0.14,
            );
            cr.set_line_width(2.5);
            cr.move_to(vertices[0].0, vertices[0].1);
            for i in 0..n {
                let prev = vertices[(i + n - 1) % n];
                let a = vertices[i];
                let b = vertices[(i + 1) % n];
                let next = vertices[(i + 2) % n];
                let (c1, c2) = edge_controls(prev, a, b, next, bow, roundness);
                cr.curve_to(c1.0, c1.1, c2.0, c2.1, b.0, b.1);
            }
            cr.close_path();
            let _ = cr.stroke();

            // Active phase progress along current outward-curved edge
            let prev_v = vertices[(active_idx + n - 1) % n];
            let start_v = vertices[active_idx];
            let end_v = vertices[(active_idx + 1) % n];
            let next_v = vertices[(active_idx + 2) % n];
            let (c1, c2) = edge_controls(prev_v, start_v, end_v, next_v, bow, roundness);

            let (sub_c1, sub_c2, (dot_x, dot_y)) = cubic_split(start_v, c1, c2, end_v, t);

            if t > 0.001 {
                cr.set_source_rgba(
                    accent.red() as f64,
                    accent.green() as f64,
                    accent.blue() as f64,
                    0.65,
                );
                cr.set_line_width(3.2);
                cr.move_to(start_v.0, start_v.1);
                cr.curve_to(sub_c1.0, sub_c1.1, sub_c2.0, sub_c2.1, dot_x, dot_y);
                let _ = cr.stroke();
            }

            // Vertices
            for (i, &(vx, vy)) in vertices.iter().enumerate() {
                let is_origin = i == active_idx;
                let radius = if is_origin { 5.0 } else { 4.0 };
                if is_origin {
                    cr.set_source_rgba(
                        accent.red() as f64,
                        accent.green() as f64,
                        accent.blue() as f64,
                        0.85,
                    );
                } else {
                    cr.set_source_rgba(
                        fg.red() as f64,
                        fg.green() as f64,
                        fg.blue() as f64,
                        0.28,
                    );
                }
                cr.arc(vx, vy, radius, 0.0, std::f64::consts::TAU);
                let _ = cr.fill();
            }

            // Moving GNOME accent dot (soft breathing halo + solid core)
            cr.set_source_rgba(
                accent.red() as f64,
                accent.green() as f64,
                accent.blue() as f64,
                0.20,
            );
            cr.arc(dot_x, dot_y, 16.0 * breath_scale, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();

            cr.set_source_rgba(
                accent.red() as f64,
                accent.green() as f64,
                accent.blue() as f64,
                1.0,
            );
            cr.arc(dot_x, dot_y, 8.0, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();
        }
    ));

    // Click anywhere on canvas to start/pause
    let click = gtk::GestureClick::new();
    click.connect_released(glib::clone!(
        #[weak]
        window,
        move |_, _, _, _| {
            let _ = gtk::prelude::WidgetExt::activate_action(&window, "win.toggle", None);
        }
    ));
    area.add_controller(click);

    // Tick callback synced to monitor refresh rate
    let update_labels_tick = update_labels.clone();
    let sync_ui_tick = sync_ui_chrome.clone();
    area.add_tick_callback(glib::clone!(
        #[strong]
        state,
        #[strong]
        synth,
        #[weak]
        app,
        #[weak]
        window,
        #[weak]
        popover,
        #[upgrade_or]
        glib::ControlFlow::Break,
        move |area, clock| {
            let now = clock.frame_time();
            let mut st = state.borrow_mut();
            if st.running {
                let mut finished = false;
                if let Some(last) = st.last_frame_us {
                    let dt = ((now - last).max(0) as f64) / 1_000_000.0;
                    finished = st.step(dt);
                }
                st.last_frame_us = Some(now);
                let idle_us = now - st.last_motion_us;
                if finished {
                    update_inhibit(&app, &window, &mut st);
                }
                synth.update(st.running && st.sound_enabled, st.breath_level(), st.phase);
                drop(st);

                if finished {
                    sync_ui_tick(true);
                } else {
                    let keep_visible = popover.is_visible() || idle_us < IDLE_HIDE_US;
                    sync_ui_tick(keep_visible);
                }

                update_labels_tick();
                area.queue_draw();
            } else {
                st.last_frame_us = None;
                synth.update(false, st.breath_level(), st.phase);
            }
            glib::ControlFlow::Continue
        }
    ));

    // Toggle play/pause + idle inhibit
    let toggle_action = gio::SimpleAction::new("toggle", None);
    let update_labels_toggle = update_labels.clone();
    let sync_ui_toggle = sync_ui_chrome.clone();
    toggle_action.connect_activate(glib::clone!(
        #[strong]
        state,
        #[strong]
        synth,
        #[weak]
        app,
        #[weak]
        window,
        #[weak]
        area,
        move |_, _| {
            let mut st = state.borrow_mut();
            st.running = !st.running;
            st.last_frame_us = None;
            st.last_motion_us = glib::monotonic_time();
            update_inhibit(&app, &window, &mut st);
            synth.update(st.running && st.sound_enabled, st.breath_level(), st.phase);
            drop(st);

            sync_ui_toggle(true);
            update_labels_toggle();
            area.queue_draw();
        }
    ));
    window.add_action(&toggle_action);

    // Reset action
    let reset_action = gio::SimpleAction::new("reset", None);
    let update_labels_reset = update_labels.clone();
    let sync_ui_reset = sync_ui_chrome.clone();
    reset_action.connect_activate(glib::clone!(
        #[strong]
        state,
        #[strong]
        synth,
        #[weak]
        app,
        #[weak]
        window,
        #[weak]
        area,
        #[weak]
        popover,
        move |_, _| {
            let mut st = state.borrow_mut();
            st.reset();
            update_inhibit(&app, &window, &mut st);
            synth.update(false, 0.0, st.phase);
            drop(st);
            popover.popdown();
            sync_ui_reset(true);
            update_labels_reset();
            area.queue_draw();
        }
    ));
    window.add_action(&reset_action);

    reset_btn.connect_clicked(glib::clone!(
        #[weak]
        window,
        move |_| {
            let _ = gtk::prelude::WidgetExt::activate_action(&window, "win.reset", None);
        }
    ));

    // Session timer selection inside popover
    timer_dropdown.connect_selected_notify(glib::clone!(
        #[strong]
        state,
        move |dd| {
            let idx = (dd.selected() as usize).min(TIMER_OPTIONS.len() - 1);
            let mut st = state.borrow_mut();
            st.session_limit = TIMER_OPTIONS[idx].seconds;
            st.session_elapsed = 0.0;
        }
    ));

    // Sound switch inside popover
    sound_switch.connect_active_notify(glib::clone!(
        #[strong]
        state,
        #[strong]
        synth,
        #[weak]
        mode_dropdown,
        move |sw| {
            let active = sw.is_active();
            let d = {
                let mut st = state.borrow_mut();
                st.sound_enabled = active;
                synth.update(st.running && st.sound_enabled, st.breath_level(), st.phase);
                st.durations
            };
            save_config(mode_dropdown.selected(), d, active);
        }
    ));

    // Mode selection inside popover
    let updating_spins = Rc::new(Cell::new(false));
    let update_labels_mode = update_labels.clone();
    let sync_ui_mode = sync_ui_chrome.clone();
    mode_dropdown.connect_selected_notify(glib::clone!(
        #[strong]
        state,
        #[strong]
        synth,
        #[strong]
        spins,
        #[strong]
        updating_spins,
        #[weak]
        app,
        #[weak]
        window,
        #[weak]
        area,
        move |dd| {
            let idx = (dd.selected() as usize).min(PRESETS.len() - 1);
            let preset = PRESETS[idx];
            if idx != PRESETS.len() - 1 {
                updating_spins.set(true);
                for (i, spin) in spins.iter().enumerate() {
                    spin.set_value(preset.durations[i]);
                }
                updating_spins.set(false);
                let mut st = state.borrow_mut();
                st.durations = preset.durations;
                st.reset();
                update_inhibit(&app, &window, &mut st);
                synth.update(false, 0.0, st.phase);
                let d = st.durations;
                let sound = st.sound_enabled;
                drop(st);
                save_config(idx as u32, d, sound);
                sync_ui_mode(true);
                update_labels_mode();
                area.queue_draw();
            } else {
                let st = state.borrow();
                save_config(idx as u32, st.durations, st.sound_enabled);
            }
        }
    ));

    // SpinButtons for custom durations
    for (i, spin) in spins.iter().enumerate() {
        let update_labels_spin = update_labels.clone();
        spin.connect_value_changed(glib::clone!(
            #[strong]
            state,
            #[strong]
            updating_spins,
            #[weak]
            mode_dropdown,
            #[weak]
            area,
            move |s| {
                if updating_spins.get() {
                    return;
                }
                let (d, sound) = {
                    let mut st = state.borrow_mut();
                    st.durations[i] = s.value();
                    st.advance_zero_phases();
                    (st.durations, st.sound_enabled)
                };
                let custom_idx = (PRESETS.len() - 1) as u32;
                if mode_dropdown.selected() != custom_idx {
                    mode_dropdown.set_selected(custom_idx);
                }
                save_config(custom_idx, d, sound);
                update_labels_spin();
                area.queue_draw();
            }
        ));
    }

    let quit_action = gio::SimpleAction::new("quit", None);
    quit_action.connect_activate(glib::clone!(
        #[weak]
        app,
        move |_, _| {
            app.quit();
        }
    ));
    app.add_action(&quit_action);

    window.present();
}
