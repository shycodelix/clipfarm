use anyhow::Result;
use chrono::Local;
use clap::{Parser, Subcommand, ValueEnum};
use clipfarm_core::analyze::AnalyzeOptions;
use clipfarm_core::export::ExportOptions;
use clipfarm_core::model::{ExportKind, Niche, Platform};
use clipfarm_core::pipeline;
use clipfarm_core::project::{self, data_dir};
use clipfarm_core::refine;
use clipfarm_core::tools::Tools;
use clipfarm_core::util::{human_bytes, human_duration};
use clipfarm_core::{AppConfig, Project, MAX_CLIP_SECONDS};

#[derive(Parser)]
#[command(
    name = "clipfarm",
    version,
    about = "Long video in. Shorts out. No cloud, no credits.",
    long_about = "clipfarm turns a VOD or local recording into review-ready vertical clips. \
Everything runs on your machine: FFmpeg, whisper.cpp and an optional local LLM."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check which local tools clipfarm can use.
    Doctor,
    /// Ingest a file or URL, analyze it, and save a project.
    Ingest {
        /// Local file path or http(s) URL.
        input: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long, default_value = "generic")]
        niche: String,
        #[arg(long, default_value = "tiktok")]
        platform: String,
        #[arg(long, default_value_t = 6)]
        clips: usize,
        #[arg(long, default_value_t = MAX_CLIP_SECONDS)]
        max_seconds: f64,
        /// Skip the analysis pass and just register the media.
        #[arg(long)]
        no_analyze: bool,
    },
    /// Re-run analysis for an existing project.
    Analyze {
        project: String,
        #[arg(long, default_value_t = 6)]
        clips: usize,
        #[arg(long, default_value_t = MAX_CLIP_SECONDS)]
        max_seconds: f64,
        /// Keep clips you already exported (they are skipped when replacing).
        #[arg(long)]
        keep_exports: bool,
    },
    /// List projects.
    Projects,
    /// List the clips in a project.
    Clips { project: String },
    /// Adjust a clip with a plain-English instruction.
    Refine {
        project: String,
        clip: String,
        /// For example: "make it 30 seconds, tighter, captions off".
        instruction: Vec<String>,
        /// Also ask the local LLM when the deterministic parser finds nothing.
        #[arg(long)]
        smart: bool,
    },
    /// Export one clip as a raw stream copy or a rendered vertical short.
    Export {
        project: String,
        clip: String,
        /// Bypass edits: trim only, stream copy, no captions or reframing.
        #[arg(long)]
        raw: bool,
        /// Rendered 9:16 short with captions (default).
        #[arg(long)]
        rendered: bool,
        #[arg(long, value_enum, default_value_t = QualityArg::Balanced)]
        quality: QualityArg,
        #[arg(long)]
        out: Option<String>,
    },
    /// Show the bundled trend profiles.
    Trends {
        #[arg(long)]
        niche: Option<String>,
    },
    /// Open the desktop app on a project (prints the path in headless builds).
    Open { project: Option<String> },
    /// Where clipfarm keeps projects, exports and config.
    Paths,
}

#[derive(Copy, Clone, ValueEnum)]
enum QualityArg {
    High,
    Balanced,
    Small,
}

