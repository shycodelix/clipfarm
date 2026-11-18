use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use clipfarm_core::analyze::AnalyzeOptions;
use clipfarm_core::export::{ExportOptions, ExportOutcome, Quality};
use clipfarm_core::model::{Clip, ExportKind, Niche, Platform, Project, MAX_CLIP_SECONDS};
use clipfarm_core::pipeline;
use clipfarm_core::project::{self, ProjectSummary};
use clipfarm_core::refine;
use clipfarm_core::tools::{ToolStatus, Tools};
use clipfarm_core::trends;
use clipfarm_core::util::{human_bytes, human_duration};
use clipfarm_core::AppConfig;
use egui::{Color32, Context, Layout, Margin, RichText, ScrollArea, Sense, TextureHandle, Ui};
use uuid::Uuid;

use crate::theme::{self, Palette};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Farm,
    Clips,
    Trends,
    Tuning,
}

impl Screen {
    fn label(self) -> &'static str {
        match self {
            Screen::Farm => "The farm",
            Screen::Clips => "The clips",
            Screen::Trends => "Trend desk",
            Screen::Tuning => "Tuning",
        }
    }
}

struct Job {
    label: String,
    stage: String,
    message: String,
    fraction: f32,
    started: Instant,
    /// Preview renders open in the system player and are not recorded as exports.
    preview: bool,
}

enum JobEvent {
    Progress {
        stage: String,
        fraction: f32,
        message: String,
    },
    Harvest {
        project: Box<Project>,
        warnings: Vec<String>,
    },
    Exported(Box<ExportOutcome>),
    Failed(String),
}

struct Toast {
    text: String,
    good: bool,
    at: Instant,
}

pub struct ClipfarmApp {
    dark: bool,
    screen: Screen,
    project: Option<Project>,
    projects: Vec<ProjectSummary>,
    projects_error: Option<String>,
    selected: usize,

    input: String,
    name: String,
    niche: Niche,
    platform: Platform,
    target_clips: usize,
    max_seconds: f64,

    config: AppConfig,
    statuses: Vec<ToolStatus>,
    tools_error: Option<String>,

    job: Option<Job>,
    rx: Option<Receiver<JobEvent>>,
    thumbnails: HashMap<Uuid, TextureHandle>,

    toast: Option<Toast>,
    warnings: Vec<String>,
    refine_text: String,
    refine_log: Vec<String>,
    quality: Quality,
}

