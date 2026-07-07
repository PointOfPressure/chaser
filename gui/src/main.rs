//! Chaser — a native GTK4/libadwaita launcher and manager for Sober.
//!
//! One "active profile" drives everything: the Play page applies & launches it,
//! and the Profiles / FastFlags / Performance pages edit that same profile.
//! Edits save immediately; a `loading` guard prevents load→signal→save loops.

use adw::prelude::*;
use gtk::glib;
use serde_json::{Map, Value};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use chaser_core::{
    build_launch, fflags, paths, GraphicsMode, Profile, Renderer, SoberConfig, SoberInstall, Store,
};

const APP_ID: &str = "org.chaser.Chaser";

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run()
}

/// All widgets and state that pages share.
struct Ui {
    toasts: adw::ToastOverlay,
    // Play
    play_combo: adw::ComboRow,
    play_model: gtk::StringList,
    play_summary: adw::ActionRow,
    play_caption: gtk::Label,
    // Shared working state
    slugs: RefCell<Vec<String>>,
    current: RefCell<Profile>,
    current_slug: RefCell<String>,
    loading: Cell<bool>,
    // Profile editor
    name_row: adw::EntryRow,
    desc_row: adw::EntryRow,
    graphics_combo: adw::ComboRow,
    renderer_combo: adw::ComboRow,
    gamemode_sw: adw::SwitchRow,
    rpc_sw: adw::SwitchRow,
    hidpi_sw: adw::SwitchRow,
    gamepad_sw: adw::SwitchRow,
    // FastFlags
    fflags_view: gtk::TextView,
    // Performance
    mangohud_sw: adw::SwitchRow,
    env_view: gtk::TextView,
}

fn build_ui(app: &adw::Application) {
    // First run = no saved state yet; decide before ensure_defaults creates it.
    let first_run = paths::state_path().map(|p| !p.exists()).unwrap_or(false);
    if let Ok(store) = Store::open() {
        let _ = store.ensure_defaults();
    }

    let css = gtk::CssProvider::new();
    css.load_from_string(".launch-hero { font-size: 130%; padding: 14px 44px; }");
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Chaser")
        .default_width(960)
        .default_height(680)
        .build();

    let toasts = adw::ToastOverlay::new();
    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new(
        "Chaser",
        "Roblox on Linux via Sober",
    )));
    toolbar.add_top_bar(&header);

    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::None);
    stack.set_hexpand(true);
    stack.set_vexpand(true);

    let sidebar = gtk::StackSidebar::new();
    sidebar.set_stack(&stack);
    sidebar.set_width_request(190);

    let split = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    split.append(&sidebar);
    split.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    split.append(&stack);

    // Build the shared UI object with all its widgets up front.
    let ui = Rc::new(Ui {
        toasts: toasts.clone(),
        play_combo: adw::ComboRow::new(),
        play_model: gtk::StringList::new(&[]),
        play_summary: adw::ActionRow::new(),
        play_caption: gtk::Label::new(None),
        slugs: RefCell::new(Vec::new()),
        current: RefCell::new(Profile::new("Balanced")),
        current_slug: RefCell::new(String::new()),
        loading: Cell::new(false),
        name_row: adw::EntryRow::new(),
        desc_row: adw::EntryRow::new(),
        graphics_combo: adw::ComboRow::new(),
        renderer_combo: adw::ComboRow::new(),
        gamemode_sw: adw::SwitchRow::new(),
        rpc_sw: adw::SwitchRow::new(),
        hidpi_sw: adw::SwitchRow::new(),
        gamepad_sw: adw::SwitchRow::new(),
        fflags_view: gtk::TextView::new(),
        mangohud_sw: adw::SwitchRow::new(),
        env_view: gtk::TextView::new(),
    });

    stack.add_titled(&build_play_page(&ui), Some("play"), "Play");
    stack.add_titled(&build_profiles_page(&ui), Some("profiles"), "Profiles");
    stack.add_titled(&build_fflags_page(&ui), Some("fflags"), "FastFlags");
    stack.add_titled(&build_performance_page(&ui), Some("perf"), "Performance");
    stack.add_titled(&build_about_page(&ui), Some("about"), "About");
    stack.set_visible_child_name("play");
    // Dev/QA aid: open a specific page on startup (e.g. CHASER_PAGE=profiles).
    if let Ok(p) = std::env::var("CHASER_PAGE") {
        stack.set_visible_child_name(&p);
    }

    toolbar.set_content(Some(&split));
    toasts.set_child(Some(&toolbar));
    window.set_content(Some(&toasts));

    // Populate from disk and present.
    ui.refresh_profiles();
    ui.load_active();
    window.present();

    // Bloxstrap-style onboarding on first run (CHASER_ONBOARD=1 forces it for QA).
    if first_run || std::env::var("CHASER_ONBOARD").is_ok() {
        show_onboarding(&window, &ui);
    }
}