impl From<QualityArg> for clipfarm_core::export::Quality {
    fn from(value: QualityArg) -> Self {
        match value {
            QualityArg::High => Self::High,
            QualityArg::Balanced => Self::Balanced,
            QualityArg::Small => Self::Small,
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("\n  ✗ {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let config = AppConfig::load();
    match cli.command {
        Command::Doctor => doctor(&config),
        Command::Paths => {
            println!("data      {}", data_dir().display());
            println!("projects  {}", project::projects_dir().display());
            println!("config    {}", AppConfig::path().display());
            Ok(())
        }
        Command::Projects => list_projects(),
        Command::Trends { niche } => trends(niche),
        Command::Ingest {
            input,
            name,
            niche,
            platform,
            clips,
            max_seconds,
            no_analyze,
        } => {
            let tools = Tools::discover(&config)?;
            let niche = Niche::parse(&niche)?;
            let platform = Platform::parse(&platform)?;
            banner();
            let mut project = pipeline::create_project(
                &tools,
                &input,
                name,
                niche,
                platform,
                &mut |fraction, message| progress("ingest", fraction, message),
            )?;
            if no_analyze {
                println!(
                    "\n  saved project {} ({})",
                    short_id(project.id),
                    project.name
                );
                return Ok(());
            }
            analyze_project(&tools, &mut project, clips, max_seconds, false)?;
            print_clips(&project);
            println!(
                "\n  Next: clipfarm export {} 1 --raw\n",
                short_id(project.id)
            );
            Ok(())
        }
        Command::Analyze {
            project: selector,
            clips,
            max_seconds,
            keep_exports,
        } => {
            let tools = Tools::discover(&config)?;
            let mut project = project::load_project(&selector)?;
            analyze_project(&tools, &mut project, clips, max_seconds, keep_exports)?;
            print_clips(&project);
            Ok(())
        }
        Command::Clips { project } => {
            let project = project::load_project(&project)?;
            print_clips(&project);
            Ok(())
        }
        Command::Refine {
            project,
            clip,
            instruction,
            smart,
        } => {
            let mut project = project::load_project(&project)?;
            let index = pipeline::find_clip_index(&project, &clip)?;
            let text = instruction.join(" ");
            let llm = if smart && config.llm_allowed() {
                Some((config.llm_endpoint.as_str(), config.llm_model.as_str()))
            } else {
                None
            };
            let snapshot = project.clone();
            let changes = {
                let clip = &mut project.clips[index];
                refine::refine(&snapshot, clip, &text, llm)
            }
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "I could not map that to an edit. Try: \"make it 30 seconds\", \"start 2 seconds later\", \"captions off\", \"title: ...\"."
                )
            })?;
            project::save_project(&mut project)?;
            println!("\n  clip {} · {}", index + 1, project.clips[index].title);
            for change in changes {
                println!("    · {change}");
            }
            println!(
                "    now {:.1}s ({:.1}s → {:.1}s)\n",
                project.clips[index].duration(),
                project.clips[index].start,
                project.clips[index].end
            );
            Ok(())
        }
        Command::Export {
            project,
            clip,
            raw,
            rendered: _,
            quality,
            out,
        } => {
            let tools = Tools::discover(&config)?;
            let mut project = project::load_project(&project)?;
            let index = pipeline::find_clip_index(&project, &clip)?;
            let kind = if raw {
                ExportKind::Raw
            } else {
                ExportKind::Rendered
            };
            let output = out.map(std::path::PathBuf::from).unwrap_or_else(|| {
                pipeline::export_path(&project, &project.clips[index], index, kind)
            });
            let options = ExportOptions {
                kind,
                output: output.clone(),
                quality: quality.into(),
            };
            let clip_snapshot = project.clips[index].clone();
            let outcome = clipfarm_core::export_clip(
                &tools,
                &project,
                &clip_snapshot,
                &options,
                &mut |fraction, message| progress("export", fraction, message),
            )?;
            if let Some(srt) = clipfarm_core::sidecar_subtitles(&clip_snapshot, &output)? {
                println!("  subtitles {}", srt.display());
            }
            clipfarm_core::export::record_export(&mut project.clips[index], &outcome);
            project::save_project(&mut project)?;
            let label = match outcome.kind {
                ExportKind::Raw => "raw",
                ExportKind::Rendered => "short",
            };
            println!(
                "\n  ✓ {label} · {} · {} · {}\n    {}\n",
                outcome.preset,
                human_bytes(outcome.bytes),
                outcome.encoder,
                outcome.path.display()
            );
            println!("    {}\n", outcome.note);
            Ok(())
        }
        Command::Open { project } => {
            let hint = project
                .as_deref()
                .map(|selector| format!(" --project {selector}"))
                .unwrap_or_default();
            println!(
                "Run the desktop app with:\n\n  cargo run -p clipfarm-app --release --{hint}\n"
            );
            println!("(The CLI is the engine; the app is the review room.)");
            Ok(())
        }
    }
}