impl ClipfarmApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let config = AppConfig::load();
        let (statuses, tools_error) = match Tools::discover(&config) {
            Ok(tools) => (tools.status(), None),
            Err(error) => (Vec::new(), Some(format!("{error}"))),
        };
        let mut app = Self {
            dark: true,
            screen: Screen::Farm,
            project: None,
            projects: Vec::new(),
            projects_error: None,
            selected: 0,
            input: String::new(),
            name: String::new(),
            niche: Niche::Podcast,
            platform: Platform::TikTok,
            target_clips: 6,
            max_seconds: MAX_CLIP_SECONDS,
            config,
            statuses,
            tools_error,
            job: None,
            rx: None,
            thumbnails: HashMap::new(),
            toast: None,
            warnings: Vec::new(),
            refine_text: String::new(),
            refine_log: Vec::new(),
            quality: Quality::Balanced,
        };
        app.refresh_projects();
        app.job = None;
        let _ = cc;
        app
    }

    fn refresh_projects(&mut self) {
        match project::list_projects() {
            Ok(projects) => {
                self.projects = projects;
                self.projects_error = None;
            }
            Err(error) => self.projects_error = Some(format!("{error:#}")),
        }
    }

    fn toast(&mut self, text: impl Into<String>, good: bool) {
        self.toast = Some(Toast {
            text: text.into(),
            good,
            at: Instant::now(),
        });
    }

    fn save(&mut self) {
        if let Some(project) = self.project.as_mut() {
            if let Err(error) = project::save_project(project) {
                self.toast(format!("Could not save: {error}"), false);
            }
        }
    }

    fn start_harvest(&mut self) {
        if self.job.is_some() {
            return;
        }
        let input = self.input.trim().to_string();
        if input.is_empty() {
            self.toast("Give me a file or a link first.", false);
            return;
        }
        let config = self.config.clone();
        let name = self.name.trim().to_string();
        let niche = self.niche;
        let platform = self.platform;
        let target_clips = self.target_clips;
        let max_seconds = self.max_seconds.min(MAX_CLIP_SECONDS);
        let (tx, rx) = std::sync::mpsc::channel();
        self.rx = Some(rx);
        self.job = Some(Job {
            label: "Harvesting".into(),
            stage: "starting".into(),
            message: "warming up the tractor".into(),
            fraction: 0.0,
            started: Instant::now(),
            preview: false,
        });
        self.warnings.clear();
        self.thumbnails.clear();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<(Project, Vec<String>)> {
                let tools = Tools::discover(&config)?;
                let mut project = pipeline::create_project(
                    &tools,
                    &input,
                    if name.is_empty() { None } else { Some(name) },
                    niche,
                    platform,
                    &mut |fraction, message| {
                        let _ = tx.send(JobEvent::Progress {
                            stage: "ingest".into(),
                            fraction: fraction * 0.18,
                            message: message.to_string(),
                        });
                    },
                )?;
                let options = AnalyzeOptions {
                    target_clips,
                    max_seconds,
                    min_seconds: 10.0,
                };
                let report =
                    pipeline::analyze(&tools, &mut project, &options, &mut |fraction, message| {
                        let _ = tx.send(JobEvent::Progress {
                            stage: "analyze".into(),
                            fraction: 0.18 + fraction * 0.80,
                            message: message.to_string(),
                        });
                    })?;
                Ok((project, report.warnings))
            })();
            let event = match result {
                Ok((project, warnings)) => JobEvent::Harvest {
                    project: Box::new(project),
                    warnings,
                },
                Err(error) => JobEvent::Failed(format!("{error:#}")),
            };
            let _ = tx.send(event);
        });
    }

    fn start_reanalyze(&mut self) {
        if self.job.is_some() {
            return;
        }
        let Some(mut project) = self.project.clone() else {
            return;
        };
        let config = self.config.clone();
        let target_clips = self.target_clips;
        let max_seconds = self.max_seconds.min(MAX_CLIP_SECONDS);
        let (tx, rx) = std::sync::mpsc::channel();
        self.rx = Some(rx);
        self.job = Some(Job {
            label: "Re-harvesting".into(),
            stage: "analysis".into(),
            message: "asking the footage for a second draft".into(),
            fraction: 0.0,
            started: Instant::now(),
            preview: false,
        });
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<(Project, Vec<String>)> {
                let tools = Tools::discover(&config)?;
                let options = AnalyzeOptions {
                    target_clips,
                    max_seconds,
                    min_seconds: 10.0,
                };
                let report =
                    pipeline::analyze(&tools, &mut project, &options, &mut |fraction, message| {
                        let _ = tx.send(JobEvent::Progress {
                            stage: "analyze".into(),
                            fraction: fraction * 0.98,
                            message: message.to_string(),
                        });
                    })?;
                Ok((project, report.warnings))
            })();
            let event = match result {
                Ok((project, warnings)) => JobEvent::Harvest {
                    project: Box::new(project),
                    warnings,
                },
                Err(error) => JobEvent::Failed(format!("{error:#}")),
            };
            let _ = tx.send(event);
        });
    }

    /// Render a small file and hand it to the system player. Not an export:
    /// nothing is recorded against the clip.
    fn start_preview(&mut self) {
        if self.job.is_some() {
            return;
        }
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let Some(clip) = project.clips.get(self.selected).cloned() else {
            self.toast("Pick a clip first.", false);
            return;
        };
        let preview_dir = project::data_dir().join("previews");
        if let Err(error) = std::fs::create_dir_all(&preview_dir) {
            self.toast(format!("Could not create {preview_dir:?}: {error}"), false);
            return;
        }
        let output = preview_dir.join(format!("{}-preview.mp4", &clip.id.to_string()[..8]));
        let config = self.config.clone();
        let project_snapshot = project.clone();
        let options = ExportOptions {
            kind: ExportKind::Rendered,
            output,
            quality: Quality::Small,
        };
        let (tx, rx) = std::sync::mpsc::channel();
        self.rx = Some(rx);
        self.job = Some(Job {
            label: "Previewing".into(),
            stage: "export".into(),
            message: "rendering a small file to watch".into(),
            fraction: 0.0,
            started: Instant::now(),
            preview: true,
        });
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<ExportOutcome> {
                let tools = Tools::discover(&config)?;
                clipfarm_core::export_clip(
                    &tools,
                    &project_snapshot,
                    &clip,
                    &options,
                    &mut |fraction, message| {
                        let _ = tx.send(JobEvent::Progress {
                            stage: "export".into(),
                            fraction,
                            message: message.to_string(),
                        });
                    },
                )
            })();
            let event = match result {
                Ok(outcome) => JobEvent::Exported(Box::new(outcome)),
                Err(error) => JobEvent::Failed(format!("{error:#}")),
            };
            let _ = tx.send(event);
        });
    }

    fn start_export(&mut self, kind: ExportKind) {
        if self.job.is_some() {
            return;
        }
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let Some(clip) = project.clips.get(self.selected).cloned() else {
            self.toast("Pick a clip first.", false);
            return;
        };
        let output = pipeline::export_path(project, &clip, self.selected, kind);
        let clip_id = clip.id;
        let config = self.config.clone();
        let quality = self.quality;
        let project_snapshot = project.clone();
        let options = ExportOptions {
            kind,
            output,
            quality,
        };
        let (tx, rx) = std::sync::mpsc::channel();
        self.rx = Some(rx);
        self.job = Some(Job {
            label: if kind == ExportKind::Raw {
                "Exporting raw".into()
            } else {
                "Rendering short".into()
            },
            stage: "export".into(),
            message: if kind == ExportKind::Raw {
                "copying streams, no re-encode".into()
            } else {
                "burning pixels politely".into()
            },
            fraction: 0.0,
            started: Instant::now(),
            preview: false,
        });
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<ExportOutcome> {
                let tools = Tools::discover(&config)?;
                clipfarm_core::export_clip(
                    &tools,
                    &project_snapshot,
                    &clip,
                    &options,
                    &mut |fraction, message| {
                        let _ = tx.send(JobEvent::Progress {
                            stage: "export".into(),
                            fraction,
                            message: message.to_string(),
                        });
                    },
                )
            })();
            let event = match result {
                Ok(outcome) => JobEvent::Exported(Box::new(outcome)),
                Err(error) => JobEvent::Failed(format!("{error:#}")),
            };
            let _ = tx.send(event);
            let _ = clip_id;
        });
    }

    fn poll_jobs(&mut self, ctx: &Context) {
        let mut finished = false;
        let mut events = Vec::new();
        if let Some(rx) = self.rx.as_ref() {
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }
        }
        for event in events {
            match event {
                JobEvent::Progress {
                    stage,
                    fraction,
                    message,
                } => {
                    if let Some(job) = self.job.as_mut() {
                        job.stage = stage;
                        job.fraction = fraction.clamp(0.0, 1.0);
                        job.message = message;
                    }
                }
                JobEvent::Harvest { project, warnings } => {
                    let clips = project.clips.len();
                    let name = project.name.clone();
                    self.project = Some(*project);
                    self.selected = 0;
                    self.screen = Screen::Clips;
                    self.warnings = warnings;
                    self.thumbnails.clear();
                    self.refresh_projects();
                    self.toast(format!("{clips} clips from “{name}”. Go be picky."), true);
                    finished = true;
                }
                JobEvent::Exported(outcome) => {
                    let preview = self.job.as_ref().map(|job| job.preview).unwrap_or(false);
                    if preview {
                        open_path(&outcome.path);
                        self.toast("Preview opened in your player.", true);
                        finished = true;
                        continue;
                    }
                    let message = format!(
                        "{} · {} · {}",
                        match outcome.kind {
                            ExportKind::Raw => "raw export",
                            ExportKind::Rendered => "rendered short",
                        },
                        human_bytes(outcome.bytes),
                        human_duration(outcome.duration)
                    );
                    if let Some(project) = self.project.as_mut() {
                        if let Some(clip) = project
                            .clips
                            .iter_mut()
                            .find(|clip| clip.id == outcome.clip_id)
                        {
                            clipfarm_core::export::record_export(clip, &outcome);
                        }
                        if let Err(error) = project::save_project(project) {
                            self.toast(format!("Exported, but could not save: {error}"), false);
                        }
                    }
                    self.refresh_projects();
                    self.toast(message, true);
                    finished = true;
                }
                JobEvent::Failed(error) => {
                    self.toast(error, false);
                    finished = true;
                }
            }
        }
        if finished {
            self.job = None;
            self.rx = None;
        } else if self.job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(90));
        }
        if let Some(toast) = &self.toast {
            if toast.at.elapsed() > Duration::from_secs(9) {
                self.toast = None;
            } else {
                ctx.request_repaint_after(Duration::from_millis(400));
            }
        }
    }

    fn load(&mut self, id: Uuid) {
        match project::load_project(&id.to_string()) {
            Ok(project) => {
                self.project = Some(project);
                self.selected = 0;
                self.thumbnails.clear();
                self.warnings.clear();
                self.refine_log.clear();
                self.screen = Screen::Clips;
            }
            Err(error) => self.toast(format!("{error:#}"), false),
        }
    }

    fn pick_input(&mut self, path: &Path) {
        self.input = path.display().to_string();
        if let Some(stem) = path.file_stem() {
            self.name = stem.to_string_lossy().to_string();
        }
    }

    fn ensure_thumbnails(&mut self, ctx: &Context) {
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let pending: Vec<(Uuid, PathBuf)> = project
            .clips
            .iter()
            .filter(|clip| !self.thumbnails.contains_key(&clip.id))
            .filter_map(|clip| clip.thumbnail.clone().map(|path| (clip.id, path)))
            .collect();
        for (id, path) in pending {
            let Ok(dynamic) = image::open(&path) else {
                continue;
            };
            let rgba = dynamic.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let image = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
            let texture =
                ctx.load_texture(format!("thumb-{id}"), image, egui::TextureOptions::LINEAR);
            self.thumbnails.insert(id, texture);
        }
    }

    fn apply_refine(&mut self) {
        let Some(project) = self.project.as_mut() else {
            return;
        };
        let instruction = self.refine_text.trim().to_string();
        if instruction.is_empty() {
            return;
        }
        let snapshot = project.clone();
        let llm = if self.config.llm_allowed() {
            Some((
                self.config.llm_endpoint.as_str(),
                self.config.llm_model.as_str(),
            ))
        } else {
            None
        };
        let Some(clip) = project.clips.get_mut(self.selected) else {
            return;
        };
        match refine::refine(&snapshot, clip, &instruction, llm) {
            Some(changes) => {
                self.refine_log = changes;
                self.refine_text.clear();
                if let Err(error) = project::save_project(project) {
                    self.toast(format!("Could not save: {error}"), false);
                }
            }
            None => {
                self.refine_log.clear();
                self.toast(
                    "I did not catch an edit in there. Try “30 seconds”, “start 2s later”, “captions off”.",
                    false,
                );
            }
        }
    }

    /// Sidebar: brand, navigation, and the engine pulse.
    fn sidebar(&mut self, ui: &mut Ui, p: &Palette) {
        ui.set_width(206.0);
        ui.horizontal(|ui| {
            theme::logo(ui, p, 38.0);
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("clipfarm").size(20.0).strong().color(p.ink));
                ui.label(RichText::new("local clip studio").size(10.5).color(p.faint));
            });
        });
        ui.add_space(22.0);
        theme::kicker(ui, p, "Workspace");
        ui.add_space(4.0);
        for screen in [Screen::Farm, Screen::Clips, Screen::Trends, Screen::Tuning] {
            let selected = self.screen == screen;
            let response = ui.add_sized(
                [194.0, 34.0],
                egui::Button::new(RichText::new(screen.label()).size(13.0).color(if selected {
                    p.ink
                } else {
                    p.muted
                }))
                .fill(if selected { p.card_alt } else { p.panel })
                .frame(false),
            );
            if response.clicked() {
                self.screen = screen;
            }
        }
        ui.add_space(24.0);
        theme::kicker(ui, p, "Today's grain");
        ui.add_space(4.0);
        let clips = self
            .project
            .as_ref()
            .map(|project| project.clips.len())
            .unwrap_or(0);
        ui.label(
            RichText::new(if clips == 0 {
                "no clips in the barn".to_string()
            } else {
                format!("{clips} clips in the barn")
            })
            .size(12.0)
            .color(p.muted),
        );
        if let Some(project) = &self.project {
            ui.label(
                RichText::new(format!(
                    "{} · {}",
                    project.niche.label(),
                    project.platform.label()
                ))
                .size(11.0)
                .color(p.faint),
            );
        }
        ui.with_layout(Layout::bottom_up(egui::Align::Min), |ui| {
            let (dot, label) = if self.tools_error.is_none() {
                (p.mint, "engines ready".to_string())
            } else {
                (p.amber, "engine missing".to_string())
            };
            ui.horizontal(|ui| {
                ui.label(RichText::new("●").size(10.0).color(dot));
                ui.label(RichText::new(label).size(11.0).color(p.muted));
            });
            ui.label(
                RichText::new("no cloud · no credits · v0.2")
                    .size(10.0)
                    .color(p.faint),
            );
            ui.add_space(10.0);
            ui.label(
                RichText::new("Every render is free because nothing leaves this machine.")
                    .size(10.5)
                    .color(p.faint),
            );
        });
    }

    fn topbar(&mut self, ui: &mut Ui, p: &Palette) {
        ui.horizontal(|ui| {
            let project_name = self
                .project
                .as_ref()
                .map(|project| project.name.clone())
                .unwrap_or_else(|| "No harvest loaded".into());
            theme::kicker(ui, p, &project_name);
            ui.add_space(6.0);
            if let Some(project) = &self.project {
                theme::chip(
                    ui,
                    if p.dark {
                        Color32::from_rgb(38, 33, 27)
                    } else {
                        Color32::from_rgb(248, 230, 204)
                    },
                    p.amber,
                    project.platform.label(),
                );
            }
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                if theme::quiet(ui, p, if self.dark { "☀ light" } else { "☾ dark" }).clicked() {
                    self.dark = !self.dark;
                }
                if theme::secondary(ui, p, "New harvest").clicked() {
                    self.project = None;
                    self.screen = Screen::Farm;
                }
            });
        });
        ui.add_space(10.0);
    }

    fn screen_farm(&mut self, ui: &mut Ui, p: &Palette) {
        ui.label(
            RichText::new("Feed the algorithm something worth watching.")
                .size(30.0)
                .strong()
                .color(p.ink),
        );
        ui.add_space(2.0);
        ui.label(
            RichText::new("Drop a VOD, paste a link, or point at a file. The dull parts get removed. The good parts get captions.")
                .size(13.5)
                .color(p.muted),
        );
        ui.add_space(18.0);

        theme::card_frame(p).show(ui, |ui| {
            ui.set_width(ui.available_width().min(880.0));
            theme::kicker(ui, p, "Source");
            let input = ui.add(
                egui::TextEdit::singleline(&mut self.input)
                    .hint_text("https://youtube.com/watch?v=…  or  ~/Videos/interview.mp4")
                    .desired_width(f32::INFINITY)
                    .margin(egui::Margin::symmetric(10, 8)),
            );
            if input.changed() {
                if let Some(stem) = Path::new(self.input.trim()).file_stem() {
                    if self.name.is_empty() {
                        self.name = stem.to_string_lossy().to_string();
                    }
                }
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Called").size(12.0).color(p.muted));
                ui.add(
                    egui::TextEdit::singleline(&mut self.name)
                        .hint_text("project name (optional)")
                        .desired_width(240.0),
                );
                if theme::quiet(ui, p, "Clear").clicked() {
                    self.name.clear();
                }
            });
            ui.add_space(14.0);
            theme::kicker(ui, p, "Niche");
            ui.horizontal_wrapped(|ui| {
                for niche in Niche::ALL {
                    let selected = self.niche == niche;
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(niche.label()).size(11.5).color(if selected {
                                    p.accent_ink
                                } else {
                                    p.muted
                                }),
                            )
                            .fill(if selected { p.accent } else { p.card_alt })
                            .min_size(egui::vec2(0.0, 26.0)),
                        )
                        .clicked()
                    {
                        self.niche = niche;
                    }
                }
            });
            ui.add_space(12.0);
            theme::kicker(ui, p, "Feed");
            ui.horizontal_wrapped(|ui| {
                for platform in Platform::ALL {
                    let selected = self.platform == platform;
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(platform.label())
                                    .size(11.5)
                                    .color(if selected { p.accent_ink } else { p.muted }),
                            )
                            .fill(if selected { p.accent } else { p.card_alt })
                            .min_size(egui::vec2(0.0, 26.0)),
                        )
                        .clicked()
                    {
                        self.platform = platform;
                    }
                }
            });
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Clips").size(12.0).color(p.muted));
                ui.add(egui::Slider::new(&mut self.target_clips, 1..=12).show_value(true));
                ui.add_space(16.0);
                ui.label(RichText::new("Max length").size(12.0).color(p.muted));
                ui.add(
                    egui::Slider::new(&mut self.max_seconds, 10.0..=60.0)
                        .suffix("s")
                        .show_value(true),
                );
            });
            ui.label(
                RichText::new(
                    "60 seconds is a wall, not a suggestion. Every export is checked against it.",
                )
                .size(10.5)
                .color(p.faint),
            );
            ui.add_space(12.0);
            let llm_available = self.config.llm_allowed();
            ui.horizontal(|ui| {
                ui.checkbox(
                    &mut self.config.llm_enabled,
                    RichText::new("Let a local model polish titles").size(12.0),
                );
                theme::chip(
                    ui,
                    if llm_available {
                        p.card_alt
                    } else {
                        p.card_alt
                    },
                    if llm_available { p.mint } else { p.faint },
                    if llm_available {
                        "ollama · loopback"
                    } else {
                        "heuristics only"
                    },
                );
            });
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                let busy = self.job.is_some();
                if ui
                    .add_enabled(
                        !busy && !self.input.trim().is_empty(),
                        egui::Button::new(
                            RichText::new(if busy {
                                "Harvesting…"
                            } else {
                                "Start the harvest"
                            })
                            .strong()
                            .color(p.accent_ink),
                        )
                        .fill(p.accent)
                        .min_size(egui::vec2(190.0, 38.0)),
                    )
                    .clicked()
                {
                    let _ = self.config.save();
                    self.start_harvest();
                }
                if let Some(job) = &self.job {
                    theme::progress_bar(ui, p, job.fraction, 220.0);
                    ui.label(RichText::new(&job.message).size(11.0).color(p.muted));
                } else {
                    ui.label(
                        RichText::new(
                            "FFmpeg does the lifting. whisper.cpp adds words. You keep the taste.",
                        )
                        .size(11.0)
                        .color(p.faint),
                    );
                }
            });
        });

        if !self.warnings.is_empty() {
            ui.add_space(8.0);
            for warning in &self.warnings {
                ui.label(
                    RichText::new(format!("! {warning}"))
                        .size(11.0)
                        .color(p.amber),
                );
            }
        }

        ui.add_space(24.0);
        ui.horizontal(|ui| {
            theme::kicker(ui, p, "Recent harvests");
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                if theme::quiet(ui, p, "Refresh").clicked() {
                    self.refresh_projects();
                }
            });
        });
        if let Some(error) = &self.projects_error {
            ui.label(RichText::new(error).size(12.0).color(p.amber));
        }
        if self.projects.is_empty() {
            ui.add_space(6.0);
            theme::flat_frame(p).show(ui, |ui| {
                ui.set_width(ui.available_width().min(880.0));
                ui.label(
                    RichText::new("Nothing here but pixels and potential.")
                        .size(15.0)
                        .strong()
                        .color(p.ink),
                );
                ui.label(
                    RichText::new(
                        "Your first harvest will show up here, with its clips and its excuses.",
                    )
                    .size(12.0)
                    .color(p.muted),
                );
            });
            return;
        }
        for summary in self.projects.clone() {
            let response = egui::Frame::default()
                .fill(p.card)
                .inner_margin(Margin::symmetric(14, 10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new(&summary.name)
                                    .size(13.5)
                                    .strong()
                                    .color(p.ink),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "{} clips · {} · {} · {}",
                                    summary.clips,
                                    human_duration(summary.duration),
                                    summary.niche.label(),
                                    summary.platform.label()
                                ))
                                .size(11.0)
                                .color(p.faint),
                            );
                        });
                        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                            if theme::quiet(ui, p, "Open").clicked() {
                                self.load(summary.id);
                            }
                            theme::chip(ui, p.card_alt, p.muted, &summary.id.to_string()[..8]);
                        });
                    });
                })
                .response;
            if response.interact(Sense::click()).clicked() {
                self.load(summary.id);
            }
            ui.add_space(2.0);
        }
    }

    fn clip_card(
        &self,
        ui: &mut Ui,
        p: &Palette,
        clip: &Clip,
        index: usize,
        selected: bool,
    ) -> bool {
        let response = egui::Frame::default()
            .fill(if selected { p.card_alt } else { p.card })
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(236.0);
                let (rect, _) = ui.allocate_exact_size(egui::vec2(236.0, 132.0), Sense::hover());
                if let Some(texture) = self.thumbnails.get(&clip.id) {
                    ui.painter().image(
                        texture.id(),
                        rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                } else {
                    ui.painter().rect_filled(rect, 6.0, p.card_alt);
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "still loading",
                        egui::FontId::proportional(11.0),
                        p.faint,
                    );
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    theme::chip(ui, p.accent, p.accent_ink, &format!("{:.0}", clip.score));
                    ui.label(
                        RichText::new(format!("{:.1}s", clip.duration()))
                            .size(11.0)
                            .strong()
                            .color(p.ink),
                    );
                    ui.label(
                        RichText::new(format!("#{}", index + 1))
                            .size(10.5)
                            .color(p.faint),
                    );
                });
                ui.label(RichText::new(&clip.title).size(12.5).strong().color(p.ink));
                ui.label(RichText::new(&clip.hook).size(11.0).color(p.muted));
            })
            .response;
        response.interact(Sense::click()).clicked()
    }

    fn screen_clips(&mut self, ui: &mut Ui, p: &Palette) {
        if self.project.is_none() {
            theme::flat_frame(p).show(ui, |ui| {
                ui.set_width(ui.available_width().min(720.0));
                ui.label(
                    RichText::new("The barn is empty.")
                        .size(22.0)
                        .strong()
                        .color(p.ink),
                );
                ui.label(
                    RichText::new(
                        "Start a harvest and this room fills with short, loud little files.",
                    )
                    .size(13.0)
                    .color(p.muted),
                );
                ui.add_space(10.0);
                if theme::primary(ui, p, "Start a harvest").clicked() {
                    self.screen = Screen::Farm;
                }
            });
            return;
        }

        let mut do_export: Option<ExportKind> = None;
        let mut do_preview = false;
        let mut do_reanalyze = false;
        let mut do_refine = false;
        let mut dirty = false;

        let (name, niche, platform, clip_count) = {
            let project = self.project.as_ref().unwrap();
            (
                project.name.clone(),
                project.niche,
                project.platform,
                project.clips.len(),
            )
        };
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(&name).size(24.0).strong().color(p.ink));
                ui.label(
                    RichText::new(format!(
                        "{clip_count} clips · {} · {} · every clip capped at 60s",
                        niche.label(),
                        platform.label()
                    ))
                    .size(11.5)
                    .color(p.faint),
                );
            });
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                if theme::secondary(ui, p, "Re-harvest").clicked() {
                    do_reanalyze = true;
                }
            });
        });
        for warning in &self.warnings {
            ui.label(
                RichText::new(format!("! {warning}"))
                    .size(10.5)
                    .color(p.amber),
            );
        }
        ui.add_space(10.0);

        if self
            .project
            .as_ref()
            .map(|project| project.clips.is_empty())
            .unwrap_or(true)
        {
            theme::flat_frame(p).show(ui, |ui| {
                ui.label(
                    RichText::new("No clips yet. The machines are stretching.")
                        .size(15.0)
                        .color(p.ink),
                );
            });
            return;
        }

        ui.horizontal_top(|ui| {
            // Clip grid.
            let grid_width = (ui.available_width() - 352.0).max(480.0);
            ScrollArea::vertical()
                .id_salt("clip-grid")
                .max_height(ui.available_height() - 8.0)
                .show(ui, |ui| {
                    ui.set_width(grid_width);
                    ui.horizontal_wrapped(|ui| {
                        let clips = self.project.as_ref().unwrap().clips.clone();
                        for (index, clip) in clips.iter().enumerate() {
                            if self.clip_card(ui, p, clip, index, index == self.selected) {
                                self.selected = index;
                                self.refine_log.clear();
                            }
                        }
                    });
                });

            ui.add_space(10.0);
            // Inspector.
            let inspector_width = 330.0;
            ScrollArea::vertical()
                .id_salt("inspector")
                .max_height(ui.available_height() - 8.0)
                .show(ui, |ui| {
                    ui.set_width(inspector_width);
                    let Some(clip) = self
                        .project
                        .as_ref()
                        .and_then(|project| project.clips.get(self.selected))
                        .cloned()
                    else {
                        return;
                    };
                    egui::Frame::default()
                        .fill(p.card)
                        .inner_margin(Margin::same(16))
                        .show(ui, |ui| {
                            ui.set_width(inspector_width - 32.0);
                            ui.horizontal(|ui| {
                                theme::chip(
                                    ui,
                                    p.accent,
                                    p.accent_ink,
                                    &format!("{:.0} hook", clip.score),
                                );
                                theme::chip(
                                    ui,
                                    p.card_alt,
                                    p.muted,
                                    &format!("{:.1}s", clip.duration()),
                                );
                                if !clip.exports.is_empty() {
                                    theme::chip(ui, p.card_alt, p.mint, "exported");
                                }
                            });
                            ui.add_space(8.0);
                            theme::kicker(ui, p, "Title");
                            if let Some(project) = self.project.as_mut() {
                                if let Some(clip) = project.clips.get_mut(self.selected) {
                                    if ui
                                        .add(
                                            egui::TextEdit::multiline(&mut clip.title)
                                                .desired_rows(2)
                                                .desired_width(f32::INFINITY),
                                        )
                                        .changed()
                                    {
                                        dirty = true;
                                    }
                                }
                            }
                            ui.add_space(8.0);
                            theme::kicker(ui, p, "Caption on the post");
                            if let Some(project) = self.project.as_mut() {
                                if let Some(clip) = project.clips.get_mut(self.selected) {
                                    if ui
                                        .add(
                                            egui::TextEdit::multiline(&mut clip.caption)
                                                .desired_rows(3)
                                                .desired_width(f32::INFINITY),
                                        )
                                        .changed()
                                    {
                                        dirty = true;
                                    }
                                    ui.label(
                                        RichText::new(
                                            clip.hashtags
                                                .iter()
                                                .map(|tag| format!("#{tag}"))
                                                .collect::<Vec<_>>()
                                                .join(" "),
                                        )
                                        .size(11.0)
                                        .color(p.muted),
                                    );
                                }
                            }
                            ui.add_space(10.0);
                            ui.horizontal_wrapped(|ui| {
                                if let Some(project) = self.project.as_mut() {
                                    if let Some(clip) = project.clips.get_mut(self.selected) {
                                        if ui
                                            .checkbox(
                                                &mut clip.settings.captions_enabled,
                                                RichText::new("captions").size(12.0),
                                            )
                                            .changed()
                                        {
                                            dirty = true;
                                        }
                                        if ui
                                            .checkbox(
                                                &mut clip.settings.vertical,
                                                RichText::new("9:16").size(12.0),
                                            )
                                            .changed()
                                        {
                                            dirty = true;
                                        }
                                    }
                                }
                            });
                            ui.add_space(10.0);
                            theme::kicker(ui, p, "Refine like a human");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.refine_text)
                                    .hint_text("“30 seconds, tighter, captions off”")
                                    .desired_width(f32::INFINITY),
                            );
                            ui.horizontal_wrapped(|ui| {
                                for suggestion in
                                    ["30 seconds", "tighter", "start 2s later", "captions off"]
                                {
                                    if theme::quiet(ui, p, suggestion).clicked() {
                                        self.refine_text = suggestion.to_string();
                                    }
                                }
                            });
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("Apply edit").strong().color(p.accent_ink),
                                    )
                                    .fill(p.accent)
                                    .min_size(egui::vec2(ui.available_width(), 30.0)),
                                )
                                .clicked()
                            {
                                do_refine = true;
                            }
                            for change in &self.refine_log {
                                ui.label(
                                    RichText::new(format!("· {change}"))
                                        .size(11.0)
                                        .color(p.mint),
                                );
                            }
                            ui.add_space(12.0);
                            ui.separator();
                            ui.add_space(8.0);
                            theme::kicker(ui, p, "Export");
                            ui.label(
                                RichText::new(
                                    "Raw keeps your options. Rendered keeps your dignity.",
                                )
                                .size(11.0)
                                .color(p.faint),
                            );
                            ui.add_space(6.0);
                            if theme::secondary(ui, p, "Preview this cut").clicked() {
                                do_preview = true;
                            }
                            ui.horizontal(|ui| {
                                if theme::secondary(ui, p, "Raw (trim only)").clicked() {
                                    do_export = Some(ExportKind::Raw);
                                }
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new("Render 9:16")
                                                .strong()
                                                .color(p.accent_ink),
                                        )
                                        .fill(p.accent)
                                        .min_size(egui::vec2(0.0, 32.0)),
                                    )
                                    .clicked()
                                {
                                    do_export = Some(ExportKind::Rendered);
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Quality").size(10.5).color(p.faint));
                                for quality in [Quality::High, Quality::Balanced, Quality::Small] {
                                    if ui
                                        .selectable_label(
                                            self.quality == quality,
                                            RichText::new(quality.label()).size(10.5),
                                        )
                                        .clicked()
                                    {
                                        self.quality = quality;
                                    }
                                }
                            });
                            ui.add_space(8.0);
                            if clip.exports.is_empty() {
                                ui.label(
                                    RichText::new(
                                        "No files yet. The good part is still theoretical.",
                                    )
                                    .size(11.0)
                                    .color(p.faint),
                                );
                            }
                            for record in clip.exports.iter().rev().take(3) {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "{} · {}",
                                            match record.kind {
                                                ExportKind::Raw => "raw",
                                                ExportKind::Rendered => "short",
                                            },
                                            human_bytes(record.bytes)
                                        ))
                                        .size(10.5)
                                        .color(p.muted),
                                    );
                                    if theme::quiet(ui, p, "Open").clicked() {
                                        open_path(&record.path);
                                    }
                                });
                            }
                        });
                });
        });

        if dirty {
            self.save();
        }
        if do_refine {
            self.apply_refine();
        }
        if do_preview {
            self.start_preview();
        }
        if let Some(kind) = do_export {
            self.start_export(kind);
        }
        if do_reanalyze {
            self.start_reanalyze();
        }
    }

    fn screen_trends(&mut self, ui: &mut Ui, p: &Palette) {
        let (niche, platform) = self
            .project
            .as_ref()
            .map(|project| (project.niche, project.platform))
            .unwrap_or((self.niche, self.platform));
        ui.label(
            RichText::new("Pacing is a language.")
                .size(26.0)
                .strong()
                .color(p.ink),
        );
        ui.label(
            RichText::new("Offline profiles for each niche and feed. They are opinionated on purpose; the app never pretends to know today's meme.")
                .size(12.5)
                .color(p.muted),
        );
        ui.add_space(16.0);
        let current = trends::resolve(niche, platform);
        theme::card_frame(p).show(ui, |ui| {
            ui.set_width(ui.available_width().min(880.0));
            ui.horizontal(|ui| {
                theme::chip(ui, p.accent, p.accent_ink, niche.label());
                theme::chip(ui, p.card_alt, p.muted, platform.label());
                theme::chip(
                    ui,
                    p.card_alt,
                    p.mint,
                    &format!("cut every {:.1}s", current.cut_every),
                );
            });
            ui.add_space(8.0);
            ui.label(
                RichText::new(format!(
                    "{:.0}–{:.0}s clips · {} captions",
                    current.min_seconds,
                    current.max_seconds,
                    current.caption_style.label().to_lowercase()
                ))
                .size(14.0)
                .strong()
                .color(p.ink),
            );
            ui.label(RichText::new(&current.notes).size(12.5).color(p.muted));
            ui.add_space(8.0);
            ui.label(
                RichText::new(
                    current
                        .hashtags
                        .iter()
                        .map(|tag| format!("#{tag}"))
                        .collect::<Vec<_>>()
                        .join("  "),
                )
                .size(11.5)
                .color(p.amber),
            );
            ui.add_space(10.0);
            if self.project.is_some()
                && theme::secondary(ui, p, "Apply this caption style to every clip").clicked()
            {
                if let Some(project) = self.project.as_mut() {
                    for clip in &mut project.clips {
                        clip.settings.caption_style = current.caption_style;
                    }
                    if let Err(error) = project::save_project(project) {
                        self.toast(format!("Could not save: {error}"), false);
                    } else {
                        self.toast("Caption style updated across the harvest.", true);
                    }
                }
            }
        });
        ui.add_space(18.0);
        theme::kicker(ui, p, "Same niche, other feeds");
        for other in Platform::ALL {
            if other == platform {
                continue;
            }
            let profile = trends::resolve(niche, other);
            theme::flat_frame(p).show(ui, |ui| {
                ui.set_width(ui.available_width().min(880.0));
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(profile.platform.label())
                            .size(12.5)
                            .strong()
                            .color(p.ink),
                    );
                    ui.label(
                        RichText::new(format!(
                            "{:.0}–{:.0}s · {}",
                            profile.min_seconds, profile.max_seconds, profile.notes
                        ))
                        .size(11.0)
                        .color(p.muted),
                    );
                });
            });
        }
        ui.add_space(10.0);
        ui.label(
            RichText::new(format!(
                "Drop your own profiles at {} to override the bundled ones.",
                project::data_dir().join("trends.json").display()
            ))
            .size(10.5)
            .color(p.faint),
        );
    }

    fn screen_tuning(&mut self, ui: &mut Ui, p: &Palette) {
        ui.label(
            RichText::new("The engine room.")
                .size(26.0)
                .strong()
                .color(p.ink),
        );
        ui.label(
            RichText::new("No API keys. No credits. No “free trial” countdown. Just binaries and a loopback cable.")
                .size(12.5)
                .color(p.muted),
        );
        ui.add_space(14.0);

        if let Some(error) = &self.tools_error {
            ui.label(
                RichText::new(format!("! {error}"))
                    .size(12.0)
                    .color(p.amber),
            );
            ui.add_space(6.0);
        }
        for status in &self.statuses {
            let mark = if status.path.is_some() {
                p.mint
            } else {
                p.amber
            };
            theme::flat_frame(p).show(ui, |ui| {
                ui.set_width(ui.available_width().min(880.0));
                ui.horizontal(|ui| {
                    ui.label(RichText::new("●").size(10.0).color(mark));
                    ui.label(RichText::new(status.name).size(12.5).strong().color(p.ink));
                    ui.label(
                        RichText::new(status.version.clone().unwrap_or_else(|| {
                            status.path.clone().unwrap_or_else(|| "missing".into())
                        }))
                        .size(11.0)
                        .color(p.muted),
                    );
                });
                if status.path.is_none() {
                    ui.label(RichText::new(status.hint).size(10.5).color(p.faint));
                }
            });
        }

        ui.add_space(16.0);
        theme::card_frame(p).show(ui, |ui| {
            ui.set_width(ui.available_width().min(880.0));
            theme::kicker(ui, p, "Binary paths (blank = look on PATH)");
            let field = |ui: &mut Ui, label: &str, value: &mut Option<PathBuf>| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).size(12.0).color(p.muted));
                    let mut text = value
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_default();
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut text)
                                .desired_width(360.0)
                                .hint_text("auto"),
                        )
                        .changed()
                    {
                        *value = if text.trim().is_empty() {
                            None
                        } else {
                            Some(PathBuf::from(text.trim()))
                        };
                    }
                });
            };
            field(ui, "FFmpeg", &mut self.config.ffmpeg);
            field(ui, "ffprobe", &mut self.config.ffprobe);
            field(ui, "yt-dlp", &mut self.config.yt_dlp);
            field(ui, "whisper.cpp", &mut self.config.whisper_cli);
            field(ui, "whisper model", &mut self.config.whisper_model);
            ui.add_space(10.0);
            theme::kicker(ui, p, "Local model (optional)");
            ui.checkbox(
                &mut self.config.llm_enabled,
                RichText::new("Use Ollama for titles and fuzzy refine requests").size(12.0),
            );
            ui.horizontal(|ui| {
                ui.label(RichText::new("Endpoint").size(12.0).color(p.muted));
                ui.add(
                    egui::TextEdit::singleline(&mut self.config.llm_endpoint)
                        .desired_width(260.0),
                );
                ui.label(RichText::new("Model").size(12.0).color(p.muted));
                ui.add(
                    egui::TextEdit::singleline(&mut self.config.llm_model)
                        .desired_width(160.0),
                );
            });
            ui.label(
                RichText::new("Only loopback addresses are accepted; clipfarm refuses anything else at call time.")
                    .size(10.5)
                    .color(p.faint),
            );
            ui.add_space(10.0);
            if theme::primary(ui, p, "Save tuning").clicked() {
                match self.config.save() {
                    Ok(()) => {
                        match Tools::discover(&self.config) {
                            Ok(tools) => {
                                self.statuses = tools.status();
                                self.tools_error = None;
                            }
                            Err(error) => self.tools_error = Some(format!("{error}")),
                        }
                        self.toast("Saved. The machines have been informed.", true);
                    }
                    Err(error) => self.toast(format!("Could not save: {error}"), false),
                }
            }
        });

        ui.add_space(14.0);
        theme::kicker(ui, p, "Where things live");
        ui.label(
            RichText::new(project::data_dir().display().to_string())
                .size(11.0)
                .monospace()
                .color(p.muted),
        );
        ui.add_space(12.0);
        theme::kicker(ui, p, "Shortcuts");
        for (keys, action) in [
            ("⌘/Ctrl + 1…4", "switch rooms"),
            ("⌘/Ctrl + E", "render the selected clip"),
            ("⌘/Ctrl + N", "start a new harvest"),
            ("Esc", "dismiss the toast"),
        ] {
            ui.horizontal(|ui| {
                theme::chip(ui, p.card_alt, p.ink, keys);
                ui.label(RichText::new(action).size(11.0).color(p.muted));
            });
        }
    }

    fn bottom_bar(&mut self, ui: &mut Ui, p: &Palette) {
        ui.horizontal(|ui| {
            let dot = if self.tools_error.is_none() {
                p.mint
            } else {
                p.amber
            };
            ui.label(RichText::new("●").size(9.0).color(dot));
            ui.label(
                RichText::new(if self.job.is_some() {
                    "working locally"
                } else {
                    "idle · nothing uploading"
                })
                .size(10.5)
                .color(p.faint),
            );
            if let Some(job) = &self.job {
                ui.add_space(10.0);
                ui.label(
                    RichText::new(format!("{}: {}", job.label, job.message))
                        .size(10.5)
                        .color(p.muted),
                );
                theme::progress_bar(ui, p, job.fraction, 140.0);
                ui.label(
                    RichText::new(format!("{}s", job.started.elapsed().as_secs()))
                        .size(10.0)
                        .monospace()
                        .color(p.faint),
                );
            }
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(toast) = &self.toast {
                    let color = if toast.good { p.mint } else { p.amber };
                    ui.label(RichText::new(&toast.text).size(11.0).color(color));
                } else {
                    ui.label(
                        RichText::new("clipfarm 0.2 · built in Rust")
                            .size(10.0)
                            .color(p.faint),
                    );
                }
            });
        });
    }
}