/// The four built-in presets, shared by onboarding and the Performance page.
const PRESETS: [(&str, &str, &str); 4] = [
    (
        "competitive-fps",
        "Competitive FPS",
        "Max frames, minimum eye-candy — uncapped FPS, low quality, no MSAA",
    ),
    (
        "balanced",
        "Balanced",
        "Sensible defaults with an uncapped framerate — a good starting point",
    ),
    (
        "cinematic",
        "Cinematic",
        "Highest fidelity — quality mode, full effects",
    ),
    (
        "potato",
        "Potato",
        "Rescue mode for weak GPUs — voxel lighting, lowest textures, no shadows",
    ),
];

// ---------------------------------------------------------------------------
// Pages
// ---------------------------------------------------------------------------

fn build_play_page(ui: &Rc<Ui>) -> gtk::Widget {
    let page = vbox();

    // Only revealed when Sober is missing.
    let banner = adw::Banner::new("Sober is not installed — Chaser needs it to launch Roblox");
    banner.set_button_label(Some("Copy install command"));
    banner.set_revealed(!SoberInstall::is_installed());
    {
        let ui = ui.clone();
        banner.connect_button_clicked(move |_| {
            if let Some(display) = gtk::gdk::Display::default() {
                display
                    .clipboard()
                    .set_text("flatpak install flathub org.vinegarhq.Sober");
                ui.toast("Copied — run it in a terminal, then restart Chaser");
            }
        });
    }
    page.append(&banner);

    // Hero: icon + one unambiguous launch button, Bloxstrap-style.
    let hero = gtk::Box::new(gtk::Orientation::Vertical, 12);
    hero.set_valign(gtk::Align::Center);
    hero.set_vexpand(true);
    hero.set_margin_top(12);
    hero.set_margin_bottom(12);

    if let Some(texture) = app_icon_texture() {
        let icon = gtk::Image::from_paintable(Some(&texture));
        icon.set_pixel_size(112);
        hero.append(&icon);
    }

    let launch_btn = gtk::Button::new();
    let content = adw::ButtonContent::new();
    content.set_icon_name("media-playback-start-symbolic");
    content.set_label("Launch Roblox");
    launch_btn.set_child(Some(&content));
    launch_btn.add_css_class("suggested-action");
    launch_btn.add_css_class("pill");
    launch_btn.add_css_class("launch-hero");
    launch_btn.set_halign(gtk::Align::Center);
    hero.append(&launch_btn);

    ui.play_caption.add_css_class("dim-label");
    ui.play_caption.set_halign(gtk::Align::Center);
    hero.append(&ui.play_caption);

    page.append(&hero);

    let prof_group = adw::PreferencesGroup::new();
    prof_group.set_title("Profile");
    ui.play_combo.set_title("Active profile");
    ui.play_combo.set_model(Some(&ui.play_model));
    ui.play_summary.set_title("Summary");
    prof_group.add(&ui.play_combo);
    prof_group.add(&ui.play_summary);
    page.append(&prof_group);

    // Selecting a profile makes it active and reloads every editor.
    {
        let ui = ui.clone();
        ui.play_combo.clone().connect_selected_notify(move |c| {
            if ui.loading.get() {
                return;
            }
            let idx = c.selected() as usize;
            let slug = ui.slugs.borrow().get(idx).cloned();
            if let Some(slug) = slug {
                if let Ok(store) = Store::open() {
                    let _ = store.set_active(&slug);
                }
                ui.load_active();
            }
        });
    }

    {
        let ui = ui.clone();
        launch_btn.connect_clicked(move |_| match ui.apply_current_to_sober(true) {
            Ok(name) => ui.toast(&format!("Applied '{name}' — launching Roblox")),
            Err(e) => ui.toast(&format!("Error: {e}")),
        });
    }

    let status = gtk::Label::new(Some(&format!(
        "{} · {}",
        sober_status_text(),
        SoberInstall::config_path().display()
    )));
    status.add_css_class("dim-label");
    status.add_css_class("caption");
    status.set_wrap(true);
    status.set_halign(gtk::Align::Center);
    page.append(&status);

    scrolled(&page)
}