fn analyze_project(
    tools: &Tools,
    project: &mut Project,
    clips: usize,
    max_seconds: f64,
    keep_exports: bool,
) -> Result<()> {
    let exported = if keep_exports {
        project
            .clips
            .iter()
            .filter(|clip| !clip.exports.is_empty())
            .cloned()
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    println!("\n  project   {} ({})", project.name, short_id(project.id));
    let options = AnalyzeOptions {
        target_clips: clips.clamp(1, 40),
        max_seconds: max_seconds.clamp(4.0, MAX_CLIP_SECONDS),
        min_seconds: 10.0,
    };
    let report = pipeline::analyze(tools, project, &options, &mut |fraction, message| {
        progress("analyze", fraction, message)
    })?;
    for warning in &report.warnings {
        println!("  ! {warning}");
    }
    for clip in exported {
        project.clips.push(clip);
    }
    project::save_project(project)?;
    println!(
        "\n  {} clips from {} speech segments{}",
        report.clips,
        report.speech_segments,
        if report.transcribed {
            ", word-level captions on"
        } else {
            ", VAD-only"
        }
    );
    Ok(())
}

fn doctor(config: &AppConfig) -> Result<()> {
    banner();
    println!("\n  Tool check\n");
    match Tools::discover(config) {
        Ok(tools) => {
            for status in tools.status() {
                let mark = if status.path.is_some() {
                    "●"
                } else if status.required {
                    "✗"
                } else {
                    "○"
                };
                println!(
                    "  {mark} {:<15} {:<42} {}",
                    status.name,
                    status
                        .version
                        .clone()
                        .or_else(|| status.path.clone())
                        .unwrap_or_else(|| "not found".into()),
                    if status.path.is_some() {
                        ""
                    } else {
                        &status.hint
                    }
                );
            }
            if tools.supports_subtitles() {
                println!("\n  ● libass present — captions can be burned in.");
            } else {
                println!(
                    "\n  ○ This FFmpeg has no libass; rendered clips ship with an SRT sidecar instead."
                );
            }
        }
        Err(error) => {
            println!("  ✗ {error}");
            println!("\n  Install FFmpeg, then re-run `clipfarm doctor`.");
        }
    }
    println!(
        "\n  Local AI: {}",
        if config.llm_allowed() {
            "enabled (loopback only)"
        } else {
            "off — heuristics are in charge"
        }
    );
    println!("  Data:     {}\n", data_dir().display());
    Ok(())
}

fn list_projects() -> Result<()> {
    let projects = project::list_projects()?;
    if projects.is_empty() {
        println!(
            "\n  No harvests yet. Try:\n\n    clipfarm ingest ~/Videos/interview.mp4 --niche podcast\n"
        );
        return Ok(());
    }
    println!(
        "\n  {:<10} {:<28} {:>6} {:>8} {:<8}",
        "ID", "NAME", "CLIPS", "LENGTH", "UPDATED"
    );
    for summary in projects {
        println!(
            "  {:<10} {:<28} {:>6} {:>8}  {}",
            short_id(summary.id),
            truncate(&summary.name, 28),
            summary.clips,
            human_duration(summary.duration),
            summary
                .updated_at
                .with_timezone(&Local)
                .format("%b %d %H:%M")
        );
    }
    println!();
    Ok(())
}

fn print_clips(project: &Project) {
    println!(
        "\n  {} · {} · {}",
        project.name,
        project.niche.label(),
        project.platform.label()
    );
    if project.clips.is_empty() {
        println!(
            "\n  No clips yet. Run `clipfarm analyze {}`.\n",
            short_id(project.id)
        );
        return;
    }
    println!(
        "\n  {:<4} {:<44} {:>6} {:>6} {:<8}",
        "#", "TITLE", "LENGTH", "SCORE", "HOOK"
    );
    for (index, clip) in project.clips.iter().enumerate() {
        println!(
            "  {:<4} {:<44} {:>6} {:>5.0}  {}",
            index + 1,
            truncate(&clip.title, 44),
            format!("{:.1}s", clip.duration()),
            clip.score,
            truncate(&clip.hook, 38)
        );
    }
    println!(
        "\n  {} clips · {:.1}s total · ceiling {}s\n",
        project.clips.len(),
        project.total_clip_seconds(),
        MAX_CLIP_SECONDS as i64
    );
}

fn trends(niche: Option<String>) -> Result<()> {
    let filter = niche.map(|value| Niche::parse(&value)).transpose()?;
    println!("\n  Bundled trend profiles (offline, editable)\n");
    for profile in clipfarm_core::trends::all() {
        if let Some(filter) = filter {
            if profile.niche != filter {
                continue;
            }
        }
        println!(
            "  {:<36} {:>4.0}–{:<4.0}s  cut every {:.1}s  {}",
            profile.id,
            profile.min_seconds,
            profile.max_seconds,
            profile.cut_every,
            profile
                .hashtags
                .iter()
                .map(|tag| format!("#{tag}"))
                .collect::<Vec<_>>()
                .join(" ")
        );
        println!("    {}", profile.notes);
    }
    println!(
        "\n  Override them by writing an array of profiles to {}\n",
        data_dir().join("trends.json").display()
    );
    Ok(())
}

fn banner() {
    println!(
        r#"
   ██████╗██╗     ██╗██████╗ ███████╗ █████╗ ██████╗ ███╗   ███╗
  ██╔════╝██║     ██║██╔══██╗██╔════╝██╔══██╗██╔══██╗████╗ ████║
  ██║     ██║     ██║██████╔╝█████╗  ███████║██████╔╝██╔████╔██║
  ██║     ██║     ██║██╔═══╝ ██╔══╝  ██╔══██║██╔══██╗██║╚██╔╝██║
  ╚██████╗███████╗██║██║     ██║     ██║  ██║██║  ██║██║ ╚═╝ ██║
   ╚═════╝╚══════╝╚═╝╚═╝     ╚═╝     ╚═╝  ╚═╝╚═╝  ╚═╝╚═╝     ╚═╝
   long video in · shorts out · nothing leaves your machine
"#
    );
}

fn progress(stage: &str, fraction: f32, message: &str) {
    let width = 24;
    let filled = ((fraction.clamp(0.0, 1.0) * width as f32).round() as usize).min(width);
    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(width - filled));
    print!(
        "\r  {bar} {:>3.0}%  {:<10} {:<46}",
        fraction * 100.0,
        stage,
        truncate(message, 46)
    );
    use std::io::Write;
    let _ = std::io::stdout().flush();
    if fraction >= 1.0 {
        println!();
    }
}

fn short_id(id: uuid::Uuid) -> String {
    id.to_string()[..8].to_string()
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let mut truncated: String = text.chars().take(width.saturating_sub(1)).collect();
    truncated.push('…');
    truncated
}