impl eframe::App for ClipfarmApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.poll_jobs(ctx);
        self.ensure_thumbnails(ctx);
        let p = theme::palette(self.dark);
        theme::apply(ctx, &p);

        let dropped: Vec<PathBuf> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect()
        });
        if let Some(path) = dropped.first() {
            self.pick_input(path);
            self.screen = Screen::Farm;
        }

        let wants_keyboard = ctx.wants_keyboard_input();
        if !wants_keyboard {
            let (new_harvest, export, nav) = ctx.input(|input| {
                (
                    input.modifiers.command && input.key_pressed(egui::Key::N),
                    input.modifiers.command && input.key_pressed(egui::Key::E),
                    [
                        input.modifiers.command && input.key_pressed(egui::Key::Num1),
                        input.modifiers.command && input.key_pressed(egui::Key::Num2),
                        input.modifiers.command && input.key_pressed(egui::Key::Num3),
                        input.modifiers.command && input.key_pressed(egui::Key::Num4),
                    ],
                )
            });
            if new_harvest {
                self.screen = Screen::Farm;
                self.project = None;
            }
            if nav[0] {
                self.screen = Screen::Farm;
            }
            if nav[1] {
                self.screen = Screen::Clips;
            }
            if nav[2] {
                self.screen = Screen::Trends;
            }
            if nav[3] {
                self.screen = Screen::Tuning;
            }
            if export && self.project.is_some() {
                self.start_export(ExportKind::Rendered);
            }
            if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
                self.toast = None;
            }
        }

        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::default()
                    .fill(p.panel)
                    .inner_margin(Margin::symmetric(16, 6)),
            )
            .show(ctx, |ui| self.bottom_bar(ui, &p));

        egui::SidePanel::left("rail")
            .resizable(false)
            .exact_width(238.0)
            .frame(
                egui::Frame::default()
                    .fill(p.panel)
                    .inner_margin(Margin::symmetric(16, 18)),
            )
            .show(ctx, |ui| self.sidebar(ui, &p));

        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(p.bg)
                    .inner_margin(Margin::symmetric(22, 8)),
            )
            .show(ctx, |ui| {
                ui.add_space(8.0);
                self.topbar(ui, &p);
                match self.screen {
                    Screen::Clips => self.screen_clips(ui, &p),
                    Screen::Farm => {
                        ScrollArea::vertical()
                            .id_salt("farm-scroll")
                            .auto_shrink([false; 2])
                            .show(ui, |ui| {
                                self.screen_farm(ui, &p);
                                ui.add_space(30.0);
                            });
                    }
                    Screen::Trends => {
                        ScrollArea::vertical()
                            .id_salt("trends-scroll")
                            .auto_shrink([false; 2])
                            .show(ui, |ui| {
                                self.screen_trends(ui, &p);
                                ui.add_space(30.0);
                            });
                    }
                    Screen::Tuning => {
                        ScrollArea::vertical()
                            .id_salt("tuning-scroll")
                            .auto_shrink([false; 2])
                            .show(ui, |ui| {
                                self.screen_tuning(ui, &p);
                                ui.add_space(30.0);
                            });
                    }
                }
            });
    }
}

fn open_path(path: &Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}