/// The bundled app icon as a paintable (works even when not installed system-wide).
fn app_icon_texture() -> Option<gtk::gdk::Texture> {
    let bytes = glib::Bytes::from_static(include_bytes!("../../data/icons/org.chaser.Chaser.svg"));
    gtk::gdk::Texture::from_bytes(&bytes).ok()
}

/// First-run welcome: check Sober, pick a starting preset, apply it right away.
fn show_onboarding(parent: &adw::ApplicationWindow, ui: &Rc<Ui>) {
    let dialog = adw::Window::builder()
        .transient_for(parent)
        .modal(true)
        .default_width(540)
        .default_height(640)
        .title("Welcome to Chaser")
        .build();

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());

    let content = vbox();

    let hero = gtk::Box::new(gtk::Orientation::Vertical, 8);
    if let Some(texture) = app_icon_texture() {
        let icon = gtk::Image::from_paintable(Some(&texture));
        icon.set_pixel_size(96);
        hero.append(&icon);
    }
    let title = gtk::Label::new(Some("Welcome to Chaser"));
    title.add_css_class("title-1");
    hero.append(&title);
    let blurb = gtk::Label::new(Some(
        "The Bloxstrap of Linux — profiles, FastFlags and performance presets for Sober.",
    ));
    blurb.add_css_class("dim-label");
    blurb.set_wrap(true);
    blurb.set_justify(gtk::Justification::Center);
    hero.append(&blurb);
    content.append(&hero);

    let check = adw::PreferencesGroup::new();
    let sober_row = adw::ActionRow::new();
    sober_row.set_title("Sober");
    sober_row.set_subtitle(&sober_status_text());
    let installed = SoberInstall::is_installed();
    sober_row.add_prefix(&gtk::Image::from_icon_name(if installed {
        "emblem-ok-symbolic"
    } else {
        "dialog-warning-symbolic"
    }));
    check.add(&sober_row);
    content.append(&check);

    let choose = adw::PreferencesGroup::new();
    choose.set_title("Pick your starting profile");
    choose.set_description(Some("You can switch or customize it anytime."));
    let selected = Rc::new(RefCell::new("balanced".to_string()));
    let mut group_leader: Option<gtk::CheckButton> = None;
    for (slug, label, desc) in PRESETS {
        let row = adw::ActionRow::new();
        row.set_title(label);
        row.set_subtitle(desc);
        let radio = gtk::CheckButton::new();
        radio.set_valign(gtk::Align::Center);
        match &group_leader {
            Some(leader) => radio.set_group(Some(leader)),
            None => group_leader = Some(radio.clone()),
        }
        if slug == "balanced" {
            radio.set_active(true);
        }
        {
            let selected = selected.clone();
            radio.connect_toggled(move |r| {
                if r.is_active() {
                    *selected.borrow_mut() = slug.to_string();
                }
            });
        }
        row.add_prefix(&radio);
        row.set_activatable_widget(Some(&radio));
        choose.add(&row);
    }
    content.append(&choose);

    // Pinned bottom bar so the button is always visible regardless of scroll.
    let go = gtk::Button::with_label("Get started");
    go.add_css_class("suggested-action");
    go.add_css_class("pill");
    go.add_css_class("launch-hero");
    go.set_halign(gtk::Align::Center);
    go.set_margin_top(10);
    go.set_margin_bottom(14);
    toolbar.add_bottom_bar(&go);
    {
        let ui = ui.clone();
        let dialog = dialog.clone();
        go.connect_clicked(move |_| {
            let slug = selected.borrow().clone();
            if let Ok(store) = Store::open() {
                let _ = store.set_active(&slug);
            }
            ui.refresh_profiles();
            ui.load_active();
            match ui.apply_current_to_sober(false) {
                Ok(name) => ui.toast(&format!("'{name}' is live — hit Launch Roblox to play")),
                Err(e) => ui.toast(&format!("Profile set. Couldn't write Sober's config: {e}")),
            }
            dialog.close();
        });
    }

    toolbar.set_content(Some(&scrolled(&content)));
    dialog.set_content(Some(&toolbar));
    dialog.present();
}

fn build_profiles_page(ui: &Rc<Ui>) -> gtk::Widget {
    let page = vbox();

    // Manage row: New / Duplicate / Delete
    let manage = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let new_btn = gtk::Button::from_icon_name("list-add-symbolic");
    new_btn.set_tooltip_text(Some("New profile"));
    let dup_btn = gtk::Button::from_icon_name("edit-copy-symbolic");
    dup_btn.set_tooltip_text(Some("Duplicate active profile"));
    let del_btn = gtk::Button::from_icon_name("user-trash-symbolic");
    del_btn.set_tooltip_text(Some("Delete active profile"));
    del_btn.add_css_class("destructive-action");
    manage.append(&new_btn);
    manage.append(&dup_btn);
    manage.append(&del_btn);
    page.append(&manage);

    let group = adw::PreferencesGroup::new();
    group.set_title("Editing the active profile");
    group.set_description(Some(
        "Changes save instantly and apply the next time you launch.",
    ));

    ui.name_row.set_title("Name");
    ui.name_row.set_show_apply_button(true);
    ui.desc_row.set_title("Description");
    ui.desc_row.set_show_apply_button(true);

    ui.graphics_combo.set_title("Graphics mode");
    ui.graphics_combo.set_model(Some(&gtk::StringList::new(&[
        "Leave as-is",
        "Quality",
        "Balanced",
        "Performance",
    ])));
    ui.renderer_combo.set_title("Renderer");
    ui.renderer_combo.set_model(Some(&gtk::StringList::new(&[
        "Leave as-is",
        "Vulkan",
        "OpenGL",
    ])));

    ui.gamemode_sw.set_title("Feral GameMode");
    ui.gamemode_sw
        .set_subtitle("Sober's built-in CPU governor boost");
    ui.rpc_sw.set_title("Discord Rich Presence");
    ui.hidpi_sw.set_title("HiDPI scaling");
    ui.gamepad_sw.set_title("Gamepad support");

    group.add(&ui.name_row);
    group.add(&ui.desc_row);
    group.add(&ui.graphics_combo);
    group.add(&ui.renderer_combo);
    group.add(&ui.gamemode_sw);
    group.add(&ui.rpc_sw);
    group.add(&ui.hidpi_sw);
    group.add(&ui.gamepad_sw);
    page.append(&group);

    // --- wiring ---
    {
        let ui = ui.clone();
        ui.name_row
            .clone()
            .connect_apply(move |e| ui.rename_current(&e.text()));
    }
    {
        let ui = ui.clone();
        ui.desc_row.clone().connect_apply(move |e| {
            ui.mutate(|p| p.description = e.text().to_string());
        });
    }
    {
        let ui = ui.clone();
        ui.graphics_combo.clone().connect_selected_notify(move |c| {
            ui.mutate(|p| p.graphics_mode = index_to_graphics(c.selected()))
        });
    }
    {
        let ui = ui.clone();
        ui.renderer_combo.clone().connect_selected_notify(move |c| {
            ui.mutate(|p| p.renderer = index_to_renderer(c.selected()))
        });
    }
    connect_switch(ui, &ui.gamemode_sw, |p, v| p.enable_gamemode = Some(v));
    connect_switch(ui, &ui.rpc_sw, |p, v| p.discord_rpc = Some(v));
    connect_switch(ui, &ui.hidpi_sw, |p, v| p.enable_hidpi = Some(v));
    connect_switch(ui, &ui.gamepad_sw, |p, v| p.allow_gamepad = Some(v));

    {
        let ui = ui.clone();
        new_btn.connect_clicked(move |_| ui.new_profile());
    }
    {
        let ui = ui.clone();
        dup_btn.connect_clicked(move |_| ui.duplicate_current());
    }
    {
        let ui = ui.clone();
        del_btn.connect_clicked(move |_| ui.delete_current());
    }

    scrolled(&page)
}

fn build_fflags_page(ui: &Rc<Ui>) -> gtk::Widget {
    let page = vbox();

    let banner = adw::PreferencesGroup::new();
    banner.set_title("FastFlags");
    banner.set_description(Some(
        "Unsupported by VinegarHQ and can break games. The catalog below is curated and \
         conservative; add anything else in the raw editor. Flags apply to the active profile.",
    ));
    page.append(&banner);

    // Curated catalog: one row per flag with an "Add" button.
    let cat_group = adw::PreferencesGroup::new();
    cat_group.set_title("Curated catalog");
    for def in fflags::catalog() {
        let row = adw::ActionRow::new();
        row.set_title(def.name);
        row.set_subtitle(&format!(
            "[{} · {}] {}",
            def.category,
            def.risk.label(),
            def.description
        ));
        row.set_subtitle_lines(3);
        let add = gtk::Button::with_label("Add");
        add.set_valign(gtk::Align::Center);
        add.add_css_class("flat");
        let name = def.name.to_string();
        let suggested = def.suggested.clone();
        let ui2 = ui.clone();
        add.connect_clicked(move |_| {
            ui2.add_fflag(&name, suggested.clone());
        });
        row.add_suffix(&add);
        cat_group.add(&row);
    }
    page.append(&cat_group);

    // Raw editor for the active profile's fflags object.
    let raw_group = adw::PreferencesGroup::new();
    raw_group.set_title("Raw editor (active profile's fflags)");
    ui.fflags_view.set_monospace(true);
    ui.fflags_view.set_top_margin(6);
    ui.fflags_view.set_left_margin(6);
    let raw_scroll = gtk::ScrolledWindow::new();
    raw_scroll.set_min_content_height(180);
    raw_scroll.set_child(Some(&ui.fflags_view));
    raw_scroll.add_css_class("card");
    page.append(&raw_group);
    page.append(&raw_scroll);

    let save_btn = gtk::Button::with_label("Save FastFlags");
    save_btn.add_css_class("pill");
    save_btn.set_halign(gtk::Align::Start);
    page.append(&save_btn);
    {
        let ui = ui.clone();
        save_btn.connect_clicked(move |_| ui.save_fflags_from_view());
    }

    scrolled(&page)
}

fn build_performance_page(ui: &Rc<Ui>) -> gtk::Widget {
    let page = vbox();

    let presets = adw::PreferencesGroup::new();
    presets.set_title("Quick presets");
    presets.set_description(Some(
        "Switch the active profile to a preset and write it to Sober.",
    ));
    for (slug, label, sub) in PRESETS {
        let row = adw::ActionRow::new();
        row.set_title(label);
        row.set_subtitle(sub);
        let btn = gtk::Button::with_label("Apply");
        btn.set_valign(gtk::Align::Center);
        btn.add_css_class("flat");
        let ui2 = ui.clone();
        let slug = slug.to_string();
        btn.connect_clicked(move |_| ui2.apply_preset(&slug));
        row.add_suffix(&btn);
        presets.add(&row);
    }
    page.append(&presets);

    let launch_group = adw::PreferencesGroup::new();
    launch_group.set_title("Launch options");
    ui.mangohud_sw.set_title("MangoHud overlay");
    ui.mangohud_sw
        .set_subtitle("Needs the MangoHud Flatpak extension installed");
    launch_group.add(&ui.mangohud_sw);
    page.append(&launch_group);
    connect_switch(ui, &ui.mangohud_sw, |p, v| p.mangohud = v);

    let env_group = adw::PreferencesGroup::new();
    env_group.set_title("Environment variables");
    env_group.set_description(Some(
        "One KEY=VALUE per line, passed via `flatpak run --env=`.",
    ));
    ui.env_view.set_monospace(true);
    ui.env_view.set_top_margin(6);
    ui.env_view.set_left_margin(6);
    let env_scroll = gtk::ScrolledWindow::new();
    env_scroll.set_min_content_height(120);
    env_scroll.set_child(Some(&ui.env_view));
    env_scroll.add_css_class("card");
    page.append(&env_group);
    page.append(&env_scroll);

    let save_env = gtk::Button::with_label("Save environment");
    save_env.add_css_class("pill");
    save_env.set_halign(gtk::Align::Start);
    page.append(&save_env);
    {
        let ui = ui.clone();
        save_env.connect_clicked(move |_| ui.save_env_from_view());
    }

    scrolled(&page)
}

fn build_about_page(ui: &Rc<Ui>) -> gtk::Widget {
    let page = vbox();

    let info = adw::PreferencesGroup::new();
    info.set_title("Chaser");
    let ver = adw::ActionRow::new();
    ver.set_title("Version");
    ver.set_subtitle(env!("CARGO_PKG_VERSION"));
    info.add(&ver);
    let sober = adw::ActionRow::new();
    sober.set_title("Sober");
    sober.set_subtitle(&sober_status_text());
    info.add(&sober);
    let credit = adw::ActionRow::new();
    credit.set_title("Built with");
    credit.set_subtitle("Claude (Anthropic)");
    info.add(&credit);
    page.append(&info);

    let backups = adw::PreferencesGroup::new();
    backups.set_title("Safety");
    backups.set_description(Some(
        "Chaser backs up Sober's config before every change, under \
         ~/.config/chaser/backups.",
    ));
    let restore = adw::ActionRow::new();
    restore.set_title("Restore latest backup");
    restore.set_subtitle("Copy the newest backup back over Sober's config");
    let restore_btn = gtk::Button::with_label("Restore");
    restore_btn.set_valign(gtk::Align::Center);
    restore_btn.add_css_class("pill");
    restore.add_suffix(&restore_btn);
    backups.add(&restore);
    page.append(&backups);
    {
        let ui = ui.clone();
        restore_btn.connect_clicked(move |_| match restore_latest_backup() {
            Ok(Some(name)) => ui.toast(&format!("Restored {name}")),
            Ok(None) => ui.toast("No backups found yet"),
            Err(e) => ui.toast(&format!("Error: {e}")),
        });
    }

    let disclaimer = adw::PreferencesGroup::new();
    disclaimer.set_title("Disclaimer");
    disclaimer.set_description(Some(
        "Chaser is unofficial and not affiliated with Roblox or VinegarHQ. It only edits Sober's \
         config and launches it — it never modifies the Roblox client. Use FastFlags at your own risk.",
    ));
    page.append(&disclaimer);

    scrolled(&page)
}

// ---------------------------------------------------------------------------
// Ui behaviour
// ---------------------------------------------------------------------------

impl Ui {
    fn toast(&self, msg: &str) {
        self.toasts.add_toast(adw::Toast::new(msg));
    }

    /// Reload the profile list, rebuild the Play combo, and select the active one.
    fn refresh_profiles(&self) {
        let profiles = load_profiles();
        *self.slugs.borrow_mut() = profiles.iter().map(|p| p.slug()).collect();
        let names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();

        self.loading.set(true);
        self.play_model.splice(0, self.play_model.n_items(), &names);
        let active = Store::open()
            .ok()
            .and_then(|s| s.active_slug().ok().flatten());
        if let Some(active) = active {
            if let Some(idx) = self.slugs.borrow().iter().position(|s| *s == active) {
                self.play_combo.set_selected(idx as u32);
            }
        }
        self.loading.set(false);
    }

    /// Load the active profile into `current` and every editor widget.
    fn load_active(&self) {
        let profile = Store::open()
            .ok()
            .and_then(|s| s.active().ok().flatten())
            .unwrap_or_else(|| Profile::new("Balanced"));

        self.loading.set(true);
        *self.current_slug.borrow_mut() = profile.slug();

        self.name_row.set_text(&profile.name);
        self.desc_row.set_text(&profile.description);
        self.graphics_combo
            .set_selected(graphics_to_index(profile.graphics_mode));
        self.renderer_combo
            .set_selected(renderer_to_index(profile.renderer));
        self.gamemode_sw
            .set_active(profile.enable_gamemode.unwrap_or(false));
        self.rpc_sw.set_active(profile.discord_rpc.unwrap_or(false));
        self.hidpi_sw
            .set_active(profile.enable_hidpi.unwrap_or(false));
        self.gamepad_sw
            .set_active(profile.allow_gamepad.unwrap_or(false));
        self.mangohud_sw.set_active(profile.mangohud);
        self.fflags_view
            .buffer()
            .set_text(&fflags_to_pretty(&profile.fflags));
        self.env_view.buffer().set_text(&env_to_text(&profile.env));
        self.play_summary.set_subtitle(&describe(&profile));
        self.play_caption.set_text(&format!(
            "Applies '{}' to Sober, then starts Roblox",
            profile.name
        ));

        *self.current.borrow_mut() = profile;
        self.loading.set(false);
    }

    /// Apply a closure to the working profile and persist it.
    fn mutate(&self, f: impl FnOnce(&mut Profile)) {
        if self.loading.get() {
            return;
        }
        {
            let mut p = self.current.borrow_mut();
            f(&mut p);
        }
        self.save_current();
    }

    fn save_current(&self) {
        if let Ok(store) = Store::open() {
            let p = self.current.borrow();
            if store.save(&p).is_ok() {
                self.play_summary.set_subtitle(&describe(&p));
            }
        }
    }

    fn rename_current(&self, new_name: &str) {
        if self.loading.get() {
            return;
        }
        let old_slug = self.current_slug.borrow().clone();
        let store = match Store::open() {
            Ok(s) => s,
            Err(_) => return,
        };
        // First persist any unsaved field state, then rename via the store,
        // which refuses to clobber a different existing profile.
        let _ = store.save(&self.current.borrow());
        match store.rename(&old_slug, new_name) {
            Ok(p) => {
                *self.current_slug.borrow_mut() = p.slug();
                *self.current.borrow_mut() = p;
                self.refresh_profiles();
                self.load_active();
                self.toast("Renamed");
            }
            Err(e) => {
                // Restore the entry to the real name so the UI doesn't lie.
                self.load_active();
                self.toast(&format!("Rename failed: {e}"));
            }
        }
    }

    fn new_profile(&self) {
        let store = match Store::open() {
            Ok(s) => s,
            Err(_) => return,
        };
        let mut p = Profile::new(unique_name("New Profile"));
        p.graphics_mode = Some(GraphicsMode::Balanced);
        p.renderer = Some(Renderer::Vulkan);
        let _ = store.save(&p);
        let _ = store.set_active(&p.slug());
        self.refresh_profiles();
        self.load_active();
        self.toast("Created new profile");
    }

    fn duplicate_current(&self) {
        let store = match Store::open() {
            Ok(s) => s,
            Err(_) => return,
        };
        let mut p = self.current.borrow().clone();
        p.name = unique_name(&format!("{} copy", p.name));
        let _ = store.save(&p);
        let _ = store.set_active(&p.slug());
        self.refresh_profiles();
        self.load_active();
        self.toast("Duplicated profile");
    }

    fn delete_current(&self) {
        let store = match Store::open() {
            Ok(s) => s,
            Err(_) => return,
        };
        if store.list().map(|l| l.len()).unwrap_or(0) <= 1 {
            self.toast("Can't delete the last profile");
            return;
        }
        let slug = self.current_slug.borrow().clone();
        let _ = store.delete(&slug);
        self.refresh_profiles();
        self.load_active();
        self.toast("Deleted profile");
    }

    fn add_fflag(&self, name: &str, value: Value) {
        self.mutate(|p| {
            p.fflags.insert(name.to_string(), value);
        });
        // Reflect the change in the raw editor immediately.
        self.fflags_view
            .buffer()
            .set_text(&fflags_to_pretty(&self.current.borrow().fflags));
        self.toast(&format!("Added {name}"));
    }

    fn save_fflags_from_view(&self) {
        let text = buffer_text(&self.fflags_view);
        match parse_fflags(&text) {
            Ok(map) => {
                self.mutate(|p| p.fflags = map);
                self.toast("FastFlags saved");
            }
            Err(e) => self.toast(&format!("Invalid JSON: {e}")),
        }
    }

    fn save_env_from_view(&self) {
        let text = buffer_text(&self.env_view);
        match parse_env(&text) {
            Ok(env) => {
                self.mutate(|p| p.env = env);
                self.toast("Environment saved");
            }
            Err(e) => self.toast(&format!("Invalid line: {e}")),
        }
    }

    fn apply_preset(&self, slug: &str) {
        if let Ok(store) = Store::open() {
            if store.load(slug).is_ok() {
                let _ = store.set_active(slug);
                self.refresh_profiles();
                self.load_active();
                match self.apply_current_to_sober(false) {
                    Ok(name) => self.toast(&format!("Applied preset '{name}' to Sober")),
                    Err(e) => self.toast(&format!("Error: {e}")),
                }
            }
        }
    }

    /// Write the working profile into Sober's config (with backup) and
    /// optionally launch. Returns the profile name.
    fn apply_current_to_sober(&self, launch: bool) -> anyhow::Result<String> {
        if !SoberInstall::is_installed() {
            anyhow::bail!("Sober is not installed");
        }
        let profile = self.current.borrow().clone();
        let path = SoberInstall::config_path();
        let mut cfg = SoberConfig::load_or_default(&path)?;
        cfg.apply_profile(&profile);
        cfg.save(&path)?;
        if launch {
            build_launch(&profile, None).to_command().spawn()?;
        }
        Ok(profile.name)
    }
}

/// Wire a SwitchRow's toggle to a profile mutation.
fn connect_switch(ui: &Rc<Ui>, sw: &adw::SwitchRow, set: fn(&mut Profile, bool)) {
    let ui = ui.clone();
    sw.connect_active_notify(move |s| {
        let v = s.is_active();
        ui.mutate(move |p| set(p, v));
    });
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn vbox() -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 14);
    b.set_margin_top(18);
    b.set_margin_bottom(18);
    b.set_margin_start(18);
    b.set_margin_end(18);
    b
}

fn scrolled(child: &impl IsA<gtk::Widget>) -> gtk::Widget {
    let s = gtk::ScrolledWindow::new();
    s.set_child(Some(child));
    s.set_vexpand(true);
    s.upcast()
}

fn load_profiles() -> Vec<Profile> {
    Store::open().and_then(|s| s.list()).unwrap_or_default()
}

fn describe(p: &Profile) -> String {
    let g = p.graphics_mode.map(|m| m.label()).unwrap_or("—");
    let r = p.renderer.map(|r| r.label()).unwrap_or("—");
    format!("{g} · {r} · {} FastFlags", p.fflags.len())
}

fn sober_status_text() -> String {
    match SoberInstall::detect() {
        Ok(s) => format!(
            "Installed · version {}",
            s.version.as_deref().unwrap_or("unknown")
        ),
        Err(_) => "Not installed — run: flatpak install flathub org.vinegarhq.Sober".to_string(),
    }
}

fn graphics_to_index(m: Option<GraphicsMode>) -> u32 {
    match m {
        None => 0,
        Some(GraphicsMode::Quality) => 1,
        Some(GraphicsMode::Balanced) => 2,
        Some(GraphicsMode::Performance) => 3,
    }
}

fn index_to_graphics(i: u32) -> Option<GraphicsMode> {
    match i {
        1 => Some(GraphicsMode::Quality),
        2 => Some(GraphicsMode::Balanced),
        3 => Some(GraphicsMode::Performance),
        _ => None,
    }
}

fn renderer_to_index(r: Option<Renderer>) -> u32 {
    match r {
        None => 0,
        Some(Renderer::Vulkan) => 1,
        Some(Renderer::OpenGl) => 2,
    }
}

fn index_to_renderer(i: u32) -> Option<Renderer> {
    match i {
        1 => Some(Renderer::Vulkan),
        2 => Some(Renderer::OpenGl),
        _ => None,
    }
}

fn fflags_to_pretty(map: &Map<String, Value>) -> String {
    serde_json::to_string_pretty(&Value::Object(map.clone())).unwrap_or_else(|_| "{}".to_string())
}

fn parse_fflags(text: &str) -> anyhow::Result<Map<String, Value>> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(trimmed)? {
        Value::Object(m) => Ok(m),
        _ => anyhow::bail!("expected a JSON object"),
    }
}

fn env_to_text(env: &BTreeMap<String, String>) -> String {
    env.iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_env(text: &str) -> anyhow::Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (k, v) = line
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("'{line}' is not KEY=VALUE"))?;
        out.insert(k.trim().to_string(), v.trim().to_string());
    }
    Ok(out)
}

fn buffer_text(view: &gtk::TextView) -> String {
    let buf = view.buffer();
    buf.text(&buf.start_iter(), &buf.end_iter(), false)
        .to_string()
}

/// A profile name whose slug doesn't collide with an existing profile.
fn unique_name(base: &str) -> String {
    let existing: Vec<String> = load_profiles().iter().map(|p| p.slug()).collect();
    let base_slug = chaser_core::models::slugify(base);
    if !existing.contains(&base_slug) {
        return base.to_string();
    }
    for n in 2..1000 {
        let candidate = format!("{base} {n}");
        if !existing.contains(&chaser_core::models::slugify(&candidate)) {
            return candidate;
        }
    }
    base.to_string()
}

fn restore_latest_backup() -> anyhow::Result<Option<String>> {
    let dir = paths::backup_dir()?;
    let mut newest: Option<(std::time::SystemTime, std::path::PathBuf)> = None;
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let modified = entry.metadata()?.modified()?;
        if newest.as_ref().map(|(t, _)| modified > *t).unwrap_or(true) {
            newest = Some((modified, entry.path()));
        }
    }
    match newest {
        Some((_, path)) => {
            std::fs::copy(&path, SoberInstall::config_path())?;
            Ok(Some(
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
            ))
        }
        None => Ok(None),
    }
}
